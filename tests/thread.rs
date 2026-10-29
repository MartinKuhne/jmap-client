//! P4 — mail-domain integration tests: **Threads**.
//!
//! Drives the `Client`/`Request` Thread helpers from `src/thread/helpers.rs`
//! against a `wiremock` HTTP server using the shared fixtures in
//! `tests/common/mod.rs`. Canned payloads are the RFC 8621 example bodies
//! wherever one exists (the section is cited on the fixture); otherwise the
//! minimal RFC-correct shape, also cited.
//!
//! Coverage targets: `thread/helpers.rs` (`Client::thread_get` plus the
//! `Request::{get_thread, send_get_thread, changes_thread,
//! send_changes_thread}` builders), `thread/get.rs` (record accessors) and
//! `thread/mod.rs` (`Thread`, `Property` (de)serialisation and `Display`).
//!
//! RFC 8621 §3.1 says `Thread/get` is "a standard '/get' method as described
//! in [RFC8620], Section 5.1", and §3.2 says the same of `/changes`, so the
//! wire shape assertions below cite RFC 8620 §5.1/§5.2 for the envelope and
//! RFC 8621 §3.1.1 for the record contents.

mod common;

use common::{
    connected_client, jmap_response, mount_jmap, single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::thread::Property;
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

// ---------------------------------------------------------------------------
// Fixtures
//
// A JMAP method response is an *Invocation* — `[name, arguments, callId]`
// (RFC 8620 §3.2) — so every body below is wrapped with [`inv`].
// ---------------------------------------------------------------------------

/// Wraps a method-response body in an RFC 8620 §3.2 Invocation.
fn inv(name: &str, call_id: &str, body: Value) -> Value {
    json!([name, body, call_id])
}

/// RFC 8620 §3.6.2 — a method-level error Invocation, delivered with HTTP 200.
fn method_error(call_id: &str, error_type: &str) -> Value {
    json!(["error", { "type": error_type }, call_id])
}

/// Answers the next `POST /jmap` with a single successful method response.
async fn mount_one(server: &MockServer, name: &str, body: Value) {
    mount_jmap(
        server,
        jmap_response(vec![inv(name, "s0", body)], SESSION_STATE),
    )
    .await
}

/// RFC 8621 §3.1.1 — the `Thread/get` response, verbatim.
fn rfc_thread_get_response() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "f6a7e214",
        "list": [
            {
                "id": "f123u4",
                "emailIds": [ "eaa623", "f782cbb" ]
            },
            {
                "id": "f41u44",
                "emailIds": [ "82cf7bb" ]
            }
        ],
        "notFound": []
    })
}

/// A single-record `Thread/get` body carrying just the RFC's first record.
fn thread_get_one() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "f6a7e214",
        "list": [
            { "id": "f123u4", "emailIds": [ "eaa623", "f782cbb" ] }
        ],
        "notFound": []
    })
}

/// RFC 8620 §5.2 — the standard `/changes` response, with Thread ids.
fn thread_changes_body(since: &str) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": since,
        "newState": "f6a7e215",
        "hasMoreChanges": false,
        "created": [ "f51u99" ],
        "updated": [ "f123u4" ],
        "destroyed": [ "f09u11" ]
    })
}

// ---------------------------------------------------------------------------
// 1. Thread/get through the Client helper
// ---------------------------------------------------------------------------

#[tokio::test]
async fn thread_get_returns_the_record_the_server_listed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Thread/get", thread_get_one()).await;

    let thread = client
        .thread_get("f123u4")
        .await
        .unwrap()
        .expect("the record must be returned");

    assert_eq!(thread.id(), "f123u4");
    assert_eq!(thread.email_ids(), ["eaa623", "f782cbb"]);

    // --- wire ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/get");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["ids"], json!(["f123u4"]));
    // `Client::thread_get` never sends `properties`, so the server decides.
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn thread_get_pops_the_last_list_entry_when_the_server_returns_several() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §3.1.1 lists two records for the two ids that were asked for.
    mount_one(&server, "Thread/get", rfc_thread_get_response()).await;

    // `Client::thread_get` is `take_list().pop()`, so with a spec-following
    // server that answers every requested id it hands back the *last* record,
    // not the one whose id was passed in. Characterised here rather than
    // silently relied upon (see the COVERAGE_PLAN findings log).
    let thread = client.thread_get("f123u4").await.unwrap().unwrap();
    assert_eq!(
        thread.id(),
        "f41u44",
        "`thread_get` pops the tail of `list`, so it is only safe for single-id fetches"
    );
    assert_eq!(thread.email_ids(), ["82cf7bb"]);
}

#[tokio::test]
async fn thread_get_of_an_unknown_id_returns_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1: ids the server does not have go in `notFound`, and the
    // client surfaces that as `Ok(None)` rather than an error.
    mount_one(
        &server,
        "Thread/get",
        json!({
            "accountId": ACCOUNT_ID,
            "state": "f6a7e214",
            "list": [],
            "notFound": [ "f123u4" ]
        }),
    )
    .await;

    // `Thread` is not `PartialEq`, so the absence is asserted with `is_none`.
    assert!(client.thread_get("f123u4").await.unwrap().is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/get");
    assert_eq!(call[1]["ids"], json!(["f123u4"]));
}

