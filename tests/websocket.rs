//! P7 — JMAP over WebSocket (RFC 8887), driven through `jmap_client`'s own WS
//! API against the `mock-jmap` server.
//!
//! This is the one surface the wiremock fixture layer cannot reach: a WebSocket
//! is a bidirectional, long-lived conversation, not a request/response pair. The
//! mock's HTTP dispatcher is the *same* code that answers `POST /jmap`, so what
//! these tests pin down is the framing, correlation and push semantics of
//! `client_ws.rs` rather than JMAP method behaviour (P3–P6 cover that).
//!
//! ## The ordering rule this file obeys (plan Finding #22)
//!
//! `WebSocketPushEnable` / `WebSocketPushDisable` get **no reply of their own**
//! (RFC 8887 §4.3.5). A push published immediately after one can therefore
//! overtake it and be dropped before the session is switched on (or after it is
//! switched off). Every enable/disable here is followed by [`sync`]: a
//! `Mailbox/get` whose `Response` proves the socket has already processed
//! everything sent before it. `wait_for_subscribers()` alone is *not* enough —
//! it shows the socket is attached to the bus, not that push is enabled.
//!
//! ## EmailPush lives here, not on SSE (plan Finding #16)
//!
//! `filter_notification()` in `src/event_source/parser.rs` returns
//! `Error::Internal("Unexpected PushObject variant")` for anything that is not
//! `StateChange`/`CalendarAlert`, so `EmailPush` is only assertable through the
//! client over the WebSocket leg. That is what [`email_push_of_a_fixture_email_
//! arrives_as_a_notification`] and its sibling do.
//!
//! Every wait is bounded: a notification that never arrives fails the test in
//! `WAIT` instead of hanging the suite.

// Written against the async WebSocket API: `client_ws` needs `websockets`, and `blocking` turns every call synchronous.
#![cfg(all(feature = "async", feature = "websockets", not(feature = "blocking")))]
use std::time::Duration;

use futures_util::StreamExt;
use jmap_client::client::Client;
use jmap_client::client_ws::{WebSocketMessage, WebSocketPushObject};
use jmap_client::core::error::{JMAPError, ProblemType};
use jmap_client::core::request::Request;
use jmap_client::{DataType, Error, Method, PushObject, URI};
use mock_jmap::{fixture_authorization, MockJmapServer};

/// Generous upper bound for "this should have arrived by now".
const WAIT: Duration = Duration::from_secs(5);
/// Window used to prove that nothing arrives.
const QUIET: Duration = Duration::from_millis(300);

/// The fixture account the mock seeds and advertises.
const ACCOUNT: &str = "account1";

/// The exact message `client_ws.rs` produces when the socket was never opened.
const NOT_CONNECTED: &str = "Websocket stream not set.";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn connected_client(server: &MockJmapServer) -> Client {
    Client::new()
        .credentials(fixture_authorization())
        .connect(server.base_url())
        .await
        .expect("connect to the mock session")
}

/// The stream type `connect_ws` hands back, spelled for the helper bounds.
type WsStream<S> = S;

/// The stream type `connect_ws` hands back, spelled for the helper bounds.
///
/// `connect_ws` returns `Pin<Box<impl Stream<…>>>`, which is `Unpin`, so the
/// helpers below stay generic over the item type only.
type WsItem = jmap_client::Result<WebSocketMessage>;

/// Waits for the next frame and unwraps it, failing loudly on error/timeout.
async fn recv<S>(stream: &mut S) -> WebSocketMessage
where
    S: futures_util::Stream<Item = WsItem> + Unpin,
{
    match tokio::time::timeout(WAIT, stream.next()).await {
        Ok(Some(Ok(message))) => message,
        Ok(Some(Err(err))) => panic!("websocket stream yielded an error: {err}"),
        Ok(None) => panic!("the websocket closed without a frame"),
        Err(_) => panic!("no frame within {WAIT:?}"),
    }
}

