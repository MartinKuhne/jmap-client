//! P2 self-tests: the EventSource (RFC 8620 §7.3) and WebSocket (RFC 8887)
//! legs of the mock.
//!
//! Both are driven from the *client* side of a real socket — a hand-written
//! chunked SSE reader here, `tokio-tungstenite` there — so what these tests
//! pin down is the wire format, not the mock's internals.
//!
//! Every wait is bounded. A push server that never pushes must fail a test
//! quickly, not hang the suite.

use std::sync::Mutex;
use std::time::Duration;

use futures_util::{sink::SinkExt, stream::StreamExt};
use mock_jmap::{MockHandle, MockJmapServer, PushEvent};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, Response};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

const AUTH: &str = "Basic dGVzdEBleGFtcGxlLm9yZzpzZWNyZXQ=";
const CORE: &str = "urn:ietf:params:jmap:core";
const MAIL: &str = "urn:ietf:params:jmap:mail";
const ACCOUNT: &str = "account1";

/// Generous upper bound for "this should have arrived by now".
const WAIT: Duration = Duration::from_secs(5);
/// Window used to prove nothing arrives.
const QUIET: Duration = Duration::from_millis(300);

// ---------------------------------------------------------------------------
// EventSource helpers
// ---------------------------------------------------------------------------

/// One parsed SSE event.
#[derive(Debug, Clone)]
struct SseEvent {
    name: Option<String>,
    id: Option<String>,
    data: String,
}

impl SseEvent {
    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.data).expect("event data is JSON")
    }
}

/// What the next read on a stream produced.
#[derive(Debug)]
enum Sse {
    Event(SseEvent),
    /// The chunked body ended (zero-length chunk or EOF).
    End,
}

/// A minimal `text/event-stream` reader: HTTP head, then chunked framing, then
/// the SSE line grammar.
struct SseReader {
    stream: TcpStream,
    /// status line and headers of the response
    head: (u16, Vec<(String, String)>),
    /// decoded bytes not yet parsed into an event
    pending: Vec<u8>,
    /// every chunk exactly as it arrived, for tests that assert on framing
    chunks: Vec<Vec<u8>>,
}

impl SseReader {
    async fn open(port: u16, path: &str, headers: &[(&str, String)]) -> Self {
        let mut raw = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        for (name, value) in headers {
            raw.push_str(&format!("{name}: {value}\r\n"));
        }
        raw.push_str("\r\n");

        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .expect("connect to mock");
        stream.write_all(raw.as_bytes()).await.expect("write request");
        stream.flush().await.expect("flush request");

        let mut reader = SseReader {
            stream,
            head: (0, Vec::new()),
            pending: Vec::new(),
            chunks: Vec::new(),
        };
        let head = tokio::time::timeout(WAIT, reader.read_head())
            .await
            .expect("head of the event stream")
            .expect("head is readable");
        reader.head = head;
        reader
    }

    /// Reads the status line and headers up to the blank line.
    async fn read_head(&mut self) -> std::io::Result<(u16, Vec<(String, String)>)> {
        let mut status = 0u16;
        let mut headers = Vec::new();
        loop {
            let line = self.read_line().await?;
            if line.is_empty() {
                return Ok((status, headers));
            }
            if status == 0 {
                status = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse::<u16>().ok())
                    .expect("status line");
            } else if let Some((name, value)) = line.split_once(':') {
                headers.push((
                    name.trim().to_ascii_lowercase(),
                    value.trim().to_string(),
                ));
            }
        }
    }

    async fn read_line(&mut self) -> std::io::Result<String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let n = self.stream.read(&mut byte).await?;
            if n == 0 {
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            if byte[0] == b'\n' {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return Ok(String::from_utf8_lossy(&line).into_owned());
            }
            line.push(byte[0]);
        }
    }

    /// Reads exactly one chunk and returns its decoded bytes.
    async fn read_chunk(&mut self) -> std::io::Result<Vec<u8>> {
        let size_line = self.read_line().await?;
        let size = usize::from_str_radix(size_line.trim(), 16)
            .unwrap_or_else(|err| panic!("chunk size {size_line:?} is not hex: {err}"));
        if size == 0 {
            return Ok(Vec::new());
        }
        let mut data = vec![0u8; size];
        self.stream.read_exact(&mut data).await?;
        let mut crlf = [0u8; 2];
        self.stream.read_exact(&mut crlf).await?;
        assert_eq!(&crlf, b"\r\n", "every chunk is followed by CRLF");
        Ok(data)
    }

    /// The next event, or `Sse::End` when the stream is over.
    async fn next(&mut self) -> Sse {
        loop {
            if let Some(event) = self.take_event() {
                return Sse::Event(event);
            }
            match self.read_chunk().await {
                Ok(chunk) if chunk.is_empty() => return Sse::End,
                Ok(chunk) => {
                    self.chunks.push(chunk.clone());
                    self.pending.extend_from_slice(&chunk);
                }
                Err(_) => return Sse::End,
            }
        }
    }

    /// Pulls one complete event (terminated by a blank line) out of `pending`.
    fn take_event(&mut self) -> Option<SseEvent> {
        let pos = find(&self.pending, b"\r\n\r\n")?;
        let block: Vec<u8> = self.pending.drain(..pos + 4).collect();

        let mut name = None;
        let mut id = None;
        let mut data = String::new();
        for line in String::from_utf8_lossy(&block).split("\r\n") {
            // A comment line contributes nothing; the client drops them too.
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };
            match field {
                "event" => name = Some(value.to_string()),
                "id" => id = Some(value.to_string()),
                "data" => data.push_str(value),
                _ => {}
            }
        }
        if name.is_none() && id.is_none() && data.is_empty() {
            return None;
        }
        Some(SseEvent { name, id, data })
    }

    /// Waits for the next event, failing the test if the stream ends first.
    async fn expect_event(&mut self) -> SseEvent {
        match tokio::time::timeout(WAIT, self.next()).await {
            Ok(Sse::Event(event)) => event,
            Ok(Sse::End) => panic!("event stream ended before an event arrived"),
            Err(_) => panic!("no event within {:?}", WAIT),
        }
    }

    /// Asserts the stream stays silent for `window`.
    async fn expect_silence(&mut self, window: Duration) {
        if let Ok(got) = tokio::time::timeout(window, self.next()).await { panic!("expected silence, got {got:?}") }
    }

    /// Asserts the stream ends (terminating chunk or EOF).
    async fn expect_end(&mut self) {
        match tokio::time::timeout(WAIT, self.next()).await {
            Ok(Sse::End) => (),
            Ok(other) => panic!("expected the stream to end, got {other:?}"),
            Err(_) => panic!("stream never ended"),
        }
    }
}

