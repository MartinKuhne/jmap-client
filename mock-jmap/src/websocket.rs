//! `GET /ws` — the RFC 8887 JMAP-over-WebSocket leg.
//!
//! One connection carries three things:
//!
//! * **Requests** (§4.3.2) — the same envelope as `POST /jmap`, so
//!   [`crate::dispatch::dispatch_request`] answers it and the two transports
//!   cannot drift apart.
//! * **Push** (§4.3.5) — `WebSocketPushEnable` / `WebSocketPushDisable` switch
//!   the socket on and off; `pushState` is the resume token.
//! * **Request-level errors** (§4.3.4) — a `RequestError` *frame*, never a
//!   close. A bad capability or a frame that is not JSON must leave the
//!   connection usable.
//!
//! The handshake itself (§4.2) is checked before the upgrade: the client has to
//! offer the `jmap` subprotocol and authenticate, or the mock answers a plain
//! HTTP problem document instead of `101 Switching Protocols`.
//!
//! Deviations, both in the plan's Findings log:
//!
//! * A binary frame is ignored rather than answered. RFC 8887 §4.3.1 allows
//!   either ignoring it or closing with 1003; ignoring keeps the connection
//!   alive so a test can assert the next text request still works.
//! * `sessionState` is always present on a `Response` frame. The RFC 8887 §4.4
//!   example omits it, but RFC 8620 §3.4 requires it and `jmap_client`'s
//!   `WebSocketResponse` deserialises it as a non-optional `String`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use futures_util::{
    sink::{Sink, SinkExt},
    stream::StreamExt,
};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as WsRequest, Response as WsResponse,
};
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::dispatch::{self, RequestProblem};
use crate::http::{self, HttpRequest, PrefixedStream, RawRequest};
use crate::problem::problem_response;
use crate::problem::problem_type::{
    MISSING_SUBPROTOCOL, NOT_JSON, NOT_REQUEST, NOT_WEBSOCKET_UPGRADE, UNAUTHORIZED,
};
use crate::push::{Changed, PushBus, PushEvent};
use crate::Shared;

/// The subprotocol RFC 8887 §6.1 registers; the handshake fails without it.
pub const SUBPROTOCOL: &str = "jmap";

