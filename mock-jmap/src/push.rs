//! The push fan-out bus.
//!
//! One mutation, many listeners. [`PushBus`] lives inside [`crate::Shared`], so a
//! test thread can call [`crate::MockHandle::mutate_from_elsewhere`] (the
//! `mejl-fake` pattern) and every open EventSource stream and every
//! push-enabled WebSocket sees the same [`PushEvent`].
//!
//! Two event shapes exist:
//!
//! * [`PushEvent::StateChange`] — the `StateChange` object of RFC 8620 §7.1,
//!   which is also the only push object RFC 8887 §4.3.5.1 allows over the
//!   WebSocket (there extended with `pushState`).
//! * [`PushEvent::EmailPush`] — the `EmailPush` variant of
//!   `jmap_client::PushObject`. RFC 8621 has no such object (its §1.5 adds the
//!   `EmailDelivery` *type*, not a new push payload), so this is emitted only
//!   when a test asks for it explicitly. See the plan's Findings log for the
//!   client-side limitation that goes with it.
//!
//! ## Ids and `pushState`
//!
//! Every published event gets a monotonically increasing id from the bus. That
//! id is what the mock writes into the SSE `id:` field, and it is also the
//! `pushState` token of RFC 8887 §4.3.5.1: a client that reconnects with
//! `Last-Event-ID: 7` (EventSource) or `WebSocketPushEnable { pushState: "7" }`
//! receives exactly the events with id > 7 that are still in the history ring.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

/// How many recent events the bus remembers for resume (`Last-Event-ID`,
/// `pushState`). Older events are dropped and a resume from them starts clean.
pub const HISTORY_CAPACITY: usize = 128;

/// Broadcast channel capacity. A stream that falls further behind than this is
/// lagged; the mock treats that as "skip, keep going".
pub const CHANNEL_CAPACITY: usize = 256;

/// The `changed` map of a `StateChange`: `accountId -> { DataType -> state }`.
pub type Changed = BTreeMap<String, BTreeMap<String, String>>;

/// A `StateChange` push object (RFC 8620 §7.1 / RFC 8887 §4.3.5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    /// Monotonic event id; also the `pushState` token.
    pub id: u64,
    /// Which types changed, per account, and their new state strings.
    pub changed: Changed,
}

/// An `EmailPush` notification (`jmap_client::PushObject::EmailPush`).
///
/// `email` carries the RFC 8621 §4.1.1 metadata a client needs to decide
/// whether to fetch: `id`, `mailboxIds`, `keywords`, `receivedAt`, `blobId`
/// and `size`.
#[derive(Debug, Clone, PartialEq)]
pub struct EmailPush {
    /// Monotonic event id; also the `pushState` token.
    pub id: u64,
    /// The account the message landed in.
    pub account_id: String,
    /// The pushed email metadata.
    pub email: Value,
}

/// One notification travelling on the bus.
#[derive(Debug, Clone, PartialEq)]
pub enum PushEvent {
    /// A data-type state change.
    StateChange(StateChange),
    /// A newly delivered message.
    EmailPush(EmailPush),
}

impl PushEvent {
    /// A `StateChange` for an explicit `changed` map.
    pub fn state_change(changed: Changed) -> Self {
        PushEvent::StateChange(StateChange { id: 0, changed })
    }

    /// A `StateChange` for one data type of one account.
    pub fn state_change_one(account_id: &str, data_type: &str, new_state: &str) -> Self {
        let mut inner = BTreeMap::new();
        inner.insert(data_type.to_string(), new_state.to_string());
        let mut changed = BTreeMap::new();
        changed.insert(account_id.to_string(), inner);
        PushEvent::state_change(changed)
    }

    /// An `EmailPush` for `account_id`.
    pub fn email_push(account_id: impl Into<String>, email: Value) -> Self {
        PushEvent::EmailPush(EmailPush {
            id: 0,
            account_id: account_id.into(),
            email,
        })
    }

    /// The event id (assigned by [`PushBus::publish`]).
    pub fn id(&self) -> u64 {
        match self {
            PushEvent::StateChange(change) => change.id,
            PushEvent::EmailPush(push) => push.id,
        }
    }

    /// Stamps the id. Called by [`PushBus::publish`]; callers pass 0.
    pub fn set_id(&mut self, id: u64) {
        match self {
            PushEvent::StateChange(change) => change.id = id,
            PushEvent::EmailPush(push) => push.id = id,
        }
    }

    /// `true` for a `StateChange`. Only those satisfy `closeafter=state`
    /// (RFC 8620 §7.3).
    pub fn is_state_change(&self) -> bool {
        matches!(self, PushEvent::StateChange(_))
    }

    /// The JMAP data type names this event concerns, used for the `types=`
    /// filter (EventSource) and `dataTypes` (RFC 8887 §4.3.5.2).
    pub fn data_types(&self) -> BTreeSet<String> {
        match self {
            PushEvent::StateChange(change) => change
                .changed
                .values()
                .flat_map(|types| types.keys().cloned())
                .collect(),
            PushEvent::EmailPush(_) => BTreeSet::from(["Email".to_string()]),
        }
    }

    /// The `pushState` token for this event (RFC 8887 §4.3.5.1).
    ///
    /// The mock uses the event id: it is short, monotonic, and the only thing
    /// the mock's push history is ordered by, so a client round-tripping it
    /// resumes exactly where it left off.
    pub fn push_state(&self) -> String {
        self.id().to_string()
    }