// `head` lives on the struct so tests can assert on the response framing.
impl SseReader {
    fn status(&self) -> u16 {
        self.head.0
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.head
            .1
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Opens an authenticated EventSource and waits until the mock has attached it
/// to the push bus, so a push after this call cannot be missed.
async fn open_stream(server: &MockJmapServer, query: &str) -> SseReader {
    let mut headers = vec![("Authorization", AUTH.to_string())];
    // Consume the value rather than cloning it: a `Last-Event-ID` must apply to
    // exactly one connection. Leaving it set would let a resume token leak into
    // whatever test the harness happens to run next on this thread.
    if let Some(id) = LAST_EVENT_ID.with(|value| value.lock().unwrap().take()) {
        headers.push(("Last-Event-ID", id));
    }
    let mut reader = SseReader::open(server.port(), &format!("/eventSource{query}"), &headers).await;
    wait_for_subscribers(server.handle(), 1).await;
    // The liveness comment is the first thing on the wire.
    let first = tokio::time::timeout(WAIT, reader.read_chunk())
        .await
        .expect("first chunk")
        .expect("first chunk is readable");
    assert!(
        first.starts_with(b":"),
        "the stream opens with a comment, got {first:?}"
    );
    reader.chunks.push(first);
    reader
}

thread_local! {
    /// `Last-Event-ID` for the next [`open_stream`], set by the resume tests.
    static LAST_EVENT_ID: Mutex<Option<String>> = const { Mutex::new(None) };
}

fn with_last_event_id(id: &str) {
    LAST_EVENT_ID.with(|value| *value.lock().unwrap() = Some(id.to_string()));
}

async fn wait_for_subscribers(handle: MockHandle, want: usize) {
    for _ in 0..500 {
        if handle.push_subscriber_count() >= want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "only {} push subscriber(s) attached, wanted {want}",
        handle.push_subscriber_count()
    );
}

// ---------------------------------------------------------------------------
// WebSocket helpers
// ---------------------------------------------------------------------------

type WsConn = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Connects a WebSocket offering the `jmap` subprotocol and authenticating.
async fn ws_connect(server: &MockJmapServer) -> WsConn {
    ws_connect_with(server, Some(AUTH), Some("jmap")).await
}

/// Performs the upgrade and hands back whatever the server answered.
///
/// `IntoClientRequest` fills in `Host`, `Upgrade`, `Connection`,
/// `Sec-WebSocket-Version` and a fresh `Sec-WebSocket-Key`; the test only adds
/// the two headers RFC 8887 cares about.
async fn try_connect(
    server: &MockJmapServer,
    auth: Option<&str>,
    subprotocol: Option<&str>,
) -> Result<(WsConn, Response<Option<Vec<u8>>>), WsError> {
    let mut request = server
        .ws_url()
        .into_client_request()
        .expect("a ws:// url becomes an upgrade request");
    if let Some(auth) = auth {
        request
            .headers_mut()
            .insert("Authorization", HeaderValue::from_str(auth).expect("header value"));
    }
    if let Some(subprotocol) = subprotocol {
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_str(subprotocol).expect("header value"),
        );
    }
    tokio::time::timeout(WAIT, connect_async(request))
        .await
        .expect("the handshake attempt finishes")
}

async fn ws_connect_with(
    server: &MockJmapServer,
    auth: Option<&str>,
    subprotocol: Option<&str>,
) -> WsConn {
    let (stream, response) = try_connect(server, auth, subprotocol)
        .await
        .unwrap_or_else(|err| panic!("handshake failed: {err}"));
    assert_eq!(
        response
            .headers()
            .get("sec-websocket-protocol")
            .map(|value| value.to_str().unwrap_or_default()),
        Some("jmap"),
        "RFC 8887 4.2: the reply must echo the jmap subprotocol"
    );
    stream
}

/// Forces the server to have processed everything sent so far.
///
/// The socket is ordered, so once the reply to this `Core/echo` arrives, a
/// `WebSocketPushEnable` sent before it has been handled. Without that, a push
/// from the test thread can overtake the enable frame and be dropped before the
/// session is switched on.
async fn sync(ws: &mut WsConn) {
    let reply = send_and_recv(
        ws,
        request_frame("sync", &[CORE], vec![json!(["Core/echo", {}, "s0"])]),
    )
    .await;
    assert_eq!(
        reply["@type"],
        json!("Response"),
        "the synchronising request must be answered"
    );
}

/// Switches push on for this socket and waits until the server has done so.
async fn enable_push(ws: &mut WsConn, data_types: Value, push_state: Option<&str>) {
    let mut frame = json!({ "@type": "WebSocketPushEnable", "dataTypes": data_types });
    if let Some(push_state) = push_state {
        frame
            .as_object_mut()
            .unwrap()
            .insert("pushState".to_string(), json!(push_state));
    }
    send_frame(ws, frame).await;
    sync(ws).await;
}

/// Switches push off for this socket and waits until the server has done so.
async fn disable_push(ws: &mut WsConn) {
    send_frame(ws, json!({ "@type": "WebSocketPushDisable" })).await;
    sync(ws).await;
}

/// Sends a frame and returns the next text frame as JSON.
async fn send_and_recv(ws: &mut WsConn, frame: Value) -> Value {
    send_frame(ws, frame).await;
    recv_json(ws).await
}

/// Sends a frame that is not answered.
///
/// `WebSocketPushEnable` / `WebSocketPushDisable` get no reply of their own
/// (RFC 8887 4.3.5), so a test that only switches push on or off must not
/// wait for one.
async fn send_frame(ws: &mut WsConn, frame: Value) {
    tokio::time::timeout(WAIT, ws.send(Message::text(frame.to_string())))
        .await
        .expect("send frame")
        .expect("frame sent");
}

async fn recv_json(ws: &mut WsConn) -> Value {
    let text = recv_text(ws).await;
    serde_json::from_str(&text).expect("a JMAP WebSocket frame is JSON")
}

async fn recv_text(ws: &mut WsConn) -> String {
    match tokio::time::timeout(WAIT, ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => text.to_string(),
        Ok(Some(Ok(other))) => panic!("expected a text frame, got {other:?}"),
        Ok(Some(Err(err))) => panic!("websocket read failed: {err}"),
        Ok(None) => panic!("the websocket closed without a frame"),
        Err(_) => panic!("no frame within {:?}", WAIT),
    }
}

async fn expect_no_frame(ws: &mut WsConn) {
    if let Ok(got) = tokio::time::timeout(QUIET, ws.next()).await { panic!("expected no frame, got {got:?}") }
}

fn request_frame(id: &str, using: &[&str], calls: Vec<Value>) -> Value {
    json!({
        "@type": "Request",
        "id": id,
        "using": using,
        "methodCalls": calls,
    })
}

// ---------------------------------------------------------------------------
// EventSource: opening and framing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn eventsource_is_a_chunked_event_stream() {
    let server = MockJmapServer::start();
    let reader = open_stream(&server, "?types=*").await;
    assert_eq!(reader.status(), 200);
    assert_eq!(reader.header("content-type"), Some("text/event-stream"));
    assert_eq!(reader.header("transfer-encoding"), Some("chunked"));
    assert_eq!(reader.header("cache-control"), Some("no-cache"));
}

