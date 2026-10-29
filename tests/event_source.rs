//! P7 — the EventSource push stream (RFC 8620 §7.3), driven through
//! `jmap_client`'s own `Client::event_source` against the `mock-jmap` server.
//!
//! Like [`websocket.rs`](super::websocket), this is a surface the wiremock fixture
//! layer cannot reach: the push stream is one long-lived, server-driven response.
//! The mock's `GET /eventSource` leg is real code (its own SSE framing, its own
//! resume history), so what these tests pin down is the *client* half of the
//! protocol: URL templating, the `types`/`closeafter`/`ping` parameters,
//! `Last-Event-ID` resume, and how the parser's output reaches the caller.
//!
//! ## Keepalives are observed from the mock, not the stream
//!
//! `EventParser::filter_notification` drops `event: ping` unless the crate is
//! built with the `debug` feature, so a keepalive is invisible to a caller. The
//! mock counts every ping frame it writes ([`MockHandle::keepalive_count`]),
//! which is the only way to assert one *happened* rather than sleeping for an
//! interval somebody guessed at.
//!
//! ## `EmailPush` cannot be consumed here (plan Finding #16)
//!
//! The SSE parser only knows `StateChange` and `CalendarAlert`; anything else is
//! reported as `Error::Internal("Unexpected PushObject variant: …")`.
//! [`email_push_over_sse_is_reported_as_an_unsupported_variant`] pins that down
//! — including the part that matters most: the stream *survives* the error and
//! keeps delivering. Real `EmailPush` delivery is asserted over the WebSocket in
//! `tests/websocket.rs`.
//!
//! Every wait is bounded: a notification that never arrives fails in [`WAIT`]
//! instead of hanging the suite.

use std::time::Duration;

use ahash::AHashMap;
use futures_util::StreamExt;
use jmap_client::client::Client;
use jmap_client::event_source::{Changes, PushNotification};
use jmap_client::{DataType, Error};
use mock_jmap::{fixture_authorization, MockJmapServer};

/// Generous upper bound for "this should have arrived by now".
const WAIT: Duration = Duration::from_secs(5);

/// The fixture account the mock seeds and advertises.
const ACCOUNT: &str = "account1";

/// The item type of the stream `Client::event_source` returns.
type SseItem = jmap_client::Result<PushNotification>;

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

/// Waits until `want` streams are attached to the mock's push bus.
///
/// Unlike the WebSocket case (plan Finding #22) there is nothing to
/// synchronise here: `event_source` registers the subscription on the mock
/// side before it writes a single byte, so once this returns every later
/// publish reaches this stream.
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

/// Waits until no stream is attached to the mock's push bus any more.
async fn wait_for_no_subscribers(server: &MockJmapServer) {
    for _ in 0..500 {
        if server.handle().push_subscriber_count() == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{} stream(s) still attached to the bus, wanted none",
        server.handle().push_subscriber_count()
    );
}

/// Waits until the mock has written `want` `ping` keepalive frames.
async fn wait_for_keepalive(server: &MockJmapServer, want: usize) {
    for _ in 0..1000 {
        if server.handle().keepalive_count() >= want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "only {} keepalive(s) written, wanted {want}",
        server.handle().keepalive_count()
    );
}

/// Pulls the next notification out of the stream, failing loudly otherwise.
async fn next_notification<S>(stream: &mut S) -> PushNotification
where
    S: futures_util::Stream<Item = SseItem> + Unpin,
{
    match tokio::time::timeout(WAIT, stream.next()).await {
        Ok(Some(Ok(notification))) => notification,
        Ok(Some(Err(err))) => panic!("event stream yielded an error: {err}"),
        Ok(None) => panic!("the event stream ended without a notification"),
        Err(_) => panic!("no notification within {WAIT:?}"),
    }
}

/// Pulls the next error out of the stream, failing loudly otherwise.
async fn next_error<S>(stream: &mut S) -> Error
where
    S: futures_util::Stream<Item = SseItem> + Unpin,
{
    match tokio::time::timeout(WAIT, stream.next()).await {
        Ok(Some(Err(err))) => err,
        Ok(Some(Ok(notification))) => panic!("expected an error, got {notification:?}"),
        Ok(None) => panic!("the event stream ended without an error"),
        Err(_) => panic!("no error within {WAIT:?}"),
    }
}

