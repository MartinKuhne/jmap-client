//! `GET /eventSource` — the RFC 8620 §7.3 push leg (Server-Sent Events).
//!
//! The stream is a `text/event-stream` body written chunk by chunk, one SSE
//! event per chunk, each flushed the moment it is produced. Every event carries
//! an `id:` field so a client that drops can come back with `Last-Event-ID` and
//! be replayed from [`crate::push::PushBus`] history.
//!
//! [`crate::Shared::split_sse_frames`] turns that framing off for a test: each
//! frame is then written as two chunks, the way a real transport cuts a frame at
//! an arbitrary byte.
//!
//! Request parameters (RFC 8620 §7.3):
//!
//! * `types=*` — push every data type; `types=Mailbox,Email` — push only those.
//! * `closeafter=state` — close the stream after the first `StateChange`.
//! * `ping=<seconds>` — keepalive interval the client asks for.
//!
//! Two deliberate deviations from the letter of the RFC, both recorded in the
//! plan's Findings log:
//!
//! * The event name is `StateChange`, not the RFC's `state`. The name is not
//!   significant to `jmap_client`, which treats anything that is not `ping` or
//!   `calendarAlert` as a state event; using the `@type` name keeps the SSE and
//!   RFC 8887 frames symmetric.
//! * `ping` accepts a fractional number of seconds (`ping=0.2`) so tests do
//!   not have to wait minutes. The RFC models an integer.
//!
//! There is no unconditional greeting event: RFC 8620 §7.3 sends a
//! `StateChange` when something *changed*, not when a listener appears. What
//! the mock writes on open instead is the [`LIVENESS`] comment — invisible to
//! the parser, but enough for a test to know the stream is up.

use std::collections::BTreeSet;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::Instant;

use crate::http::{self, HttpRequest, HttpResponse};
use crate::push::PushEvent;
use crate::Shared;

/// Written as soon as the stream opens.
///
/// A comment, not an event: it proves the connection is live without claiming
/// anything changed. SSE comments start with `:` and `jmap_client`'s parser
/// drops them.
pub const LIVENESS: &str = ": mock-jmap event stream open\r\n";

/// Keepalive interval used when the client does not ask for one
/// (RFC 8620 §7.3 suggests five minutes).
pub const DEFAULT_PING: Duration = Duration::from_secs(300);

/// Floor for the keepalive interval. A client asking for `ping=0` would
/// otherwise turn the stream task into a busy loop.
pub const MIN_PING: Duration = Duration::from_millis(20);

/// Which data types a stream is interested in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeFilter {
    /// `types=*` (or no `types` parameter): everything.
    All,
    /// Only events that touch one of these type names.
    Only(BTreeSet<String>),
}

impl TypeFilter {
    /// Parses the `types` parameter. Empty and `*` mean "all".
    pub fn parse(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty() || value == "*" {
            return TypeFilter::All;
        }
        TypeFilter::Only(
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty() && *item != "*")
                .map(str::to_string)
                .collect(),
        )
    }

    /// `true` when an event with these data types should be delivered.
    pub fn matches(&self, types: &BTreeSet<String>) -> bool {
        match self {
            TypeFilter::All => true,
            TypeFilter::Only(wanted) => types.iter().any(|ty| wanted.contains(ty)),
        }
    }
}

/// The parsed query string of an EventSource request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseParams {
    /// The `types=` filter.
    pub types: TypeFilter,
    /// `closeafter=state`: end the stream after the first `StateChange`.
    pub close_after_state: bool,
    /// The keepalive interval to advertise.
    pub ping: Duration,
}

impl Default for SseParams {
    fn default() -> Self {
        Self {
            types: TypeFilter::All,
            close_after_state: false,
            ping: DEFAULT_PING,
        }
    }
}

impl SseParams {
    /// Reads `types`, `closeafter` and `ping` out of a raw query string.
    ///
    /// Unknown parameters are ignored, and a parameter with an unparsable value
    /// falls back to its default rather than failing the request: the RFC gives
    /// the server latitude here and a silent default keeps tests focused.
    pub fn parse(query: Option<&str>) -> Self {
        let mut params = SseParams::default();
        let Some(query) = query else {
            return params;
        };
        for (key, value) in http::parse_query(query) {
            match key.as_str() {
                "types" => params.types = TypeFilter::parse(&value),
                "closeafter" => params.close_after_state = value.eq_ignore_ascii_case("state"),
                "ping" => {
                    if let Ok(seconds) = value.trim().parse::<f64>() {
                        if seconds.is_finite() && seconds > 0.0 {
                            params.ping = Duration::from_secs_f64(seconds).max(MIN_PING);
                        }
                    }
                }
                _ => {}
            }
        }
        params
    }