/// Waits for the next frame and returns the error it produced.
async fn recv_err<S>(stream: &mut S) -> Error
where
    S: futures_util::Stream<Item = jmap_client::Result<WebSocketMessage>> + Unpin,
{
    match tokio::time::timeout(WAIT, stream.next()).await {
        Ok(Some(Err(err))) => err,
        Ok(Some(Ok(message))) => panic!("expected an error frame, got {message:?}"),
        Ok(None) => panic!("the websocket closed without an error frame"),
        Err(_) => panic!("no error frame within {WAIT:?}"),
    }
}

/// Asserts the stream stays silent for [`QUIET`].
async fn expect_no_frame<S>(stream: &mut S)
where
    S: futures_util::Stream<Item = jmap_client::Result<WebSocketMessage>> + Unpin,
{
    if let Ok(got) = tokio::time::timeout(QUIET, stream.next()).await {
        panic!("expected no frame, got {got:?}");
    }
}

/// Sends a `Mailbox/get` and waits for its `Response` frame.
///
/// The synchronisation frame of Finding #22: the socket is ordered, so once
/// this reply is in hand every `WebSocketPushEnable`/`WebSocketPushDisable`
/// sent earlier has been processed by the mock.
async fn sync<S>(client: &Client, stream: &mut WsStream<S>)
where
    S: futures_util::Stream<Item = jmap_client::Result<WebSocketMessage>> + Unpin,
{
    let request_id = send_mailbox_get(client).await;
    let message = recv(stream).await;
    match message {
        WebSocketMessage::Response(response) => assert_eq!(
            response.request_id(),
            Some(request_id.as_str()),
            "the synchronising request must be answered with its own requestId"
        ),
        other => panic!("expected the synchronising Response, got {other:?}"),
    }
}

/// Sends one `Mailbox/get` and returns the `requestId` the client assigned.
async fn send_mailbox_get(client: &Client) -> String {
    let mut request = Request::new(client);
    request.get_mailbox();
    client.send_ws(request).await.expect("send_ws Mailbox/get")
}

/// Waits until `want` streams are attached to the mock's push bus.
async fn wait_for_subscribers(server: &MockJmapServer, want: usize) {
    for _ in 0..500 {
        if server.handle().push_subscriber_count() >= want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "only {} push subscriber(s) attached, wanted {want}",
        server.handle().push_subscriber_count()
    );
}

/// Pulls the RFC 8887 §5.2 WebSocketPush object out of a notification.
fn push_notification(message: WebSocketMessage) -> WebSocketPushObject {
    match message {
        WebSocketMessage::PushNotification(notification) => notification,
        other => panic!("expected a push notification, got {other:?}"),
    }
}

/// Pulls the `changed` map out of a `StateChange` notification.
fn state_change(message: WebSocketMessage) -> ahash::AHashMap<String, ahash::AHashMap<DataType, String>> {
    match push_notification(message).push {
        PushObject::StateChange { changed } => changed,
        other => panic!("expected a StateChange push, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Requests: correlation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_ws_request_is_answered_and_correlated_by_request_id() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = match client.connect_ws().await {
        Ok(stream) => stream,
        Err(err) => panic!("connect_ws failed: {err}"),
    };

    let mut request = Request::new(&client);
    request.get_mailbox();
    let request_id = client.send_ws(request).await.expect("send_ws");
    // `send_ws` numbers requests from a counter that `connect_ws` resets to 0.
    assert_eq!(request_id, "0", "the first request id");

    let message = recv(&mut stream).await;
    match message {
        WebSocketMessage::Response(response) => {
            assert_eq!(
                response.request_id(),
                Some("0"),
                "RFC 8887 4.3.3: the Response carries the requestId of its Request"
            );
            assert_eq!(response.method_responses().len(), 1);
            let tagged = response.method_response_by_id("s0").expect("call id s0");
            assert!(
                tagged.is_type(Method::GetMailbox),
                "the response is the Mailbox/get that was asked for"
            );
            assert!(
                !response.session_state().is_empty(),
                "sessionState is required on every Response (RFC 8620 3.4, Finding 13)"
            );
        }
        other => panic!("expected a Response frame, got {other:?}"),
    }
}

/// `Request::send_ws()` is the convenience wrapper that hands the whole
/// request back to `Client::send_ws` (RFC 8887 §4.3.3). Every other test
/// in this file drives the socket through the client directly, so this one
/// pins the wrapper: it must return the id the connection's counter
/// assigned, and the answer must arrive correlated to *that* id.
#[tokio::test]
async fn request_send_ws_returns_the_id_the_connection_assigned() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");

    let mut request = Request::new(&client);
    request.get_mailbox();
    let request_id = request.send_ws().await.expect("Request::send_ws");

    assert_eq!(
        request_id, "0",
        "the wrapper must hand back the connection's first request id"
    );

    match recv(&mut stream).await {
        WebSocketMessage::Response(response) => assert_eq!(
            response.request_id(),
            Some(request_id.as_str()),
            "the id the wrapper returned is the id the Response is correlated to"
        ),
        other => panic!("expected a Response frame, got {other:?}"),
    }
}