/// Asserts the stream has ended (`None`), which is how `closeafter=state` and a
/// server-side hangup reach the client.
async fn expect_end<S>(stream: &mut S)
where
    S: futures_util::Stream<Item = SseItem> + Unpin,
{
    match tokio::time::timeout(WAIT, stream.next()).await {
        Ok(None) => (),
        Ok(Some(Ok(notification))) => panic!("expected the stream to end, got {notification:?}"),
        Ok(Some(Err(err))) => panic!("expected the stream to end, got error {err}"),
        Err(_) => panic!("the stream is still open after {WAIT:?}"),
    }
}

/// The `StateChange` payload of a notification, or a panic that says what came
/// back instead.
fn into_changes(notification: PushNotification) -> Changes {
    match notification {
        PushNotification::StateChange(changes) => changes,
        other => panic!("expected a StateChange notification, got {other:?}"),
    }
}

/// The `account1 -> { type -> state }` view of a notification, as plain data.
fn changed_types(changes: &Changes) -> Vec<(String, String)> {
    changes
        .changes(ACCOUNT)
        .map(|iter| {
            iter.map(|(ty, state)| (ty.to_string(), state.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// The query string the mock saw on `GET /eventSource`.
fn event_source_query(server: &MockJmapServer) -> String {
    server
        .handle()
        .requests_log()
        .iter()
        .find(|record| record.path == "/eventSource")
        .and_then(|record| record.query.clone())
        .expect("the mock recorded the GET /eventSource request")
}

// ---------------------------------------------------------------------------
// Delivering a StateChange
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_state_change_arrives_with_its_event_id_account_and_types() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client
        .event_source(None::<Vec<DataType>>, false, None, None)
        .await
        .expect("event_source");

    wait_for_subscribers(&server, 1).await;
    let pushed = server.handle().trigger_state_change("Mailbox", "42");

    let mut changes = into_changes(next_notification(&mut stream).await);

    // The SSE `id:` field is the mock's monotonic event id, and the client
    // surfaces it as the `Changes` id.
    assert_eq!(
        changes.id(),
        Some(pushed.to_string()).as_deref(),
        "the notification carries the event id the mock assigned"
    );
    assert_eq!(
        changes.changed_accounts().cloned().collect::<Vec<_>>(),
        vec![ACCOUNT.to_string()],
        "account1 is the only account that changed"
    );
    assert_eq!(
        changed_types(&changes),
        vec![("Mailbox".to_string(), "42".to_string())],
        "Mailbox changed to the state that was published"
    );
    assert!(changes.has_type(DataType::Mailbox));
    assert!(!changes.has_type(DataType::Email));
    assert!(!changes.is_empty());

    // `account_changes` takes the entry out of the map: that is how a caller
    // drains a notification account by account.
    let drained = changes
        .account_changes(ACCOUNT)
        .expect("the account is present the first time");
    assert_eq!(drained.get(&DataType::Mailbox).map(String::as_str), Some("42"));
    assert!(
        changes.account_changes(ACCOUNT).is_none(),
        "account_changes removes what it returns"
    );
    assert!(changes.is_empty(), "a drained notification is empty");
    assert!(changes.into_inner().is_empty());
}

#[tokio::test]
async fn the_types_parameter_hides_the_types_that_were_not_asked_for() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client
        .event_source(Some(vec![DataType::Mailbox]), false, None, None)
        .await
        .expect("event_source(types=Mailbox)");

    wait_for_subscribers(&server, 1).await;

    // Published first, but filtered out by the server. Asking for the next
    // *wanted* event instead of sleeping for silence keeps this deterministic:
    // if the filter were broken, the first notification would be the Email one.
    server.handle().trigger_state_change("Email", "1");
    let mailbox_event = server.handle().trigger_state_change("Mailbox", "2");

    let changes = into_changes(next_notification(&mut stream).await);
    assert_eq!(
        changes.id(),
        Some(mailbox_event.to_string()).as_deref(),
        "the Email change must not have been delivered to a Mailbox-only stream"
    );
    assert_eq!(changed_types(&changes), vec![("Mailbox".to_string(), "2".to_string())]);
    assert!(!changes.has_type(DataType::Email));
}

// ---------------------------------------------------------------------------
// closeafter=state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn closeafter_state_ends_the_stream_after_the_first_change() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client
        .event_source(None::<Vec<DataType>>, true, None, None)
        .await
        .expect("event_source(closeafter=state)");

    wait_for_subscribers(&server, 1).await;
    let pushed = server.handle().trigger_state_change("Mailbox", "1");

    let changes = into_changes(next_notification(&mut stream).await);
    assert_eq!(
        changes.id(),
        Some(pushed.to_string()).as_deref(),
        "the closing event is the state change itself, not a bare hangup"
    );

    // The mock finishes the chunked body and shuts the socket down; the client
    // sees that as the end of the stream, and the mock's receiver goes away
    // with it, so nothing is subscribed any more.
    expect_end(&mut stream).await;
    wait_for_no_subscribers(&server).await;
}

// ---------------------------------------------------------------------------
// ping keepalive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_ping_keepalive_is_written_and_the_stream_keeps_delivering() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    // One second: the smallest interval the client's `Option<u32>` can ask for,
    // and well inside the mock's floor of 20 ms.
    let mut stream = client
        .event_source(None::<Vec<DataType>>, false, Some(1), None)
        .await
        .expect("event_source(ping=1)");

    wait_for_subscribers(&server, 1).await;

    // The keepalive is proven by the mock's counter, because the client's
    // parser swallows ping events unless the `debug` feature is on.
    wait_for_keepalive(&server, 1).await;

    // And the stream is still a working stream afterwards.
    let pushed = server.handle().trigger_state_change("Mailbox", "7");
    let changes = into_changes(next_notification(&mut stream).await);
    assert_eq!(changes.id(), Some(pushed.to_string()).as_deref());
    assert_eq!(changed_types(&changes), vec![("Mailbox".to_string(), "7".to_string())]);
}

// ---------------------------------------------------------------------------
// Last-Event-ID resume
// ---------------------------------------------------------------------------

#[tokio::test]
async fn last_event_id_replays_only_what_the_client_missed() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    // Published while nobody is listening; the mock keeps them in its history
    // ring precisely so a reconnecting client can catch up.
    let first = server.handle().trigger_state_change("Mailbox", "1");
    let second = server.handle().trigger_state_change("Email", "2");
    assert_eq!((first, second), (1, 2), "the bus numbers events from 1");

    let mut stream = client
        .event_source(None::<Vec<DataType>>, false, None, Some("1"))
        .await
        .expect("event_source(Last-Event-ID: 1)");

    wait_for_subscribers(&server, 1).await;

    // Only id 2 is replayed; id 1 is the client's own resume point.
    let replayed = into_changes(next_notification(&mut stream).await);
    assert_eq!(replayed.id(), Some("2"));
    assert_eq!(changed_types(&replayed), vec![("Email".to_string(), "2".to_string())]);

    // Live delivery continues from the resume point with no gap and no overlap.
    let third = server.handle().trigger_state_change("Identity", "3");
    let live = into_changes(next_notification(&mut stream).await);
    assert_eq!(live.id(), Some(third.to_string()).as_deref());
    assert_eq!(changed_types(&live), vec![("Identity".to_string(), "3".to_string())]);
}

