//! P3 — changes / query / queryChanges integration tests.
//!
//! Drives `jmap_client::client::Client` against a `wiremock` HTTP server using
//! the shared fixture layer in `tests/common/mod.rs`. Canned payloads are taken
//! **verbatim from the RFC example bodies** wherever the RFC has one (each
//! fixture cites its section); where the RFC specifies a shape but gives no
//! example, the payload is the minimal RFC-correct object for that section.
//!
//! Coverage targets: `core/changes.rs` (`ChangesRequest`, `ChangesResponse`,
//! the `created`/`updated`/`updated_properties` reference helpers),
//! `core/query.rs` (`QueryRequest`, `Filter`/`FilterOperator`, `Comparator`,
//! `QueryResponse`), `core/query_changes.rs` (`QueryChangesRequest`,
//! `QueryChangesResponse`, `AddedItem`) and the changes/query/queryChanges
//! arms of `core/response.rs`.
//!
//! Every test asserts the method name actually went on the wire: a typed
//! `send_*` helper transmits only the calls already added to the builder, so a
//! bare `client.build().send_changes_mailbox()` would post `methodCalls: []`
//! and still deserialise a canned single-call response (Findings #31).

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use common::{
    connected_client, jmap_response, mount_jmap, mount_session, single_jmap_request, ACCOUNT_ID,
    SESSION_STATE,
};
use jmap_client::client::Client;
use jmap_client::core::error::MethodErrorType;
use jmap_client::core::query::Filter;
use jmap_client::core::query::QueryResponse;
use jmap_client::core::query_changes::QueryChangesResponse;
use jmap_client::core::response::{EmailChangesResponse, MailboxChangesResponse, ThreadChangesResponse};
use jmap_client::email::query::{Comparator as EmailComparator, Filter as EmailFilter};
use jmap_client::mailbox::query::{Comparator as MailboxComparator, Filter as MailboxFilter};
use jmap_client::mailbox::Property;
use jmap_client::{Error, Method};
use serde_json::{json, Value};
use wiremock::MockServer;

// ---------------------------------------------------------------------------
// Fixtures
//
// A JMAP method response is an *Invocation* — the three-element array
// `[name, arguments, callId]` (RFC 8620 §3.2). `TaggedMethodResponse` is
// deserialised from that array, never from the bare arguments object, so every
// body below is wrapped with [`inv`].
// ---------------------------------------------------------------------------

/// Wraps a method-response body in an RFC 8620 §3.2 Invocation.
fn inv(name: &str, call_id: &str, body: Value) -> Value {
    json!([name, body, call_id])
}

/// RFC 8620 §3.6.2 — a method-level error Invocation:
/// `["error", { "type": … }, "call-id"]`, delivered with HTTP 200.
fn method_error(call_id: &str, error_type: &str) -> Value {
    json!(["error", { "type": error_type }, call_id])
}

/// RFC 8620 §5.2 — the standard `/changes` response arguments.
///
/// All seven members are always emitted: `ChangesResponse` declares
/// `{accountId,oldState,newState,hasMoreChanges,created,updated,destroyed}`
/// as non-`Option`, so a fixture that omits any of them fails to deserialise
/// (Findings #8 — asserted explicitly by
/// [`a_changes_response_missing_a_required_member_is_a_parse_error`]).
fn changes_body(
    account_id: &str,
    old_state: &str,
    new_state: &str,
    has_more_changes: bool,
    created: &[&str],
    updated: &[&str],
    destroyed: &[&str],
) -> Value {
    json!({
        "accountId": account_id,
        "oldState": old_state,
        "newState": new_state,
        "hasMoreChanges": has_more_changes,
        "created": created,
        "updated": updated,
        "destroyed": destroyed,
    })
}

/// RFC 8621 §2.6 — the `Mailbox/changes` response of the RFC's sync
/// walkthrough, with the `updatedProperties` extension of §2.2. The account id
/// is this suite's session account rather than the RFC's `u33084183`… which it
/// happens to be identical to.
fn mailbox_changes_body(updated_properties: Value) -> Value {
    let mut body = changes_body(
        ACCOUNT_ID,
        "78541",
        "78542",
        false,
        &[],
        &["MB23cfa8094c0f41e6"],
        &[],
    );
    body["updatedProperties"] = updated_properties;
    body
}

/// RFC 8621 §4.10 (the "push update arrives" walkthrough) — the `Email/changes`
/// response, verbatim apart from the account id.
fn email_changes_body() -> Value {
    changes_body(
        ACCOUNT_ID,
        "780605",
        "780800",
        false,
        &["Me8de6c9f6de198239b982ea2"],
        &["Ma783e5cdf5f2deffbc97930a"],
        &["M9bd17497e2a99cb345fc1d0a"],
    )
}