#[tokio::test]
async fn eventsource_opens_with_a_comment_not_a_hello_event() {
    // RFC 8620 7.3 sends a StateChange when something *changed*. A fresh
    // stream must therefore not invent one; the liveness comment is all there
    // is until a real push happens.
    let server = MockJmapServer::start();
    let reader = open_stream(&server, "?types=*").await;
    assert!(
        reader.chunks[0].starts_with(b":"),
        "first chunk is a comment: {:?}",
        String::from_utf8_lossy(&reader.chunks[0])
    );
    assert!(
        !String::from_utf8_lossy(&reader.chunks[0]).contains("StateChange"),
        "the greeting must not be a state event"
    );
}

// ---------------------------------------------------------------------------
// EventSource: delivery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn eventsource_delivers_a_state_change() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*").await;

    let id = server.handle().trigger_state_change("Mailbox", "42");
    let event = reader.expect_event().await;

    assert_eq!(event.name(), "StateChange");
    assert_eq!(event.id.as_deref(), Some(id.to_string().as_str()));
    assert_eq!(
        event.json()["changed"][ACCOUNT]["Mailbox"],
        json!("42"),
        "the payload is a RFC 8620 7.1 StateChange"
    );
}

#[tokio::test]
async fn eventsource_carries_an_id_for_every_event() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*").await;

    server.handle().trigger_state_change("Mailbox", "1");
    let first = reader.expect_event().await;
    server.handle().trigger_state_change("Email", "2");
    let second = reader.expect_event().await;

    assert_eq!(first.id.as_deref(), Some("1"));
    assert_eq!(second.id.as_deref(), Some("2"));
}