#[tokio::test]
async fn a_last_event_id_ahead_of_the_bus_replays_nothing() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    server.handle().trigger_state_change("Mailbox", "1");

    let mut stream = client
        .event_source(None::<Vec<DataType>>, false, None, Some("99"))
        .await
        .expect("event_source(Last-Event-ID: 99)");

    wait_for_subscribers(&server, 1).await;

    // Nothing was replayed, so the first notification the client sees is the
    // one published after it subscribed.
    let later = server.handle().trigger_state_change("Mailbox", "2");
    let changes = into_changes(next_notification(&mut stream).await);
    assert_eq!(
        changes.id(),
        Some(later.to_string()).as_deref(),
        "a resume token newer than the history must not replay old events"
    );
}

// ---------------------------------------------------------------------------
// URL templating
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_session_template_is_filled_with_types_closeafter_and_ping() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    let _stream = client
        .event_source(
            Some(vec![DataType::Mailbox, DataType::Email]),
            true,
            Some(45),
            None,
        )
        .await
        .expect("event_source");

    wait_for_subscribers(&server, 1).await;

    let query = event_source_query(&server);
    assert_eq!(
        query, "types=Mailbox,Email&closeafter=state&ping=45",
        "the client fills the session's eventSourceUrl template in order, \
         with the RFC 8620 7.3 parameter names"
    );
}