/// RFC 8621 §3.2 declares `Thread/changes` a standard `/changes` method
/// (RFC 8620 §5.2); the ids are the Thread ids used in the §4.10 examples.
fn thread_changes_body() -> Value {
    changes_body(
        ACCOUNT_ID,
        "22a8728b",
        "22a8728c",
        false,
        &["T0a22ad76e9c097a1"],
        &["T36703c2cfe9bd5ed"],
        &[],
    )
}

/// RFC 8620 §5.5 — the `/query` response arguments (the RFC gives the member
/// list but no JSON example; this is that list in a minimal correct instance).
fn query_body(
    account_id: &str,
    query_state: &str,
    can_calculate_changes: Value,
    position: i32,
    total: Value,
    ids: &[&str],
) -> Value {
    let mut body = json!({
        "accountId": account_id,
        "queryState": query_state,
        "position": position,
        "ids": ids,
    });
    // `canCalculateChanges` and `total` are inserted explicitly so a test can
    // leave them out entirely and observe the client's defaulting.
    if !can_calculate_changes.is_null() {
        body["canCalculateChanges"] = can_calculate_changes;
    }
    if !total.is_null() {
        body["total"] = total;
    }
    body
}

/// RFC 8620 §5.6 — the `/queryChanges` response arguments.
///
/// Note there is **no `upToPosition` member** in the RFC's response, nor in
/// `QueryChangesResponse`: `upToId` is a *request*-only argument.
fn query_changes_body(
    account_id: &str,
    old_query_state: &str,
    new_query_state: &str,
    total: Value,
    removed: &[&str],
    added: Value,
) -> Value {
    let mut body = json!({
        "accountId": account_id,
        "oldQueryState": old_query_state,
        "newQueryState": new_query_state,
        "removed": removed,
        "added": added,
    });
    if !total.is_null() {
        body["total"] = total;
    }
    body
}

// ---------------------------------------------------------------------------
// Send helpers
//
// Each helper adds the method call explicitly before the typed send, so the
// request body is never empty (Findings #31).
// ---------------------------------------------------------------------------

async fn mailbox_changes(client: &Client, since_state: &str) -> jmap_client::Result<MailboxChangesResponse> {
    let mut request = client.build();
    request.changes_mailbox(since_state);
    request.send_changes_mailbox().await
}

async fn email_changes(client: &Client, since_state: &str) -> jmap_client::Result<EmailChangesResponse> {
    let mut request = client.build();
    request.changes_email(since_state);
    request.send_changes_email().await
}

async fn thread_changes(client: &Client, since_state: &str) -> jmap_client::Result<ThreadChangesResponse> {
    let mut request = client.build();
    request.changes_thread(since_state);
    request.send_changes_thread().await
}

async fn mailbox_query(client: &Client) -> jmap_client::Result<QueryResponse> {
    let mut request = client.build();
    request.query_mailbox();
    request.send_query_mailbox().await
}

async fn mailbox_query_changes(
    client: &Client,
    since_query_state: &str,
) -> jmap_client::Result<QueryChangesResponse> {
    let mut request = client.build();
    request.query_mailbox_changes(since_query_state);
    request.send_query_mailbox_changes().await
}

// ---------------------------------------------------------------------------
// A. Changes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_changes_round_trip_parses_every_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                changes_body(
                    ACCOUNT_ID,
                    "78541",
                    "78542",
                    false,
                    &["MBdfe8ab4d4e0c4f0"],
                    &["MB23cfa8094c0f41e6"],
                    &["MB674cc24095db49ce"],
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut response = mailbox_changes(&client, "78541").await.unwrap();

    // RFC 8620 §5.2 response members.
    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "78541");
    assert_eq!(response.new_state(), "78542");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["MBdfe8ab4d4e0c4f0"]);
    assert_eq!(response.updated(), ["MB23cfa8094c0f41e6"]);
    assert_eq!(response.destroyed(), ["MB674cc24095db49ce"]);
    assert_eq!(response.total_changes(), 3);

    // The `take_*` accessors move the values out, leaving empties behind.
    assert_eq!(response.take_account_id(), ACCOUNT_ID);
    assert_eq!(response.take_new_state(), "78542");
    assert_eq!(response.take_created(), vec!["MBdfe8ab4d4e0c4f0".to_string()]);
    assert_eq!(response.take_updated(), vec!["MB23cfa8094c0f41e6".to_string()]);
    assert_eq!(
        response.take_destroyed(),
        vec!["MB674cc24095db49ce".to_string()]
    );
    assert!(response.created().is_empty());
    assert_eq!(response.total_changes(), 0);

    // --- what actually went over the wire (RFC 8620 §3.2/§5.2) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/changes", "method name must be on the wire");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["sinceState"], "78541");
    // maxChanges was never set, so it must not be serialised (§5.2: "If not
    // given … the server may choose how many to return").
    assert!(call[1].get("maxChanges").is_none());
}