#[tokio::test]
async fn two_sequential_requests_get_distinct_correctly_correlated_ids() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");

    let mut first = Request::new(&client);
    first.get_mailbox();
    let first_id = client.send_ws(first).await.expect("send first");

    let mut second = Request::new(&client);
    second.query_mailbox();
    let second_id = client.send_ws(second).await.expect("send second");

    assert_ne!(
        first_id, second_id,
        "each request gets its own id from the connection's counter"
    );

    // The socket is ordered, so the replies arrive in request order.
    let first_response = recv(&mut stream).await;
    let second_response = recv(&mut stream).await;

    match (first_response, second_response) {
        (
            WebSocketMessage::Response(one),
            WebSocketMessage::Response(two),
        ) => {
            assert_eq!(one.request_id(), Some(first_id.as_str()));
            assert_eq!(two.request_id(), Some(second_id.as_str()));
            assert!(
                one.method_response_by_id("s0")
                    .is_some_and(|r| r.is_type(Method::GetMailbox)),
                "the first reply answers the Mailbox/get"
            );
            assert!(
                two.method_response_by_id("s0")
                    .is_some_and(|r| r.is_type(Method::QueryMailbox)),
                "the second reply answers the Mailbox/query"
            );
        }
        other => panic!("expected two Response frames, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Push: enable / disable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enable_push_ws_delivers_a_state_change_to_this_client() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect("enable_push_ws");
    // The enable has no reply of its own; the echo proves it was processed.
    sync(&client, &mut stream).await;

    server.handle().trigger_state_change("Mailbox", "11");

    let changed = state_change(recv(&mut stream).await);
    assert_eq!(
        changed
            .get(ACCOUNT)
            .and_then(|types| types.get(&DataType::Mailbox))
            .map(|state| state.as_str()),
        Some("11"),
        "the pushed TypeState is exactly what the mock published"
    );
}

#[tokio::test]
async fn enable_push_ws_with_data_types_omits_the_types_that_were_not_asked_for() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    client
        .enable_push_ws(Some(vec![DataType::Mailbox]), None::<String>)
        .await
        .expect("enable_push_ws(Mailbox)");
    sync(&client, &mut stream).await;

    // One change that touches two types: only Mailbox may come through
    // (RFC 8887 4.3.5.2 omits unwanted types from the TypeState object).
    let mut inner = std::collections::BTreeMap::new();
    inner.insert("Mailbox".to_string(), "1".to_string());
    inner.insert("Email".to_string(), "2".to_string());
    let mut changed_map = std::collections::BTreeMap::new();
    changed_map.insert(ACCOUNT.to_string(), inner);
    server.handle().broadcast_state_change(changed_map);

    let changed = state_change(recv(&mut stream).await);
    let types = changed.get(ACCOUNT).expect("the fixture account");
    assert_eq!(types.get(&DataType::Mailbox).map(|s| s.as_str()), Some("1"));
    assert!(
        types.get(&DataType::Email).is_none(),
        "Email was not in dataTypes, so it is omitted: {types:?}"
    );

    // A change of an unwanted type alone sends nothing at all.
    server.handle().trigger_state_change("Email", "3");
    expect_no_frame(&mut stream).await;
}