#[tokio::test]
async fn eventsource_types_filter_pushes_only_what_was_asked_for() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=Mailbox&ping=30").await;

    server.handle().trigger_state_change("Email", "1");
    reader.expect_silence(QUIET).await;

    server.handle().trigger_state_change("Mailbox", "2");
    let event = reader.expect_event().await;
    assert_eq!(event.json()["changed"][ACCOUNT]["Mailbox"], json!("2"));
}

#[tokio::test]
async fn eventsource_types_star_is_everything() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*&ping=30").await;

    server.handle().trigger_state_change("Email", "1");
    server.handle().trigger_state_change("Mailbox", "2");
    server.handle().trigger_state_change("Identity", "3");

    assert_eq!(reader.expect_event().await.name(), "StateChange");
    assert_eq!(reader.expect_event().await.name(), "StateChange");
    assert_eq!(reader.expect_event().await.name(), "StateChange");
}

#[tokio::test]
async fn eventsource_a_type_outside_the_filter_never_arrives() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=Mailbox&ping=30").await;

    // One matching, one not: only the matching one may show up.
    server.handle().broadcast_state_change({
        let mut inner = std::collections::BTreeMap::new();
        inner.insert("Email".to_string(), "9".to_string());
        let mut changed = std::collections::BTreeMap::new();
        changed.insert(ACCOUNT.to_string(), inner);
        changed
    });
    reader.expect_silence(QUIET).await;

    server.handle().trigger_state_change("Mailbox", "10");
    let event = reader.expect_event().await;
    assert_eq!(event.json()["changed"][ACCOUNT]["Mailbox"], json!("10"));
}

#[tokio::test]
async fn eventsource_mutate_from_elsewhere_reaches_the_stream() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=Mailbox&ping=30").await;

    let state = server.handle().with_store(|store| store.mailboxes.state_str());
    let pushed = server.handle().mutate_from_elsewhere(|store| {
        // What a `Mailbox/set` handler does: change the record, then record the
        // transition so the collection's state moves.
        let id = store.mailboxes.create(json!({ "name": "From elsewhere" }));
        store.mailboxes.record_change(vec![id], Vec::new(), Vec::new());
        vec!["Mailbox".to_string()]
    });

    let event = reader.expect_event().await;
    let new_state = event.json()["changed"][ACCOUNT]["Mailbox"]
        .as_str()
        .unwrap()
        .to_string();

    assert_ne!(
        new_state, state,
        "the pushed state must be the state the store moved to"
    );
    assert_eq!(
        new_state,
        server.handle().with_store(|store| store.mailboxes.state_str()),
        "and it must match the store exactly"
    );
    assert!(pushed > 0);
}

#[tokio::test]
async fn eventsource_every_open_stream_sees_the_same_event() {
    let server = MockJmapServer::start();
    let mut first = open_stream(&server, "?types=*&ping=30").await;
    let mut second = open_stream(&server, "?types=Mailbox&ping=30").await;
    wait_for_subscribers(server.handle(), 2).await;
    assert_eq!(server.handle().push_subscriber_count(), 2);

    server.handle().trigger_state_change("Mailbox", "7");

    assert_eq!(
        first.expect_event().await.json()["changed"][ACCOUNT]["Mailbox"],
        json!("7")
    );
    assert_eq!(
        second.expect_event().await.json()["changed"][ACCOUNT]["Mailbox"],
        json!("7")
    );
}

#[tokio::test]
async fn eventsource_email_push_is_a_distinct_event() {
    // The mock can emit EmailPush over SSE; whether a given client can *parse*
    // it is a different question (see the plan's Findings log).
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=Email&ping=30").await;

    server.handle().push_email(json!({
        "id": "em_new",
        "mailboxIds": { "mb_inbox": true },
        "keywords": {},
        "receivedAt": "2026-02-01T10:00:00Z",
        "blobId": "blob_new",
        "size": 512,
    }));

    let event = reader.expect_event().await;
    assert_eq!(event.name(), "EmailPush");
    assert_eq!(event.json()["@type"], json!("EmailPush"));
    assert_eq!(event.json()["accountId"], json!(ACCOUNT));
    assert_eq!(event.json()["email"]["id"], json!("em_new"));
}