#[tokio::test]
async fn changes_max_changes_is_sent_verbatim_even_when_the_rfc_forbids_zero() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                changes_body(ACCOUNT_ID, "78541", "78542", false, &[], &[], &[]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // This is exactly what the crate-level doc example in `src/lib.rs` does.
    let mut request = client.build();
    request.changes_mailbox("78541").max_changes(0);
    request.send_changes_mailbox().await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/changes");
    // RFC 8620 §5.2: "If supplied by the client, the value MUST be a positive
    // integer greater than 0. If a value outside of this range is given, the
    // server MUST reject the call with an `invalidArguments` error." The client
    // does not validate `max_changes`; it serialises whatever it is given.
    assert_eq!(call[1]["maxChanges"], 0);

    // A legal value round-trips unchanged too.
    //
    // NOTE: `MockServer::reset()` clears the *recorded requests* as well as the
    // mocks, so the spy log above must be read before resetting.
    server.reset().await;
    mount_session(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                changes_body(ACCOUNT_ID, "780605", "780800", false, &[], &[], &[]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // RFC 8621 §4.10 sends `"maxChanges": 50` for Email/changes.
    let mut request = client.build();
    request.changes_email("780605").max_changes(50);
    request.send_changes_email().await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/changes");
    assert_eq!(call[1]["sinceState"], "780605");
    assert_eq!(call[1]["maxChanges"], 50);
}

#[tokio::test]
async fn mailbox_changes_updated_properties_deserialises_to_count_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §2.6 response, call "0" — only the count properties changed.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                mailbox_changes_body(json!([
                    "totalEmails",
                    "unreadEmails",
                    "totalThreads",
                    "unreadThreads"
                ])),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let response = mailbox_changes(&client, "78541").await.unwrap();

    // `updatedProperties` lives in the flattened per-type arguments object
    // (`mailbox::ChangesResponse`), not in the core `ChangesResponse`.
    let properties = response.arguments().updated_properties().unwrap();
    assert_eq!(
        properties,
        [
            Property::TotalEmails,
            Property::UnreadEmails,
            Property::TotalThreads,
            Property::UnreadThreads
        ]
    );
    assert!(properties.iter().all(Property::is_count));
    // The core members still parse alongside the extension.
    assert_eq!(response.updated(), ["MB23cfa8094c0f41e6"]);
    assert_eq!(response.new_state(), "78542");
}

#[tokio::test]
async fn mailbox_changes_updated_properties_null_deserialises_to_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §2.2: "If the server is unable to tell if only counts have
    // changed, it MUST just be null."
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                mailbox_changes_body(Value::Null),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let response = mailbox_changes(&client, "78541").await.unwrap();
    assert!(response.arguments().updated_properties().is_none());
    assert_eq!(response.new_state(), "78542");
}

#[tokio::test]
async fn email_changes_round_trip() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv("Email/changes", "s0", email_changes_body())],
            SESSION_STATE,
        ),
    )
    .await;

    let response = email_changes(&client, "780605").await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "780605");
    assert_eq!(response.new_state(), "780800");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["Me8de6c9f6de198239b982ea2"]);
    assert_eq!(response.updated(), ["Ma783e5cdf5f2deffbc97930a"]);
    assert_eq!(response.destroyed(), ["M9bd17497e2a99cb345fc1d0a"]);
    assert_eq!(response.total_changes(), 3);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/changes");
    assert_eq!(call[1]["sinceState"], "780605");
    // Email has no `updatedProperties` extension: the flattened argument is
    // `()`, so nothing extra may appear in the arguments object.
    assert_eq!(
        response.arguments(),
        &(),
        "Email/changes carries no per-type extension members"
    );
}

#[tokio::test]
async fn thread_changes_round_trip() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv("Thread/changes", "s0", thread_changes_body())],
            SESSION_STATE,
        ),
    )
    .await;

    let response = thread_changes(&client, "22a8728b").await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "22a8728b");
    assert_eq!(response.new_state(), "22a8728c");
    assert_eq!(response.created(), ["T0a22ad76e9c097a1"]);
    assert_eq!(response.updated(), ["T36703c2cfe9bd5ed"]);
    assert!(response.destroyed().is_empty());
    assert_eq!(response.total_changes(), 2);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/changes");
    assert_eq!(call[1]["sinceState"], "22a8728b");
}