#[tokio::test]
async fn disable_push_ws_stops_delivery_to_this_client() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect("enable_push_ws");
    sync(&client, &mut stream).await;

    server.handle().trigger_state_change("Mailbox", "1");
    let changed = state_change(recv(&mut stream).await);
    assert_eq!(
        changed
            .get(ACCOUNT)
            .and_then(|types| types.get(&DataType::Mailbox))
            .map(|s| s.as_str()),
        Some("1"),
        "push works while enabled"
    );

    client.disable_push_ws().await.expect("disable_push_ws");
    // Same rule on the way out: the disable needs a synchronising reply before
    // a trigger can be said to have arrived after it.
    sync(&client, &mut stream).await;

    server.handle().trigger_state_change("Mailbox", "2");
    expect_no_frame(&mut stream).await;
}

#[tokio::test]
async fn a_socket_that_never_enabled_push_receives_nothing() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    server.handle().trigger_state_change("Mailbox", "1");
    expect_no_frame(&mut stream).await;

    // ...while a request on the same socket still works.
    sync(&client, &mut stream).await;
}

// ---------------------------------------------------------------------------
// Push: EmailPush (WebSocket-only in practice, Finding #16)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_push_of_a_fixture_email_arrives_as_a_notification() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect("enable_push_ws");
    sync(&client, &mut stream).await;

    server
        .handle()
        .push_email_of("em_1")
        .expect("the fixture email em_1 exists");

    let message = recv(&mut stream).await;
    match message {
        WebSocketMessage::PushNotification(WebSocketPushObject {
            push: PushObject::EmailPush { account_id, email },
            push_state,
        }) => {
            assert_eq!(account_id, ACCOUNT);
            assert_eq!(email["id"], "em_1");
            assert!(
                email["mailboxIds"].is_object(),
                "the push carries the RFC 8621 4.1.1 metadata a client needs"
            );
            assert!(
                push_state.is_some(),
                "RFC 8887 5.2: every WebSocketPush object carries a pushState"
            );
        }
        other => panic!("expected an EmailPush notification, got {other:?}"),
    }
}

#[tokio::test]
async fn push_email_with_an_explicit_payload_arrives_verbatim() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;

    client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect("enable_push_ws");
    sync(&client, &mut stream).await;

    server.handle().push_email(serde_json::json!({
        "id": "em_fresh",
        "mailboxIds": { "mb_inbox": true },
        "keywords": {},
        "receivedAt": "2026-02-01T10:00:00Z",
        "blobId": "blob_fresh",
        "size": 512,
    }));

    let message = recv(&mut stream).await;
    match message {
        WebSocketMessage::PushNotification(WebSocketPushObject {
            push: PushObject::EmailPush { account_id, email },
            push_state,
        }) => {
            assert_eq!(account_id, ACCOUNT);
            assert_eq!(email["id"], "em_fresh");
            assert_eq!(email["blobId"], "blob_fresh");
            assert_eq!(email["size"], 512);
            assert!(
                push_state.is_some(),
                "RFC 8887 5.2: the pushState travels with the notification"
            );
        }
        other => panic!("expected an EmailPush notification, got {other:?}"),
    }
}