// ---------------------------------------------------------------------------
// EventSource: keepalive, closeafter, resume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn eventsource_sends_ping_keepalives_without_an_id() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*&ping=0.05").await;

    let first = reader.expect_event().await;
    assert_eq!(first.name(), "ping");
    assert_eq!(
        first.id, None,
        "a keepalive is not a state change and must not be a resume point"
    );
    assert_eq!(first.json()["interval"], json!(0.05));

    let second = reader.expect_event().await;
    assert_eq!(second.name(), "ping", "the keepalive repeats");
}

#[tokio::test]
async fn eventsource_closeafter_state_closes_after_the_first_state_change() {
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*&closeafter=state&ping=30").await;

    server.handle().trigger_state_change("Mailbox", "5");
    let event = reader.expect_event().await;
    assert_eq!(event.name(), "StateChange");

    reader.expect_end().await;
}

#[tokio::test]
async fn eventsource_closeafter_state_ignores_the_liveness_comment() {
    // The greeting is not a state change: the stream must still be open after
    // it, waiting for something real.
    let server = MockJmapServer::start();
    let mut reader = open_stream(&server, "?types=*&closeafter=state&ping=0.05").await;

    // A ping arrives, still not a state change.
    let ping = reader.expect_event().await;
    assert_eq!(ping.name(), "ping");

    server.handle().trigger_state_change("Mailbox", "6");
    assert_eq!(reader.expect_event().await.name(), "StateChange");
    reader.expect_end().await;
}

#[tokio::test]
async fn eventsource_resumes_from_last_event_id() {
    let server = MockJmapServer::start();
    // Two events happen while nobody is listening.
    server.handle().trigger_state_change("Mailbox", "1");
    server.handle().trigger_state_change("Email", "2");
    server.handle().trigger_state_change("Identity", "3");

    // Reconnect saying "I got as far as 1".
    with_last_event_id("1");
    let mut reader = open_stream(&server, "?types=*&ping=30").await;

    let first = reader.expect_event().await;
    assert_eq!(first.id.as_deref(), Some("2"));
    assert_eq!(first.json()["changed"][ACCOUNT]["Email"], json!("2"));

    let second = reader.expect_event().await;
    assert_eq!(second.id.as_deref(), Some("3"));
    assert_eq!(second.json()["changed"][ACCOUNT]["Identity"], json!("3"));

    server.handle().trigger_state_change("Mailbox", "4");
    assert_eq!(reader.expect_event().await.id.as_deref(), Some("4"));
}

#[tokio::test]
async fn eventsource_a_stale_last_event_id_replays_what_history_still_has() {
    let server = MockJmapServer::start();
    server.handle().trigger_state_change("Mailbox", "1");
    server.handle().trigger_state_change("Email", "2");

    // A token from before the beginning of time: replay everything remembered.
    with_last_event_id("99999");
    let mut reader = open_stream(&server, "?types=*&ping=30").await;
    // Nothing matches "after 99999", so the stream is live but empty.
    reader.expect_silence(QUIET).await;

    server.handle().trigger_state_change("Mailbox", "3");
    let event = reader.expect_event().await;
    assert_eq!(event.json()["changed"][ACCOUNT]["Mailbox"], json!("3"));
}

#[tokio::test]
async fn eventsource_without_last_event_id_gets_no_replay() {
    let server = MockJmapServer::start();
    server.handle().trigger_state_change("Mailbox", "1");

    let mut reader = open_stream(&server, "?types=*&ping=30").await;
    reader.expect_silence(QUIET).await;

    server.handle().trigger_state_change("Email", "2");
    let event = reader.expect_event().await;
    assert_eq!(event.id.as_deref(), Some("2"));
}

#[tokio::test]
async fn eventsource_a_dropped_stream_frees_its_subscriber() {
    let server = MockJmapServer::start();
    // A short ping so the mock writes soon and notices the socket is gone.
    let reader = open_stream(&server, "?types=*&ping=0.05").await;
    wait_for_subscribers(server.handle(), 1).await;
    let port = server.port();
    drop(reader);

    // The mock notices the socket is gone as soon as it tries to write to it.
    server.handle().trigger_state_change("Mailbox", "1");
    for _ in 0..500 {
        if server.handle().push_subscriber_count() == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "the dead stream on port {port} is still attached: {}",
        server.handle().push_subscriber_count()
    );
}

// ---------------------------------------------------------------------------
// WebSocket: the handshake
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_echoes_the_jmap_subprotocol() {
    let server = MockJmapServer::start();
    let _ws = ws_connect(&server).await;
    // ws_connect asserts the echo; reaching here is the assertion.
}