#[tokio::test]
async fn the_defaults_are_everything_no_close_and_no_keepalive() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;

    let _stream = client
        .event_source(None::<Vec<DataType>>, false, None, None)
        .await
        .expect("event_source defaults");

    wait_for_subscribers(&server, 1).await;

    let query = event_source_query(&server);
    assert_eq!(
        query, "types=*&closeafter=no&ping=0",
        "no types means `*`, close_after_state=false means `no`, and no ping \
         means 0 (the server then picks its own interval)"
    );
}

// ---------------------------------------------------------------------------
// Finding #16: EmailPush is not consumable over SSE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_push_over_sse_is_reported_as_an_unsupported_variant() {
    let server = MockJmapServer::start();
    let client = connected_client(&server).await;
    let mut stream = client
        .event_source(None::<Vec<DataType>>, false, None, None)
        .await
        .expect("event_source");

    wait_for_subscribers(&server, 1).await;
    server
        .handle()
        .push_email_of("em_1")
        .expect("the fixture email em_1 exists");

    let err = next_error(&mut stream).await;
    assert!(
        err.to_string()
            .contains("Unexpected PushObject variant"),
        "the SSE parser rejects EmailPush; got: {err}"
    );

    // The important half: one undecodable event does not kill the stream.
    let pushed = server.handle().trigger_state_change("Mailbox", "5");
    let changes = into_changes(next_notification(&mut stream).await);
    assert_eq!(changes.id(), Some(pushed.to_string()).as_deref());
    assert_eq!(
        changed_types(&changes),
        vec![("Mailbox".to_string(), "5".to_string())]
    );
}

// ---------------------------------------------------------------------------
// Changes, on its own
// ---------------------------------------------------------------------------

#[test]
fn changes_accessors_round_trip_a_hand_built_state_change() {
    let mut types = AHashMap::new();
    types.insert(DataType::Mailbox, "1".to_string());
    types.insert(DataType::Email, "2".to_string());
    let mut by_account = AHashMap::new();
    by_account.insert(ACCOUNT.to_string(), types);
    by_account.insert("account2".to_string(), AHashMap::new());

    let changes = Changes::new(Some("7".into()), by_account);

    assert_eq!(changes.id(), Some("7"));
    assert!(changes.has_type(DataType::Mailbox));
    assert!(changes.has_type(DataType::Email));
    assert!(!changes.has_type(DataType::Thread));

    let mut accounts = changes
        .changed_accounts()
        .map(|account| account.as_str())
        .collect::<Vec<_>>();
    accounts.sort_unstable();
    assert_eq!(accounts, vec!["account1", "account2"]);

    // An account whose map is empty does not make the notification non-empty:
    // `is_empty` looks at the type maps, not the account keys.
    assert!(!changes.is_empty());
    assert_eq!(changes.changes("account2").map(|iter| iter.count()), Some(0));
    assert_eq!(changes.changes("nope").map(|iter| iter.count()), None);
    assert_eq!(changes.into_inner().len(), 2);
}

#[test]
fn an_empty_changes_value_is_empty_and_has_no_id() {
    let changes = Changes::new(None, AHashMap::new());
    assert!(changes.is_empty());
    assert_eq!(changes.id(), None);
    assert!(!changes.has_type(DataType::Mailbox));
    assert_eq!(
        changes.changed_accounts().count(),
        0,
        "no accounts, no changes"
    );
}