/// RFC 8887 §5.2 — the WebSocketPush object is a PushNotification **plus**
/// `pushState: String|null`, the id of the last event. That token exists so a
/// client can resume push after reconnecting (RFC 8620 §7.1 `Last-Event-ID`).
/// This drives the mock's WebSocket leg end to end: read the state off a live
/// push, reconnect with it, and get the event that was missed while the socket
/// was down.
#[tokio::test]
async fn a_push_notification_surfaces_its_push_state_and_resumes_after_a_reconnect() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    // First connection: two changes, and the client records the state of the
    // first one — as if the socket died right after it.
    let mut stream = client.connect_ws().await.expect("connect_ws");
    wait_for_subscribers(&server, 1).await;
    client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect("enable_push_ws");
    sync(&client, &mut stream).await;

    server.handle().trigger_state_change("Mailbox", "1");
    let first = push_notification(recv(&mut stream).await);
    assert!(
        matches!(first.push, PushObject::StateChange { .. }),
        "the notification itself is still reachable through `.push`: {:?}",
        first.push
    );
    let resume_state = first
        .push_state
        .expect("RFC 8887 §5.2 puts pushState on every WebSocketPush object");
    assert!(
        !resume_state.is_empty(),
        "the resume token is a non-empty last-event id"
    );

    // A second change arrives after that token was taken, so resuming from it
    // must replay exactly this one.
    server.handle().trigger_state_change("Mailbox", "2");
    let second = push_notification(recv(&mut stream).await);
    assert_ne!(
        second.push_state.as_deref(),
        Some(resume_state.as_str()),
        "each push carries its own, later, pushState"
    );
    drop(stream);

    // Reconnect and resume from the token read off the live push.
    let mut stream = client.connect_ws().await.expect("reconnect_ws");
    wait_for_subscribers(&server, 1).await;
    client
        .enable_push_ws(None::<Vec<DataType>>, Some(resume_state.as_str()))
        .await
        .expect("enable_push_ws with a pushState to replay from");

    let replayed = state_change(recv(&mut stream).await);
    assert_eq!(
        replayed
            .get(ACCOUNT)
            .and_then(|types| types.get(&DataType::Mailbox))
            .map(String::as_str),
        Some("2"),
        "resuming from the pushState read off the live push replays the missed event"
    );
}