/// Serves the WebSocket upgrade on a request the HTTP reader already parsed.
///
/// `raw.head` (and `raw.leftover`, in case the client pipelined its first frame
/// into the same read) are replayed to `tokio_tungstenite` through
/// [`PrefixedStream`], so the library parses the handshake from the exact bytes
/// that arrived instead of the mock inventing a second HTTP parser.
pub async fn serve(shared: Arc<Mutex<Shared>>, stream: TcpStream, raw: RawRequest) {
    let request = &raw.request;

    if !is_upgrade(request) {
        let mut stream = stream;
        let _ = http::write_response(
            &mut stream,
            &problem_response(
                400,
                NOT_WEBSOCKET_UPGRADE,
                "Not a WebSocket upgrade request",
                Some("Send `Upgrade: websocket` and `Connection: Upgrade` to open the JMAP WebSocket"),
            ),
        )
        .await;
        return;
    }

    // RFC 8887 §4.1: the handshake request is authenticated, and those
    // credentials stand for the life of the connection.
    if !request.is_authenticated() {
        let mut stream = stream;
        let _ = http::write_response(
            &mut stream,
            &problem_response(
                401,
                UNAUTHORIZED,
                "Missing or invalid Authorization header",
                Some("RFC 8887 4.1: authenticate the handshake request"),
            ),
        )
        .await;
        return;
    }

    if !offers_subprotocol(request) {
        let mut stream = stream;
        let _ = http::write_response(
            &mut stream,
            &problem_response(
                400,
                MISSING_SUBPROTOCOL,
                "Client did not offer the jmap subprotocol",
                Some("Include `Sec-WebSocket-Protocol: jmap` in the upgrade request"),
            ),
        )
        .await;
        return;
    }

    let mut prefix = raw.head.clone();
    prefix.extend_from_slice(&raw.leftover);

    let Ok(socket) = accept_hdr_async(PrefixedStream::new(prefix, stream), echo_subprotocol).await
    else {
        return;
    };

    let bus = shared
        .lock()
        .expect("mock-jmap: shared state poisoned")
        .push
        .clone();
    // Live events only; anything older comes from history when the client
    // enables push with a `pushState` token.
    let mut receiver = bus.subscribe(None).0;
    let mut session = Session::new();

    let (mut sink, mut incoming) = socket.split();

    loop {
        tokio::select! {
            frame = incoming.next() => {
                match frame {
                    None | Some(Err(_)) => break,
                    Some(Ok(Message::Close(_))) => {
                        let _ = sink.close().await;
                        break;
                    }
                    // tungstenite queues the auto-pong while reading; without
                    // this flush it would only go out with the next message.
                    Some(Ok(Message::Ping(_))) => {
                        let _ = sink.flush().await;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Text(text))) => {
                        if handle_text(&shared, &bus, &mut sink, &mut session, text.as_str())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(Message::Frame(_))) => {}
                }
            }
            event = receiver.recv() => {
                match event {
                    Ok(event) => {
                        if let Some(frame) = session.filter(&event) {
                            if sink.send(Message::text(frame.to_string())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    let _ = sink.close().await;
}

/// Adds the `jmap` subprotocol the client asked for (§4.2: the reply "MUST also
/// contain a corresponding `Sec-WebSocket-Protocol` header field").
#[allow(clippy::result_large_err)] // Required by tokio-tungstenite's handshake callback signature.
fn echo_subprotocol(
    _request: &WsRequest,
    response: WsResponse,
) -> Result<WsResponse, ErrorResponse> {
    let mut response = response;
    response.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static(SUBPROTOCOL),
    );
    Ok(response)
}

/// `true` when the request is a WebSocket upgrade (`Upgrade: websocket` plus a
/// `Connection` header that carries `upgrade`).
///
/// The accept loop asks this before routing, because an upgrade is not a request
/// that gets a response: the socket changes hands.
pub fn is_upgrade(request: &HttpRequest) -> bool {
    let upgrades = request.header("upgrade").is_some_and(|value| {
        value
            .split(',')
            .any(|item| item.trim().eq_ignore_ascii_case("websocket"))
    });
    let keeps = request.header("connection").is_some_and(|value| {
        value
            .split(',')
            .any(|item| item.trim().eq_ignore_ascii_case("upgrade"))
    });
    upgrades && keeps
}

fn offers_subprotocol(request: &HttpRequest) -> bool {
    request
        .header("sec-websocket-protocol")
        .is_some_and(|value| {
            value
                .split(',')
                .any(|item| item.trim().eq_ignore_ascii_case(SUBPROTOCOL))
        })
}

/// Per-connection push state (§4.3.5).
#[derive(Debug, Default)]
struct Session {
    /// Whether `WebSocketPushEnable` has been seen and not undone.
    enabled: bool,
    /// `dataTypes`: `None` means every type.
    types: Option<BTreeSet<String>>,
    /// Highest event id already delivered, so a replay and the live stream can
    /// never send the same notification twice.
    last_sent: u64,
}

impl Session {
    fn new() -> Self {
        Self::default()
    }

    /// The frame to send for `event`, or `None` when this socket does not want
    /// it.
    ///
    /// RFC 8887 §4.3.5.2 wants the *contents* filtered, not just the event:
    /// types the client did not ask for are "omitted from the TypeState object".
    fn filter(&mut self, event: &PushEvent) -> Option<Value> {
        if !self.enabled || event.id() <= self.last_sent {
            return None;
        }
        let frame = match (&self.types, event) {
            (None, _) => event.to_ws_frame(),
            (Some(wanted), PushEvent::StateChange(change)) => {
                let mut changed: Changed = BTreeMap::new();
                for (account, types) in &change.changed {
                    let kept: BTreeMap<String, String> = types
                        .iter()
                        .filter(|(data_type, _)| wanted.contains(*data_type))
                        .map(|(data_type, state)| (data_type.clone(), state.clone()))
                        .collect();
                    if !kept.is_empty() {
                        changed.insert(account.clone(), kept);
                    }
                }
                if changed.is_empty() {
                    return None;
                }
                json!({
                    "@type": "StateChange",
                    "changed": changed,
                    "pushState": event.push_state(),
                })
            }
            (Some(wanted), PushEvent::EmailPush(_)) => {
                if wanted.contains("Email") {
                    event.to_ws_frame()
                } else {
                    return None;
                }
            }
        };
        self.last_sent = event.id();
        Some(frame)
    }
}

/// Handles one text frame from the client.
async fn handle_text<S>(
    shared: &Arc<Mutex<Shared>>,
    bus: &PushBus,
    sink: &mut S,
    session: &mut Session,
    text: &str,
) -> Result<(), WsError>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        // RFC 8887 4.4: a frame that is not JSON is answered with a
        // RequestError *frame*, and the connection stays open.
        let problem = RequestProblem::new(
            400,
            NOT_JSON,
            "The request did not parse as I-JSON.",
            Some(text.chars().take(200).collect()),
        );
        return sink
            .send(Message::text(problem.to_ws_frame().to_string()))
            .await;
    };

    match root.get("@type").and_then(Value::as_str) {
        Some("Request") => handle_request(shared, sink, root).await,
        Some("WebSocketPushEnable") => {
            session.types = root
                .get("dataTypes")
                .and_then(Value::as_array)
                .map(|types| {
                    types
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<BTreeSet<String>>()
                });
            session.enabled = true;

            // §4.3.5.2: "Upon receipt of a pushState token, the server SHOULD
            // immediately send all changes since that state token."
            if let Some(state) = root.get("pushState").and_then(Value::as_str) {
                if let Ok(after) = state.trim().parse::<u64>() {
                    for event in bus.replay_after(after) {
                        if let Some(frame) = session.filter(&event) {
                            sink.send(Message::text(frame.to_string())).await?;
                        }
                    }
                }
            }
            Ok(())
        }
        Some("WebSocketPushDisable") => {
            session.enabled = false;
            session.types = None;
            Ok(())
        }
        Some(other) => {
            let mut frame = RequestProblem::new(
                400,
                NOT_REQUEST,
                "Unknown WebSocket message type",
                Some(format!(
                    "\"@type\": \"{other}\" is not Request, WebSocketPushEnable or WebSocketPushDisable"
                )),
            )
            .to_ws_frame();
            frame.as_object_mut().expect("frame is an object").insert(
                "requestId".to_string(),
                root.get("id")
                    .and_then(Value::as_str)
                    .map(Value::from)
                    .unwrap_or(Value::Null),
            );
            sink.send(Message::text(frame.to_string())).await
        }
        None => {
            let frame = RequestProblem::new(
                400,
                NOT_REQUEST,
                "WebSocket frame has no \"@type\" member",
                None,
            );
            sink.send(Message::text(frame.to_ws_frame().to_string()))
                .await
        }
    }
}

/// Runs a `Request` frame through the same dispatcher `POST /jmap` uses and
/// answers it with a `Response` (§4.3.3) or `RequestError` (§4.3.4) frame.
async fn handle_request<S>(
    shared: &Arc<Mutex<Shared>>,
    sink: &mut S,
    root: Value,
) -> Result<(), WsError>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    let request_id = root.get("id").and_then(Value::as_str).map(str::to_string);

    // The state lock is taken for the synchronous dispatch only; nothing is
    // awaited while it is held.
    let outcome = {
        let mut guard = shared.lock().expect("mock-jmap: shared state poisoned");
        dispatch::dispatch_request(&mut guard, root)
    };

    let mut frame = match outcome {
        Ok(mut body) => {
            body.as_object_mut()
                .expect("dispatcher returns an object")
                .insert("@type".to_string(), json!("Response"));
            body
        }
        Err(problem) => problem.to_ws_frame(),
    };
    frame.as_object_mut().expect("frame is an object").insert(
        "requestId".to_string(),
        request_id.map(Value::from).unwrap_or(Value::Null),
    );

    sink.send(Message::text(frame.to_string())).await
}