#[tokio::test]
async fn changes_pages_through_intermediate_states_until_has_more_changes_is_false() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.2's A→B→C→D→E walkthrough: the first update takes the
    // client from "B" to the intermediate state "B-D-E" and says more remain.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                changes_body(ACCOUNT_ID, "B", "B-D-E", true, &[], &["MB23cfa8094c0f41e6"], &[]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let first = mailbox_changes(&client, "B").await.unwrap();
    assert!(first.has_more_changes());
    assert_eq!(first.new_state(), "B-D-E");

    // Snapshot the spy log first: `MockServer::reset()` clears the recorded
    // requests along with the mocks.
    let first_call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(first_call[0], "Mailbox/changes");
    assert_eq!(first_call[1]["sinceState"], "B");

    // …so the client calls again *with the new state*, and the server walks it
    // to the final state "E" with hasMoreChanges false.
    server.reset().await;
    mount_session(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                changes_body(ACCOUNT_ID, "B-D-E", "E", false, &[], &[], &["MB674cc24095db49ce"]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let second = mailbox_changes(&client, first.new_state()).await.unwrap();
    assert!(!second.has_more_changes());
    assert_eq!(second.old_state(), "B-D-E");
    assert_eq!(second.new_state(), "E");
    assert_eq!(second.destroyed(), ["MB674cc24095db49ce"]);

    // Both calls really went out, each carrying the right sinceState.
    let second_call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(second_call[0], "Mailbox/changes");
    assert_eq!(second_call[1]["sinceState"], "B-D-E");
}

#[tokio::test]
async fn changes_cannot_calculate_changes_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.2: an unresolvable sinceState is a `cannotCalculateChanges`
    // *method* error, delivered with HTTP 200, not a request-level problem.
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "cannotCalculateChanges")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = mailbox_changes(&client, "very-old-state").await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::CannotCalculateChanges);
    assert_eq!(method_err.to_string(), "Cannot calculate changes");

    // The call still went out — the error is about its arguments, not its absence.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/changes");
    assert_eq!(call[1]["sinceState"], "very-old-state");
}

#[tokio::test]
async fn a_changes_response_missing_a_required_member_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // Findings #8: `hasMoreChanges` is a non-`Option` member of
    // `ChangesResponse`, so an otherwise well-formed response without it fails
    // deserialisation of the whole Response object.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/changes",
                "s0",
                json!({
                    "accountId": ACCOUNT_ID,
                    "oldState": "78541",
                    "newState": "78542",
                    "created": [],
                    "updated": [],
                    "destroyed": []
                }),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let err = mailbox_changes(&client, "78541").await.unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
    // Loud failure, not a silent one: the request was on the wire.
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/changes"
    );
}

// ---------------------------------------------------------------------------
// B. Query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_query_serialises_filter_sort_and_window() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/query",
                "s0",
                query_body(
                    ACCOUNT_ID,
                    "09aa9a075588-78540:0",
                    json!(true),
                    0,
                    json!(2),
                    &["MB23cfa8094c0f41e6", "MB674cc24095db49ce"],
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let query = request.query_mailbox();
    // RFC 8620 §5.5: filter is FilterOperator|FilterCondition|null.
    query.filter(Filter::<MailboxFilter>::and([
        MailboxFilter::name("Inbox"),
        MailboxFilter::is_subscribed(true),
    ]));
    // RFC 8621 §2.3: sortOrder and name MUST be sortable.
    query.sort([
        MailboxComparator::sort_order(),
        MailboxComparator::name().descending(),
    ]);
    query.position(0).limit(2).calculate_total(true);
    // RFC 8621 §2.3 Mailbox-specific arguments.
    query.arguments().sort_as_tree(true).filter_as_tree(true);

    let mut response = request.send_query_mailbox().await.unwrap();

    // RFC 8620 §5.5 response members.
    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.query_state(), "09aa9a075588-78540:0");
    assert!(response.can_calculate_changes());
    assert_eq!(response.position(), 0);
    assert_eq!(response.total(), Some(2));
    assert_eq!(response.limit(), None, "server set no limit of its own");
    assert_eq!(response.ids().len(), 2);
    assert_eq!(response.id(0), "MB23cfa8094c0f41e6");
    assert_eq!(response.id(1), "MB674cc24095db49ce");
    assert_eq!(response.take_ids().len(), 2);

    // --- wire assertions (RFC 8620 §5.5) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/query", "method name must be on the wire");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        call[1]["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "name": "Inbox" },
                { "isSubscribed": true }
            ]
        })
    );
    assert_eq!(
        call[1]["sort"],
        json!([
            { "isAscending": true, "property": "sortOrder" },
            { "isAscending": false, "property": "name" }
        ])
    );
    assert_eq!(call[1]["position"], 0);
    assert_eq!(call[1]["limit"], 2);
    assert_eq!(call[1]["calculateTotal"], true);
    // Mailbox's `QueryArguments` holds sortAsTree/filterAsTree as non-`Option`
    // bools, so they are always serialised — never absent.
    assert_eq!(call[1]["sortAsTree"], true);
    assert_eq!(call[1]["filterAsTree"], true);
    // No anchor was given, so neither anchor member may appear (§5.5).
    assert!(call[1].get("anchor").is_none());
    assert!(call[1].get("anchorOffset").is_none());
}