// ---------------------------------------------------------------------------
// Keepalive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_ping_round_trips_without_breaking_the_stream() {
    // The client sends a Ping and tungstenite answers it with a Pong inside the
    // library; `client_ws.rs` only yields *text* frames, so the pong must be
    // swallowed silently and the connection must go on serving requests.
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");

    client.ws_ping().await.expect("ws_ping");

    // The very next frame must be the Response to this request: no error, no
    // phantom item produced by the pong, and no hang.
    let request_id = send_mailbox_get(&client).await;
    match recv(&mut stream).await {
        WebSocketMessage::Response(response) => {
            assert_eq!(response.request_id(), Some(request_id.as_str()));
        }
        other => panic!("expected the Response after the ping, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Error frames
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_request_error_frame_surfaces_as_problem_details() {
    // `using` a capability the server never advertised. Over the socket this is
    // a RequestError *frame*, never a close (RFC 8887 4.3.4), and the client
    // maps it through `From<WebSocketError> for ProblemDetails`.
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");

    let mut request = Request::new(&client);
    // The mock advertises core/mail/submission/vacationresponse/websocket/sieve/
    // principals/blob/quota/mdn; Contacts is not among them, and
    // `Request::new` would have filtered it out — `add_capability` forces it.
    request.add_capability(URI::Contacts);
    request.get_mailbox();
    let request_id = client.send_ws(request).await.expect("send_ws");

    let err = recv_err(&mut stream).await;
    match err {
        Error::Problem(problem) => {
            assert!(
                matches!(
                    problem.error(),
                    ProblemType::JMAP(JMAPError::UnknownCapability)
                ),
                "expected unknownCapability, got {:?}",
                problem.error()
            );
            assert_eq!(problem.status(), Some(400));
            assert!(problem.title().is_some(), "the problem explains itself");
            assert_eq!(
                problem.request_id(),
                Some(request_id.as_str()),
                "the frame's requestId survives into ProblemDetails"
            );
        }
        other => panic!("expected Error::Problem, got {other:?}"),
    }

    // And the connection is still usable: a request-level error is not a close.
    sync(&client, &mut stream).await;
}

#[tokio::test]
async fn a_method_error_still_arrives_as_a_response_frame() {
    // Contrast with the above: an in-band method error is a normal Response
    // whose payload is an error, not a RequestError frame.
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client.connect_ws().await.expect("connect_ws");

    server
        .handle()
        .inject_method_error("Mailbox/get", "serverFail", 1);

    let request_id = send_mailbox_get(&client).await;
    let message = recv(&mut stream).await;
    match message {
        WebSocketMessage::Response(mut response) => {
            assert_eq!(response.request_id(), Some(request_id.as_str()));
            // `TaggedMethodResponse` is not `Clone` (Finding 25), so read what
            // is needed by reference first, then take ownership.
            assert!(
                response
                    .method_response_by_id("s0")
                    .is_some_and(|r| r.is_error()),
                "the call answered with a method error"
            );
            let tagged = response.method_response_by_pos(0);
            match tagged.unwrap_method_response() {
                jmap_client::core::response::MethodResponse::Error(err) => {
                    assert_eq!(
                        err.error(),
                        &jmap_client::core::error::MethodErrorType::ServerFail
                    );
                }
                other => panic!("expected a MethodResponse::Error, got {other:?}"),
            }
        }
        other => panic!("expected a Response frame carrying the method error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Errors before the socket exists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_ws_before_connect_ws_reports_the_stream_is_not_set() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    let err = client
        .send_ws(Request::new(&client))
        .await
        .expect_err("no socket has been opened");
    assert!(
        matches!(&err, Error::Internal(msg) if msg == NOT_CONNECTED),
        "got {err:?}"
    );
}

#[tokio::test]
async fn push_and_ping_before_connect_ws_report_the_stream_is_not_set() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    let err = client
        .enable_push_ws(None::<Vec<DataType>>, None::<String>)
        .await
        .expect_err("no socket has been opened");
    assert!(
        matches!(&err, Error::Internal(msg) if msg == NOT_CONNECTED),
        "enable_push_ws: {err:?}"
    );

    let err = client
        .disable_push_ws()
        .await
        .expect_err("no socket has been opened");
    assert!(
        matches!(&err, Error::Internal(msg) if msg == NOT_CONNECTED),
        "disable_push_ws: {err:?}"
    );

    let err = client
        .ws_ping()
        .await
        .expect_err("no socket has been opened");
    assert!(
        matches!(&err, Error::Internal(msg) if msg == NOT_CONNECTED),
        "ws_ping: {err:?}"
    );
}

#[tokio::test]
async fn connect_ws_against_a_session_without_a_websocket_capability_fails() {
    // The session decides: no `urn:ietf:params:jmap:websocket` block means
    // there is nothing to upgrade to, and the client says so before touching
    // the network.
    let server = MockJmapServer::start();
    let mut session = server.handle().session_json();
    session["capabilities"]
        .as_object_mut()
        .expect("capabilities")
        .remove("urn:ietf:params:jmap:websocket");
    server.handle().inject_raw(
        "/.well-known/jmap",
        200,
        "application/json",
        session.to_string().as_bytes(),
        1,
    );

    let client = connected_client(&server).await;
    assert!(
        !client.session().has_capability(URI::WebSocket),
        "the fixture session really has no websocket capability"
    );

    match client.connect_ws().await {
        Ok(_) => panic!("connect_ws must refuse a session with no websocket capability"),
        Err(err) => assert!(
            matches!(
                &err,
                Error::Internal(msg)
                    if msg == "JMAP server does not advertise any websocket capabilities."
            ),
            "got {err:?}"
        ),
    }
}

#[tokio::test]
async fn the_mock_records_the_websocket_handshake_request() {
    // Cheap proof that `connect_ws` really talked to `/ws` and offered the
    // `jmap` subprotocol (the mock would have refused otherwise).
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let _stream = client.connect_ws().await.expect("connect_ws");

    let log = server.requests_log();
    let handshake = log
        .iter()
        .find(|record| record.method == "GET" && record.path == "/ws")
        .expect("the upgrade request was logged");
    assert_eq!(handshake.query, None, "the ws url carries no query");
}