    /// `true` when this stream wants the given event.
    pub fn matches(&self, event: &PushEvent) -> bool {
        self.types.matches(&event.data_types())
    }
}

/// The `ping` keepalive event (RFC 8620 §7.3).
///
/// Deliberately has no `id:` field: a keepalive is not a state change, so it
/// must not become a resume point.
pub fn ping_frame(interval: Duration) -> String {
    format!(
        "event: ping\r\ndata: {{\"interval\":{}}}\r\n\r\n",
        format_seconds(interval)
    )
}

/// `300` for five minutes, `0.2` for the test interval — never `300.0`.
fn format_seconds(interval: Duration) -> String {
    let seconds = interval.as_secs_f64();
    if seconds.fract() == 0.0 {
        format!("{}", seconds as u64)
    } else {
        format!("{seconds}")
    }
}

/// The routing marker for `GET /eventSource`.
///
/// A streaming [`HttpResponse`] whose first chunk is [`LIVENESS`]; the caller
/// hands the socket to [`serve`] instead of writing a fixed body.
pub fn route_marker() -> HttpResponse {
    HttpResponse::streaming(200, LIVENESS.as_bytes().to_vec())
}

/// Serves one EventSource stream on an already-read request.
///
/// `greeting` is written first (the mock sends [`LIVENESS`]), then every event
/// the client missed since its `Last-Event-ID`, then the live stream until the
/// client goes away, the bus closes, or `closeafter=state` fires.
///
/// The state lock is taken only to clone the [`PushBus`]; the subscription is
/// registered before the first byte is written, so no event can be missed.
pub async fn serve(
    shared: &Arc<Mutex<Shared>>,
    stream: &mut TcpStream,
    request: &HttpRequest,
    greeting: &[u8],
) -> io::Result<()> {
    let params = SseParams::parse(request.query.as_deref());
    // RFC 8620 §7.3: a client that reconnects says where it got to.
    let since = request
        .header("last-event-id")
        .and_then(|value| value.trim().parse::<u64>().ok());

    let (mut receiver, replay, bus, split_frames) = {
        let guard = shared.lock().expect("mock-jmap: shared state poisoned");
        let (receiver, replay) = guard.push.subscribe(since);
        (receiver, replay, guard.push.clone(), guard.split_sse_frames)
    };

    http::write_streaming_head(stream).await?;
    if !greeting.is_empty() {
        http::write_chunk(stream, greeting).await?;
    }

    for event in &replay {
        if params.matches(event) {
            send(stream, event, split_frames).await?;
            if params.close_after_state && event.is_state_change() {
                return http::finish_stream(stream).await;
            }
        }
    }

    let mut ticker = tokio::time::interval_at(Instant::now() + params.ping, params.ping);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            received = receiver.recv() => match received {
                Ok(event) => {
                    if params.matches(&event) {
                        send(stream, &event, split_frames).await?;
                        if params.close_after_state && event.is_state_change() {
                            break;
                        }
                    }
                }
                // Too far behind to be useful: end the stream so the client
                // resumes from its Last-Event-ID instead of a hole.
                Err(broadcast::error::RecvError::Lagged(_)) => break,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = ticker.tick() => {
                http::write_chunk(stream, ping_frame(params.ping).as_bytes()).await?;
                // Counted *after* the write lands, so `keepalive_count() >= 1`
                // always means the frame is already on the wire.
                bus.record_keepalive();
            }
        }
    }

    http::finish_stream(stream).await?;
    // The stream is over: hand the write half back so the client sees EOF
    // rather than an idle socket.
    let _ = stream.shutdown().await;
    Ok(())
}

/// Writes one SSE event.
///
/// With `split_frames` the frame is cut in half and written as two HTTP
/// chunks, so the client's parser is handed a half-read `data:` field and
/// must carry it over into the next read (RFC 8620 §7.1: the read boundary
/// is not an event boundary).
async fn send(stream: &mut TcpStream, event: &PushEvent, split_frames: bool) -> io::Result<()> {
    let frame = event.to_sse();
    if !split_frames {
        return http::write_chunk(stream, frame.as_bytes()).await;
    }

    // Cut inside the `data:` payload when there is one, so the parser is
    // stopped mid-value; otherwise straight down the middle.
    let cut = frame
        .find("changed")
        .or_else(|| frame.find('{'))
        .unwrap_or(frame.len() / 2);
    let (head, tail) = frame.split_at(cut.max(1));
    http::write_chunk(stream, head.as_bytes()).await?;
    http::write_chunk(stream, tail.as_bytes()).await
}