#[tokio::test]
async fn websocket_refuses_a_connection_that_offers_no_subprotocol() {
    let server = MockJmapServer::start();
    let err = try_connect(&server, Some(AUTH), None)
        .await
        .expect_err("no subprotocol must be refused");
    match err {
        WsError::Http(response) => {
            assert_eq!(response.status().as_u16(), 400);
            let body = rejection_body(&response);
            assert!(
                body.contains("missingSubprotocol"),
                "the rejection explains itself, got: {body}"
            );
        }
        other => panic!("expected an HTTP rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn websocket_refuses_an_unrelated_subprotocol() {
    let server = MockJmapServer::start();
    let err = try_connect(&server, Some(AUTH), Some("chat"))
        .await
        .expect_err("a non-jmap subprotocol must be refused");
    match err {
        WsError::Http(response) => {
            assert_eq!(response.status().as_u16(), 400);
            assert!(
                rejection_body(&response).contains("missingSubprotocol"),
                "got: {}",
                rejection_body(&response)
            );
        }
        other => panic!("expected an HTTP rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn websocket_requires_authorization() {
    let server = MockJmapServer::start();
    let err = try_connect(&server, None, Some("jmap"))
        .await
        .expect_err("an anonymous handshake must be refused");
    match err {
        WsError::Http(response) => {
            assert_eq!(response.status().as_u16(), 401);
            let body = rejection_body(&response);
            assert!(body.contains("unauthorized"), "got: {body}");
        }
        other => panic!("expected an HTTP rejection, got {other:?}"),
    }
}

/// The problem document a rejected handshake came back with.
fn rejection_body(response: &Response<Option<Vec<u8>>>) -> String {
    response
        .body()
        .as_ref()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// WebSocket: requests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_answers_a_jmap_request() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    let frame = send_and_recv(
        &mut ws,
        request_frame(
            "R1",
            &[CORE, MAIL],
            vec![json!(["Mailbox/get", { "accountId": ACCOUNT }, "c0"])],
        ),
    )
    .await;

    assert_eq!(frame["@type"], json!("Response"));
    assert_eq!(frame["requestId"], json!("R1"), "RFC 8887 4.3.3 echo");
    assert_eq!(frame["methodResponses"][0][0], json!("Mailbox/get"));
    assert_eq!(frame["methodResponses"][0][2], json!("c0"));
    assert!(
        frame["sessionState"].is_string(),
        "sessionState is required (RFC 8620 3.4)"
    );
}

#[tokio::test]
async fn websocket_shares_the_http_dispatchers_semantics() {
    // Same dispatcher, same rules: result references and createdIds work over
    // the socket exactly as they do over POST.
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    let frame = send_and_recv(
        &mut ws,
        request_frame(
            "R2",
            &[CORE, MAIL],
            vec![
                json!([
                    "Mailbox/set",
                    {
                        "accountId": ACCOUNT,
                        "create": { "newbox": { "name": "Via WS", "role": null } }
                    },
                    "c0"
                ]),
                json!([
                    "Mailbox/get",
                    {
                        "accountId": ACCOUNT,
                        "ids": [{ "resultOf": "c0", "name": "Mailbox/set", "path": "/created/newbox/id" }]
                    },
                    "c1"
                ]),
            ],
        ),
    )
    .await;

    assert_eq!(frame["@type"], json!("Response"));
    let created = frame["createdIds"]["newbox"].as_str().expect("createdIds");
    assert!(!created.is_empty());
    assert_eq!(frame["methodResponses"][1][1]["list"][0]["id"], json!(created));
}

#[tokio::test]
async fn websocket_unknown_capability_is_a_request_error_frame() {
    // A request-level failure over the socket is a frame, not a close: the
    // connection has to stay usable (RFC 8887 4.3.4).
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    let frame = send_and_recv(
        &mut ws,
        request_frame(
            "R1",
            &[CORE, "urn:ietf:params:jmap:does-not-exist"],
            vec![json!(["Mailbox/get", { "accountId": ACCOUNT }, "c0"])],
        ),
    )
    .await;

    assert_eq!(frame["@type"], json!("RequestError"));
    assert_eq!(
        frame["type"],
        json!("urn:ietf:params:jmap:error:unknownCapability")
    );
    assert_eq!(frame["status"], json!(400));
    assert_eq!(frame["requestId"], json!("R1"));

    let reply = send_and_recv(
        &mut ws,
        request_frame("R2", &[CORE], vec![json!(["Core/echo", {}, "c0"])]),
    )
    .await;
    assert_eq!(reply["@type"], json!("Response"));
    assert_eq!(reply["requestId"], json!("R2"));
}

#[tokio::test]
async fn websocket_malformed_json_is_a_request_error_frame() {
    // RFC 8887 4.4 spells the type `urn:ietf:params:jmap:error:notJSON` and
    // keeps the connection open.
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    tokio::time::timeout(WAIT, ws.send(Message::text("this is not json")))
        .await
        .expect("send")
        .expect("sent");

    let frame = recv_json(&mut ws).await;
    assert_eq!(frame["@type"], json!("RequestError"));
    assert_eq!(frame["type"], json!("urn:ietf:params:jmap:error:notJSON"));
    assert_eq!(frame["status"], json!(400));

    // Still usable afterwards.
    let reply = send_and_recv(
        &mut ws,
        request_frame("R2", &[CORE], vec![json!(["Core/echo", {}, "c0"])]),
    )
    .await;
    assert_eq!(reply["@type"], json!("Response"));
    assert_eq!(reply["requestId"], json!("R2"));
}

#[tokio::test]
async fn websocket_unknown_message_type_is_a_request_error_frame() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    let frame = send_and_recv(&mut ws, json!({ "@type": "Nonsense", "id": "R9" })).await;
    assert_eq!(frame["@type"], json!("RequestError"));
    assert_eq!(frame["type"], json!("urn:ietf:params:jmap:error:notRequest"));
    assert_eq!(frame["requestId"], json!("R9"));
}

#[tokio::test]
async fn websocket_a_frame_without_a_type_is_a_request_error_frame() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    let frame = send_and_recv(&mut ws, json!({ "using": [CORE], "methodCalls": [] })).await;
    assert_eq!(frame["@type"], json!("RequestError"));
    assert_eq!(frame["type"], json!("urn:ietf:params:jmap:error:notRequest"));
}

#[tokio::test]
async fn websocket_a_binary_frame_is_ignored() {
    // RFC 8887 4.3.1 allows ignoring a binary frame; the connection must go on
    // serving text as if nothing happened.
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    tokio::time::timeout(WAIT, ws.send(Message::binary(vec![1, 2, 3])))
        .await
        .expect("send")
        .expect("sent");

    let reply = send_and_recv(
        &mut ws,
        request_frame("R3", &[CORE], vec![json!(["Core/echo", {}, "c0"])]),
    )
    .await;
    assert_eq!(reply["@type"], json!("Response"));
    assert_eq!(reply["requestId"], json!("R3"));
}

// ---------------------------------------------------------------------------
// WebSocket: push
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_push_enable_delivers_state_changes() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, Value::Null, None).await;

    server.handle().trigger_state_change("Mailbox", "11");

    let frame = recv_json(&mut ws).await;
    assert_eq!(frame["@type"], json!("StateChange"));
    assert_eq!(frame["changed"][ACCOUNT]["Mailbox"], json!("11"));
    assert_eq!(
        frame["pushState"],
        json!("1"),
        "RFC 8887 4.3.5.1: the push carries a resume token"
    );
}