    /// The JMAP `PushObject` JSON (`@type` tagged, as `jmap_client::PushObject`
    /// deserialises it).
    pub fn to_push_object(&self) -> Value {
        match self {
            PushEvent::StateChange(change) => json!({
                "@type": "StateChange",
                "changed": change.changed,
            }),
            PushEvent::EmailPush(push) => json!({
                "@type": "EmailPush",
                "accountId": push.account_id,
                "email": push.email,
            }),
        }
    }

    /// The RFC 8887 §4.3.5.1 push frame: the `StateChange` object plus
    /// `pushState`.
    pub fn to_ws_frame(&self) -> Value {
        let mut frame = self.to_push_object();
        frame
            .as_object_mut()
            .unwrap()
            .insert("pushState".to_string(), json!(self.push_state()));
        frame
    }

    /// The SSE event name for this push.
    ///
    /// RFC 8620 §7.3 calls the state event "state"; `jmap_client`'s parser
    /// treats *any* name other than `ping`/`calendarAlert` as a state event, so
    /// the mock uses the `@type` name for symmetry with RFC 8887.
    pub fn sse_event_name(&self) -> &'static str {
        match self {
            PushEvent::StateChange(_) => "StateChange",
            PushEvent::EmailPush(_) => "EmailPush",
        }
    }

    /// The complete SSE event, ready to go into one chunk.
    ///
    /// The JSON payload is a single line on purpose: `jmap_client`'s parser
    /// concatenates successive `data:` fields with no separator, so a
    /// multi-line payload would be reassembled without its newlines.
    pub fn to_sse(&self) -> String {
        format!(
            "event: {}\r\nid: {}\r\ndata: {}\r\n\r\n",
            self.sse_event_name(),
            self.id(),
            self.to_push_object()
        )
    }
}

/// The bus: a live broadcast channel plus the resume history ring.
///
/// Cheap to clone — every clone shares the same [`Arc`], so a handle, a
/// connection task and the accept loop all see one bus. Ids are assigned and
/// history appended under a single mutex, and
/// [`PushBus::subscribe`] takes the replay set under that same lock so no
/// event can be both missed and duplicated at the join.
#[derive(Debug, Clone)]
pub struct PushBus {
    inner: Arc<Mutex<BusInner>>,
}

#[derive(Debug)]
struct BusInner {
    tx: broadcast::Sender<PushEvent>,
    history: VecDeque<PushEvent>,
    next_id: u64,
}

impl Default for PushBus {
    fn default() -> Self {
        Self::new()
    }
}

impl PushBus {
    /// An empty bus; the first published event has id 1.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            inner: Arc::new(Mutex::new(BusInner {
                tx,
                history: VecDeque::new(),
                next_id: 1,
            })),
        }
    }

    /// Attaches a live subscriber and returns the events to replay before it.
    ///
    /// `since` is the client's resume token: the `Last-Event-ID` header of an
    /// EventSource request (RFC 8620 §7.3) or the `pushState` of a
    /// `WebSocketPushEnable` (RFC 8887 §4.3.5.1). `None` means "start fresh,
    /// replay nothing".
    ///
    /// The receiver only carries events published *after* this call, so the
    /// caller writes `replay` first and then drains the receiver: the two sets
    /// cannot overlap and cannot have a gap.
    pub fn subscribe(
        &self,
        since: Option<u64>,
    ) -> (broadcast::Receiver<PushEvent>, Vec<PushEvent>) {
        let guard = self.inner.lock().expect("mock-jmap: push bus poisoned");
        let rx = guard.tx.subscribe();
        let replay = match since {
            Some(after) => guard
                .history
                .iter()
                .filter(|event| event.id() > after)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        (rx, replay)
    }

    /// Assigns the next id, records the event and wakes every subscriber.
    /// Returns the id that was assigned.
    ///
    /// `broadcast::Sender::send` never blocks, so this is safe to call while
    /// holding the mock's state lock: a slow stream cannot stall a mutation.
    pub fn publish(&self, mut event: PushEvent) -> u64 {
        let mut guard = self.inner.lock().expect("mock-jmap: push bus poisoned");
        let id = guard.next_id;
        guard.next_id += 1;
        event.set_id(id);
        guard.history.push_back(event.clone());
        if guard.history.len() > HISTORY_CAPACITY {
            guard.history.pop_front();
        }
        // `send` only fails when nobody is listening, which is fine.
        let _ = guard.tx.send(event);
        id
    }

    /// Every remembered event with an id strictly greater than `after`.
    pub fn replay_after(&self, after: u64) -> Vec<PushEvent> {
        self.inner
            .lock()
            .expect("mock-jmap: push bus poisoned")
            .history
            .iter()
            .filter(|event| event.id() > after)
            .cloned()
            .collect()
    }

    /// Id of the most recently published event, or 0 when nothing was ever sent.
    pub fn last_id(&self) -> u64 {
        self.inner
            .lock()
            .expect("mock-jmap: push bus poisoned")
            .next_id
            .saturating_sub(1)
    }

    /// Number of live subscribers (open EventSource streams and WS sockets
    /// that have not dropped their receiver).
    pub fn subscriber_count(&self) -> usize {
        self.inner
            .lock()
            .expect("mock-jmap: push bus poisoned")
            .tx
            .receiver_count()
    }
}

/// Builds the `email` member of an `EmailPush` from a stored Email record,
/// keeping the RFC 8621 §4.1.1 metadata a client needs to decide whether to
/// fetch the whole message.
pub fn email_push_payload(email: &Value) -> Value {
    let mut payload = Map::new();
    for key in [
        "id",
        "mailboxIds",
        "keywords",
        "receivedAt",
        "blobId",
        "size",
    ] {
        if let Some(value) = email.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(payload)
}