#[tokio::test]
async fn email_query_serialises_comparators_and_collation() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §4.10 — the response to the inbox-window query.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Email/query",
                "s0",
                query_body(
                    ACCOUNT_ID,
                    "09aa9a075588-780599:0",
                    json!(true),
                    0,
                    json!(115),
                    &["Ma783e5cdf5f2deffbc97930a", "M9bd17497e2a99cb345fc1d0a"],
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let query = request.query_email();
    query.filter(Filter::<EmailFilter>::or([
        EmailFilter::in_mailbox("fb666a55"),
        EmailFilter::subject("World domination"),
    ]));
    query.sort([
        EmailComparator::received_at()
            .descending()
            .collation("i;unicode-casemap".to_string()),
        EmailComparator::has_keyword("$flagged"),
    ]);
    query.position(-30).limit(30);
    // RFC 8621 §4.4 extra request argument.
    query.arguments().collapse_threads(true);

    let response = request.send_query_email().await.unwrap();
    assert_eq!(response.query_state(), "09aa9a075588-780599:0");
    assert_eq!(response.total(), Some(115));
    assert!(response.can_calculate_changes());

    // --- wire assertions ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/query");
    assert_eq!(
        call[1]["filter"],
        json!({
            "operator": "OR",
            "conditions": [
                { "inMailbox": "fb666a55" },
                { "subject": "World domination" }
            ]
        })
    );
    // RFC 8620 §5.5: a Comparator is { property, isAscending?, collation? }
    // plus any type-specific members (here `keyword`).
    assert_eq!(
        call[1]["sort"],
        json!([
            {
                "isAscending": false,
                "collation": "i;unicode-casemap",
                "property": "receivedAt"
            },
            {
                "isAscending": true,
                "property": "hasKeyword",
                "keyword": "$flagged"
            }
        ])
    );
    // §5.5: a negative position is an offset from the end of the results.
    assert_eq!(call[1]["position"], -30);
    assert_eq!(call[1]["limit"], 30);
    assert_eq!(call[1]["collapseThreads"], true);
    // calculateTotal was not requested → not serialised.
    assert!(call[1].get("calculateTotal").is_none());
}

#[tokio::test]
async fn query_unsupported_sort_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.5: "unsupportedSort: The sort is syntactically valid, but it
    // includes a property the server does not support sorting on".
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unsupportedSort")], SESSION_STATE),
    )
    .await;

    let err = mailbox_query(&client).await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::UnsupportedSort);
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/query"
    );
}

#[tokio::test]
async fn query_unsupported_filter_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.5: "unsupportedFilter: The filter is syntactically valid,
    // but the server cannot process it."
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unsupportedFilter")], SESSION_STATE),
    )
    .await;

    let err = mailbox_query(&client).await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::UnsupportedFilter);
    assert_eq!(method_err.to_string(), "Unsupported filter");
}