#[tokio::test]
async fn websocket_without_push_enable_gets_nothing() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    server.handle().trigger_state_change("Mailbox", "1");
    expect_no_frame(&mut ws).await;
}

#[tokio::test]
async fn websocket_push_enable_with_push_state_replays() {
    let server = MockJmapServer::start();
    // Two changes happen before the socket exists.
    server.handle().trigger_state_change("Mailbox", "1");
    server.handle().trigger_state_change("Email", "2");

    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    let frame = send_and_recv(
        &mut ws,
        json!({ "@type": "WebSocketPushEnable", "dataTypes": null, "pushState": "1" }),
    )
    .await;

    assert_eq!(frame["@type"], json!("StateChange"));
    assert_eq!(frame["changed"][ACCOUNT]["Email"], json!("2"));
    assert_eq!(frame["pushState"], json!("2"));
    assert!(
        frame["changed"][ACCOUNT].get("Mailbox").is_none(),
        "state 1 was already seen, so it is not replayed"
    );
}

#[tokio::test]
async fn websocket_push_enable_data_types_filters_the_changed_map() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, json!(["Mailbox"]), None).await;

    // A change that includes both types: only Mailbox may come through.
    server.handle().broadcast_state_change({
        let mut inner = std::collections::BTreeMap::new();
        inner.insert("Mailbox".to_string(), "1".to_string());
        inner.insert("Email".to_string(), "2".to_string());
        let mut changed = std::collections::BTreeMap::new();
        changed.insert(ACCOUNT.to_string(), inner);
        changed
    });

    let frame = recv_json(&mut ws).await;
    assert_eq!(frame["@type"], json!("StateChange"));
    assert_eq!(frame["changed"][ACCOUNT]["Mailbox"], json!("1"));
    assert!(
        frame["changed"][ACCOUNT].get("Email").is_none(),
        "RFC 8887 4.3.5.2: unwanted types are omitted from the TypeState object"
    );

    // A change of an unwanted type alone sends nothing at all.
    server.handle().trigger_state_change("Email", "3");
    expect_no_frame(&mut ws).await;
}

#[tokio::test]
async fn websocket_push_disable_stops_delivery() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, Value::Null, None).await;
    server.handle().trigger_state_change("Mailbox", "1");
    let frame = recv_json(&mut ws).await;
    assert_eq!(frame["@type"], json!("StateChange"));

    disable_push(&mut ws).await;
    server.handle().trigger_state_change("Mailbox", "2");
    expect_no_frame(&mut ws).await;
}