#[tokio::test]
async fn thread_get_method_error_is_propagated() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "unknownMethod")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = client.thread_get("f123u4").await.unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
    // The call still went on the wire under the RFC method name.
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Thread/get"
    );
}

// ---------------------------------------------------------------------------
// 2. Thread/get through the Request builder
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_get_thread_sends_explicit_properties_and_reads_the_whole_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Thread/get", rfc_thread_get_response()).await;

    let response = {
        let mut request = client.build();
        request
            .get_thread()
            .ids(["f123u4", "f41u44"])
            .properties([Property::Id, Property::EmailIds]);
        request.send_get_thread().await.unwrap()
    };

    assert_eq!(response.state(), "f6a7e214");
    assert_eq!(response.list().len(), 2);
    assert_eq!(response.list()[0].id(), "f123u4");
    assert_eq!(response.list()[0].email_ids(), ["eaa623", "f782cbb"]);
    assert_eq!(response.list()[1].id(), "f41u44");
    assert_eq!(response.list()[1].email_ids(), ["82cf7bb"]);
    assert!(response.not_found().is_empty());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/get");
    assert_eq!(call[1]["ids"], json!(["f123u4", "f41u44"]));
    assert_eq!(call[1]["properties"], json!(["id", "emailIds"]));
}

#[tokio::test]
async fn request_get_thread_reports_not_found_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Thread/get",
        json!({
            "accountId": ACCOUNT_ID,
            "state": "f6a7e214",
            "list": [ { "id": "f123u4", "emailIds": [] } ],
            "notFound": [ "f41u44", "f99u99" ]
        }),
    )
    .await;

    let mut request = client.build();
    request.get_thread().ids(["f123u4", "f41u44", "f99u99"]);
    let mut response = request.send_get_thread().await.unwrap();

    assert_eq!(response.list().len(), 1);
    // A thread with no emails is legal; `emailIds` is an empty array, not null.
    assert!(response.list()[0].email_ids().is_empty());
    assert_eq!(response.not_found(), ["f41u44", "f99u99"]);
    assert_eq!(response.take_not_found(), ["f41u44", "f99u99"]);
}

// ---------------------------------------------------------------------------
// 3. Thread/changes
//
// RFC 8621 §3.2: "This is a standard '/changes' method as described in
// [RFC8620], Section 5.2." There is no `Client::thread_changes`; the
// `Request::changes_thread` builder is the only entry point.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn thread_changes_reports_the_three_id_lists() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Thread/changes", thread_changes_body("f6a7e214")).await;

    let mut request = client.build();
    request.changes_thread("f6a7e214");
    let response = request.send_changes_thread().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "f6a7e214");
    assert_eq!(response.new_state(), "f6a7e215");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["f51u99"]);
    assert_eq!(response.updated(), ["f123u4"]);
    assert_eq!(response.destroyed(), ["f09u11"]);
    // RFC 8620 §5.2: `totalChanges` is the sum of the three lists.
    assert_eq!(response.total_changes(), 3);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/changes");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["sinceState"], "f6a7e214");
    // `maxChanges` is optional and `changes_thread` does not set it.
    assert!(call[1].get("maxChanges").is_none());
}

#[tokio::test]
async fn thread_changes_with_max_changes_and_more_changes_pending() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Thread/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "f6a7e215",
            "newState": "f6a7e216",
            "hasMoreChanges": true,
            "created": [],
            "updated": [ "f123u4", "f41u44" ],
            "destroyed": []
        }),
    )
    .await;

    let mut request = client.build();
    request.changes_thread("f6a7e215").max_changes(2);
    let response = request.send_changes_thread().await.unwrap();

    assert!(response.has_more_changes());
    assert_eq!(response.updated(), ["f123u4", "f41u44"]);
    assert!(response.created().is_empty());
    assert!(response.destroyed().is_empty());
    assert_eq!(response.total_changes(), 2);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Thread/changes");
    assert_eq!(call[1]["sinceState"], "f6a7e215");
    assert_eq!(call[1]["maxChanges"], 2);
}

#[tokio::test]
async fn thread_changes_method_error_is_propagated() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.2: an unparseable `sinceState` the server cannot map to a
    // previous state is a `cannotCalculateChanges` method error.
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "cannotCalculateChanges")],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.changes_thread("bogus-state");
    let err = request.send_changes_thread().await.unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Thread/changes"
    );
}

// ---------------------------------------------------------------------------
// 4. `thread::Property` round-trip and Display
// ---------------------------------------------------------------------------

#[test]
fn thread_property_display_and_serde_agree() {
    assert_eq!(Property::Id.to_string(), "id");
    assert_eq!(Property::EmailIds.to_string(), "emailIds");

    for (name, property) in [
        ("id", Property::Id),
        ("emailIds", Property::EmailIds),
    ] {
        let encoded = serde_json::to_string(&property).unwrap();
        assert_eq!(encoded, format!("\"{name}\""));
        let decoded: Property = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, property);
    }
}

#[tokio::test]
async fn thread_property_names_are_what_the_server_sees() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Thread/get", thread_get_one()).await;

    let mut request = client.build();
    request
        .get_thread()
        .ids(["f123u4"])
        .properties([Property::EmailIds, Property::Id]);
    request.send_get_thread().await.unwrap();

    // The wire carries the Display spelling, in the order supplied.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["properties"], json!(["emailIds", "id"]));
}