#[tokio::test]
async fn can_calculate_changes_false_is_surfaced_and_absent_means_false() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.5: canCalculateChanges tells the client whether
    // Foo/queryChanges is possible with *these* filter/sort parameters.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/query",
                "s0",
                query_body(ACCOUNT_ID, "mutable-sort-state", json!(false), 0, Value::Null, &[]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let response = mailbox_query(&client).await.unwrap();
    assert!(
        !response.can_calculate_changes(),
        "an explicit false must surface as false"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/query"
    );

    // The member is `Option<bool>` in the client and `can_calculate_changes()`
    // is `unwrap_or(false)`, so a server that omits it reads as false too.
    server.reset().await;
    mount_session(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/query",
                "s0",
                query_body(ACCOUNT_ID, "silent-state", Value::Null, 0, Value::Null, &[]),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let response = mailbox_query(&client).await.unwrap();
    assert!(
        !response.can_calculate_changes(),
        "an omitted canCalculateChanges must default to false"
    );
    assert!(response.ids().is_empty());
}

// ---------------------------------------------------------------------------
// C. QueryChanges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_query_changes_round_trip() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.6 response shape; RFC 8621 §2.4 makes Mailbox/queryChanges
    // exactly that standard method.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/queryChanges",
                "s0",
                query_changes_body(
                    ACCOUNT_ID,
                    "09aa9a075588-78540:0",
                    "e35e9facf117-78542:0",
                    json!(3),
                    &["MB674cc24095db49ce"],
                    json!([{ "id": "MBdfe8ab4d4e0c4f0", "index": 0 }]),
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let query = request.query_mailbox_changes("09aa9a075588-78540:0");
    query.filter(MailboxFilter::is_subscribed(true));
    query.sort([MailboxComparator::name()]);
    query.max_changes(25);
    query.up_to_id("MB23cfa8094c0f41e6");
    query.calculate_total(true);

    let response = request.send_query_mailbox_changes().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_query_state(), "09aa9a075588-78540:0");
    assert_eq!(response.new_query_state(), "e35e9facf117-78542:0");
    assert_eq!(response.total(), Some(3));
    assert_eq!(response.removed(), ["MB674cc24095db49ce"]);
    assert_eq!(response.added().len(), 1);
    assert_eq!(response.added()[0].id(), "MBdfe8ab4d4e0c4f0");
    assert_eq!(response.added()[0].index(), 0);

    // --- wire assertions (RFC 8620 §5.6 request arguments) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/queryChanges", "method name must be on the wire");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["sinceQueryState"], "09aa9a075588-78540:0");
    assert_eq!(call[1]["filter"], json!({ "isSubscribed": true }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": true, "property": "name" }])
    );
    assert_eq!(call[1]["maxChanges"], 25);
    assert_eq!(call[1]["upToId"], "MB23cfa8094c0f41e6");
    assert_eq!(call[1]["calculateTotal"], true);
    // Mailbox's tree arguments ride along on queryChanges too.
    assert_eq!(call[1]["sortAsTree"], false);
    assert_eq!(call[1]["filterAsTree"], false);
}

#[tokio::test]
async fn email_query_changes_round_trip_uses_the_rfc_8621_example() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §4.10 — the `Email/queryChanges` response, verbatim apart from
    // the account id.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Email/queryChanges",
                "s0",
                query_changes_body(
                    ACCOUNT_ID,
                    "09aa9a075588-780599:0",
                    "e35e9facf117-780615:0",
                    json!(115),
                    &["M9bd17497e2a99cb345fc1d0a"],
                    json!([{ "id": "Me8de6c9f6de198239b982ea2", "index": 0 }]),
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // The matching request from the same RFC walkthrough.
    let mut request = client.build();
    let query = request.query_email_changes("09aa9a075588-780599:0");
    query.filter(EmailFilter::in_mailbox("fb666a55"));
    query.sort([EmailComparator::received_at().descending()]);
    query.arguments().collapse_threads(true);
    query.up_to_id("Mc2781d5e856a908d8a35a564");
    query.max_changes(25);
    query.calculate_total(true);

    let response = request.send_query_email_changes().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_query_state(), "09aa9a075588-780599:0");
    assert_eq!(response.new_query_state(), "e35e9facf117-780615:0");
    assert_eq!(response.total(), Some(115));
    assert_eq!(response.removed(), ["M9bd17497e2a99cb345fc1d0a"]);
    assert_eq!(response.added().len(), 1);
    assert_eq!(response.added()[0].id(), "Me8de6c9f6de198239b982ea2");
    assert_eq!(response.added()[0].index(), 0);

    // --- wire assertions, against RFC 8621 §4.10's own request ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/queryChanges");
    assert_eq!(call[1]["sinceQueryState"], "09aa9a075588-780599:0");
    assert_eq!(call[1]["filter"], json!({ "inMailbox": "fb666a55" }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": false, "property": "receivedAt" }])
    );
    assert_eq!(call[1]["collapseThreads"], true);
    assert_eq!(call[1]["upToId"], "Mc2781d5e856a908d8a35a564");
    assert_eq!(call[1]["maxChanges"], 25);
    assert_eq!(call[1]["calculateTotal"], true);
}

#[tokio::test]
async fn query_changes_calculate_total_is_always_on_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/queryChanges",
                "s0",
                query_changes_body(
                    ACCOUNT_ID,
                    "old-state",
                    "new-state",
                    Value::Null,
                    &[],
                    json!([]),
                ),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // calculate_total is never called here.
    let response = mailbox_query_changes(&client, "old-state").await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/queryChanges");
    // `QueryChangesRequest::calculate_total` is a plain `bool` (default false),
    // not an `Option`, so unlike QueryRequest it is *always* serialised. RFC
    // 8620 §5.6 gives the same default, so the wire stays conformant — but the
    // member is never absent.
    assert_eq!(call[1]["calculateTotal"], false);
    // maxChanges / upToId are Options and stay off the wire when unset.
    assert!(call[1].get("maxChanges").is_none());
    assert!(call[1].get("upToId").is_none());
    // And the *parsed client response* carries no `total`, because none was
    // requested. Asserting on the fixture builder here would prove nothing about
    // the client; this checks the deserialised value instead.
    assert_eq!(response.total(), None);
    assert_eq!(response.old_query_state(), "old-state");
    assert_eq!(response.new_query_state(), "new-state");
}

#[tokio::test]
async fn query_changes_too_many_changes_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.6: "tooManyChanges: There are more changes than the client's
    // maxChanges argument."
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "tooManyChanges")], SESSION_STATE),
    )
    .await;

    let err = mailbox_query_changes(&client, "stale-query-state")
        .await
        .unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::TooManyChanges);
    assert_eq!(method_err.to_string(), "Too many changes");
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/queryChanges"
    );
}

#[tokio::test]
async fn query_changes_cannot_calculate_changes_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.6: the client MUST invalidate its query cache on this one.
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "cannotCalculateChanges")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = mailbox_query_changes(&client, "ancient-query-state")
        .await
        .unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::CannotCalculateChanges);
}