#[tokio::test]
async fn websocket_reconnecting_with_push_state_catches_up() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, Value::Null, None).await;
    server.handle().trigger_state_change("Mailbox", "1");
    let frame = recv_json(&mut ws).await;
    let seen = frame["pushState"].as_str().unwrap().to_string();

    // "Drop" the connection and come back with the last token.
    drop(ws);
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    server.handle().trigger_state_change("Email", "2");
    let frame = send_and_recv(
        &mut ws,
        json!({ "@type": "WebSocketPushEnable", "dataTypes": null, "pushState": seen }),
    )
    .await;
    assert_eq!(frame["changed"][ACCOUNT]["Email"], json!("2"));
    assert_ne!(frame["pushState"], json!(seen));
}

#[tokio::test]
async fn websocket_push_and_requests_share_one_connection() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, Value::Null, None).await;
    server.handle().trigger_state_change("Mailbox", "1");
    assert_eq!(recv_json(&mut ws).await["@type"], json!("StateChange"));

    let reply = send_and_recv(
        &mut ws,
        request_frame(
            "R5",
            &[CORE, MAIL],
            vec![json!(["Mailbox/get", { "accountId": ACCOUNT }, "c0"])],
        ),
    )
    .await;
    assert_eq!(reply["@type"], json!("Response"));
    assert_eq!(reply["requestId"], json!("R5"));

    server.handle().trigger_state_change("Email", "2");
    assert_eq!(recv_json(&mut ws).await["@type"], json!("StateChange"));
}

#[tokio::test]
async fn websocket_email_push_frame() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    enable_push(&mut ws, Value::Null, None).await;
    server.handle().push_email_of("em_1").expect("fixture email exists");

    let frame = recv_json(&mut ws).await;
    assert_eq!(frame["@type"], json!("EmailPush"));
    assert_eq!(frame["accountId"], json!(ACCOUNT));
    assert_eq!(frame["email"]["id"], json!("em_1"));
    assert!(frame["email"]["mailboxIds"].is_object());
    assert!(frame["pushState"].is_string());
}

#[tokio::test]
async fn websocket_ping_gets_an_auto_pong() {
    // RFC 6455: a Ping must be answered with a Pong carrying the same payload.
    // tungstenite queues the pong while reading; the mock has to flush it.
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;

    tokio::time::timeout(WAIT, ws.send(Message::Ping(b"are you there".to_vec().into())))
        .await
        .expect("send ping")
        .expect("ping sent");

    match tokio::time::timeout(WAIT, ws.next()).await {
        Ok(Some(Ok(Message::Pong(payload)))
        ) => {
            assert_eq!(payload, b"are you there".to_vec());
        }
        Ok(Some(Ok(other))) => panic!("expected a Pong, got {other:?}"),
        Ok(Some(Err(err))) => panic!("websocket read failed: {err}"),
        Ok(None) => panic!("the websocket closed without a pong"),
        Err(_) => panic!("no pong within {:?}", WAIT),
    }
}

#[tokio::test]
async fn websocket_survives_a_client_close() {
    let server = MockJmapServer::start();
    let mut ws = ws_connect(&server).await;
    wait_for_subscribers(server.handle(), 1).await;

    tokio::time::timeout(WAIT, ws.close(None))
        .await
        .expect("close")
        .expect("closed");

    // The mock releases the subscriber instead of spinning on a dead socket.
    for _ in 0..500 {
        if server.handle().push_subscriber_count() == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "the closed socket is still attached: {}",
        server.handle().push_subscriber_count()
    );
}

// ---------------------------------------------------------------------------
// The bus itself
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_ids_are_monotonic_across_both_shapes() {
    let server = MockJmapServer::start();
    let handle = server.handle();

    let first = handle.trigger_state_change("Mailbox", "a");
    let second = handle.push_email(json!({ "id": "em_x" }));
    let third = handle.push_event(PushEvent::state_change_one(ACCOUNT, "Identity", "c"));

    assert_eq!((first, second, third), (1, 2, 3));
    assert_eq!(handle.last_push_id(), 3);
}

#[tokio::test]
async fn the_shared_bus_is_one_bus() {
    // Handles, connection tasks and the accept loop must all see the same ids.
    let server = MockJmapServer::start();
    let a = server.handle();
    let b = server.handle();

    assert_eq!(a.trigger_state_change("Mailbox", "1"), 1);
    assert_eq!(b.last_push_id(), 1);
    assert_eq!(a.push_email(json!({ "id": "em_y" })), 2);
    assert_eq!(b.last_push_id(), 2);
}

#[tokio::test]
async fn a_stream_that_never_arrives_fails_fast() {
    // Guards the test suite itself: nothing here may hang.
    let server = MockJmapServer::start();
    let started = std::time::Instant::now();
    let mut reader = open_stream(&server, "?types=Mailbox&ping=30").await;
    reader.expect_silence(QUIET).await;
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the suite must not hang on a quiet mock"
    );
}