// ---------------------------------------------------------------------------
// D. Cross-cutting — result references between calls in one request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn doc_example_changes_to_get_chain_wires_result_references() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §2.6 responses for the changes→get pattern (the RFC's calls
    // "0" and "2"; the client numbers them s0/s1).
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Mailbox/changes",
                    "s0",
                    mailbox_changes_body(json!([
                        "totalEmails",
                        "unreadEmails",
                        "totalThreads",
                        "unreadThreads"
                    ])),
                ),
                inv(
                    "Mailbox/get",
                    "s1",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "state": "78542",
                        "list": [{
                            "id": "MB23cfa8094c0f41e6",
                            "totalEmails": 16307,
                            "unreadEmails": 13903,
                            "totalThreads": 5833,
                            "unreadThreads": 5127
                        }],
                        "notFound": []
                    }),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    // This block mirrors the crate-level doc example in `src/lib.rs` exactly,
    // including its RFC 8620 §5.2 compliant `maxChanges` of 16.
    let mut request = client.build();
    let changes_request = request.changes_mailbox("78541").max_changes(16);
    let properties_ref = changes_request.updated_properties_reference();
    let updated_ref = changes_request.updated_reference();
    request
        .get_mailbox()
        .ids_ref(updated_ref)
        .properties_ref(properties_ref);

    let mut response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 2);

    // Both responses come back tagged with the call ids the client assigned.
    let changes = response.method_response_by_id("s0").unwrap();
    assert!(changes.is_type(Method::ChangesMailbox));
    assert!(!changes.is_error());
    let get = response.method_response_by_id("s1").unwrap();
    assert!(get.is_type(Method::GetMailbox));
    assert!(!get.is_error());

    // The changes half is usable as a sync step.
    let changes = response.method_response_by_pos(0).unwrap_changes_mailbox().unwrap();
    assert_eq!(changes.new_state(), "78542");
    assert_eq!(changes.updated(), ["MB23cfa8094c0f41e6"]);

    // The get half carries only the referenced count properties.
    let mut get = response.method_response_by_pos(0).unwrap_get_mailbox().unwrap();
    let mailbox = get.take_list().pop().unwrap();
    assert_eq!(mailbox.id(), Some("MB23cfa8094c0f41e6"));
    assert_eq!(mailbox.total_emails(), 16307);
    assert_eq!(mailbox.unread_emails(), 13903);
    // `name` was not among the referenced properties, so it is absent.
    assert_eq!(mailbox.name(), None);

    // --- wire assertions (RFC 8620 §3.7) ---
    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "Mailbox/changes");
    assert_eq!(calls[0][2], "s0");
    assert_eq!(calls[0][1]["sinceState"], "78541");
    // The value the doc example sends: RFC 8620 §5.2 requires `maxChanges` to
    // be greater than 0 when `sinceState` is present.
    assert_eq!(calls[0][1]["maxChanges"], 16);

    assert_eq!(calls[1][0], "Mailbox/get");
    assert_eq!(calls[1][2], "s1");
    assert_eq!(
        calls[1][1]["#ids"],
        json!({
            "resultOf": "s0",
            "name": "Mailbox/changes",
            "path": "/updated"
        })
    );
    assert_eq!(
        calls[1][1]["#properties"],
        json!({
            "resultOf": "s0",
            "name": "Mailbox/changes",
            "path": "/updatedProperties"
        })
    );
    // Setting a reference clears the literal form: RFC 8620 §3.7 makes having
    // both `ids` and `#ids` an invalidArguments error.
    assert!(calls[1][1].get("ids").is_none());
    assert!(calls[1][1].get("properties").is_none());
}

#[tokio::test]
async fn rfc_8621_2_6_full_changes_chain_uses_created_and_updated_references() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §2.6 — the three-call sync request and its response, with the
    // client's `s`-prefixed call ids in place of the RFC's "0"/"1"/"2".
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Mailbox/changes",
                    "s0",
                    mailbox_changes_body(json!([
                        "totalEmails",
                        "unreadEmails",
                        "totalThreads",
                        "unreadThreads"
                    ])),
                ),
                inv(
                    "Mailbox/get",
                    "s1",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "state": "78542",
                        "list": [],
                        "notFound": []
                    }),
                ),
                inv(
                    "Mailbox/get",
                    "s2",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "state": "78542",
                        "list": [{
                            "id": "MB23cfa8094c0f41e6",
                            "totalEmails": 16307,
                            "unreadEmails": 13903,
                            "totalThreads": 5833,
                            "unreadThreads": 5127
                        }],
                        "notFound": []
                    }),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let changes = request.changes_mailbox("78541");
    let created_ref = changes.created_reference();
    let updated_ref = changes.updated_reference();
    let properties_ref = changes.updated_properties_reference();
    request.get_mailbox().ids_ref(created_ref);
    request
        .get_mailbox()
        .ids_ref(updated_ref)
        .properties_ref(properties_ref);

    let response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 3);
    assert!(response
        .method_response_by_id("s0")
        .unwrap()
        .is_type(Method::ChangesMailbox));
    assert!(response
        .method_response_by_id("s1")
        .unwrap()
        .is_type(Method::GetMailbox));
    assert!(response
        .method_response_by_id("s2")
        .unwrap()
        .is_type(Method::GetMailbox));

    // The responses stay in request order after unwrapping.
    let responses = response.unwrap_method_responses();
    assert_eq!(responses[0].call_id(), "s0");
    assert_eq!(responses[1].call_id(), "s1");
    assert_eq!(responses[2].call_id(), "s2");

    // --- wire assertions: every reference points at s0 with the right path ---
    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0][0], "Mailbox/changes");
    assert_eq!(
        calls[1][1]["#ids"],
        json!({ "resultOf": "s0", "name": "Mailbox/changes", "path": "/created" })
    );
    assert_eq!(
        calls[2][1]["#ids"],
        json!({ "resultOf": "s0", "name": "Mailbox/changes", "path": "/updated" })
    );
    assert_eq!(
        calls[2][1]["#properties"],
        json!({ "resultOf": "s0", "name": "Mailbox/changes", "path": "/updatedProperties" })
    );
    // The changes call itself carries no references at all.
    assert!(calls[0][1].get("#ids").is_none());
    assert!(calls[0][1].get("#properties").is_none());
}

#[tokio::test]
async fn rfc_8621_4_10_query_to_get_chain_references_the_query_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §4.10 — Email/query then Email/get on the query's /ids.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Email/query",
                    "s0",
                    query_body(
                        ACCOUNT_ID,
                        "09aa9a075588-780599:0",
                        json!(true),
                        0,
                        json!(115),
                        &["Ma783e5cdf5f2deffbc97930a", "M9bd17497e2a99cb345fc1d0a"],
                    ),
                ),
                inv(
                    "Email/get",
                    "s1",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "state": "780599",
                        "list": [
                            { "id": "Ma783e5cdf5f2deffbc97930a", "threadId": "T36703c2cfe9bd5ed" },
                            { "id": "M9bd17497e2a99cb345fc1d0a", "threadId": "T0a22ad76e9c097a1" }
                        ],
                        "notFound": []
                    }),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let query = request.query_email();
    query.filter(EmailFilter::in_mailbox("fb666a55"));
    query.sort([EmailComparator::received_at().descending()]);
    query.arguments().collapse_threads(true);
    query.position(0).limit(30);
    // RFC 8620 §3.7 / §5.5: QueryRequest::result_reference() points at /ids.
    let ids_ref = query.result_reference();
    request.get_email().ids_ref(ids_ref);

    let mut response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 2);
    assert!(response
        .method_response_by_id("s0")
        .unwrap()
        .is_type(Method::QueryEmail));
    assert!(response
        .method_response_by_id("s1")
        .unwrap()
        .is_type(Method::GetEmail));

    let query_response = response
        .method_response_by_pos(0)
        .unwrap_query_email()
        .unwrap();
    assert_eq!(query_response.ids().len(), 2);
    let get_response = response
        .method_response_by_pos(0)
        .unwrap_get_email()
        .unwrap();
    assert_eq!(get_response.state(), "780599");
    assert_eq!(get_response.list().len(), 2);

    // --- wire assertions ---
    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "Email/query");
    assert_eq!(calls[0][1]["limit"], 30);
    assert_eq!(calls[1][0], "Email/get");
    assert_eq!(calls[1][2], "s1");
    assert_eq!(
        calls[1][1]["#ids"],
        json!({ "resultOf": "s0", "name": "Email/query", "path": "/ids" })
    );
    assert!(calls[1][1].get("ids").is_none());
}

#[tokio::test]
async fn a_query_error_in_a_chained_request_does_not_disturb_the_other_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // The changes call succeeds; the queryChanges call cannot catch up.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Mailbox/changes",
                    "s0",
                    changes_body(ACCOUNT_ID, "78541", "78542", false, &[], &["MB23cfa8094c0f41e6"], &[]),
                ),
                method_error("s1", "cannotCalculateChanges"),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.changes_mailbox("78541");
    request.query_mailbox_changes("09aa9a075588-78540:0");

    let mut response = request.send().await.unwrap();
    assert!(!response.method_response_by_id("s0").unwrap().is_error());
    assert!(response.method_response_by_id("s1").unwrap().is_error());
    assert!(response
        .method_response_by_id("s1")
        .unwrap()
        .is_type(Method::Error));

    // The good response still unwraps; the bad one yields the method error.
    let changes = response
        .method_response_by_pos(0)
        .unwrap_changes_mailbox()
        .unwrap();
    assert_eq!(changes.new_state(), "78542");

    let err = response
        .method_response_by_pos(0)
        .unwrap_query_changes_mailbox()
        .unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::CannotCalculateChanges);

    // Both calls went out, in order, with their own names.
    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls[0][0], "Mailbox/changes");
    assert_eq!(calls[1][0], "Mailbox/queryChanges");
    assert_eq!(calls[1][1]["sinceQueryState"], "09aa9a075588-78540:0");
}
