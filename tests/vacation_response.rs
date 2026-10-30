//! P5 — vacation-domain integration tests: **VacationResponse**.
//!
//! Drives the `Client::vacation_response_*` helpers from
//! `src/vacation_response/helpers.rs` and the matching `Request::` builders
//! against a `wiremock` HTTP server, using the shared fixture layer in
//! `tests/common/mod.rs`. The RFC gives no worked example for
//! VacationResponse/get or /set, so the fixtures here are the minimal
//! RFC-correct objects for RFC 8621 §8, each cited.
//!
//! Coverage targets: `vacation_response/helpers.rs` (every `Client::` helper
//! plus the `Request::{get,set}_vacation_response` builders),
//! `vacation_response/get.rs` (record accessors), `vacation_response/set.rs`
//! (patch keys, the `from_timestamp` conversion, the seeded-empty behaviour)
//! and `vacation_response/mod.rs` (`Property`).
//!
//! Contract facts this file pins:
//!
//! * RFC 8621 §8.1: there is exactly one object per account and its id is
//!   `"singleton"` — every helper addresses that id, never a caller-supplied one.
//! * `VacationResponse::new()` seeds the string members with `""` and the date
//!   members with the epoch, which `string_not_set`/`date_not_set` suppress.
//!   Once a setter assigns `None` the member rides along as an explicit JSON
//!   `null` instead (the date analogue of Finding #39) — pinned by
//!   [`vacation_response_set_dates_with_none_clears_both_members`].
//! * RFC 8621 §8.2 defines no dedicated "bad date range" SetError, so a server
//!   rejects one with the standard `invalidProperties` (RFC 8620 §5.3).

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use common::{
    connected_client, jmap_response, jmap_response_with_created_ids, mount_jmap, single_jmap_request,
    ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::set::SetErrorType;
use jmap_client::vacation_response::Property;
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

/// The property list `Client::vacation_response_get` expects, so the
/// "no properties" case can name the type instead of leaving it ambiguous.
type VacationProps = Vec<Property>;

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

/// A `VacationResponse/get` result envelope (RFC 8620 §5.1: `state`, `list`
/// and `notFound` are all required members — Finding #8).
fn vacation_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "42",
        "list": list,
        "notFound": not_found
    })
}

/// RFC 8621 §8 — the enabled singleton: a vacation window from
/// 2024-12-24T00:00:00Z to 2025-01-05T00:00:00Z with both body forms.
fn enabled_singleton() -> Value {
    json!({
        "id": "singleton",
        "isEnabled": true,
        "fromDate": "2024-12-24T00:00:00Z",
        "toDate": "2025-01-05T00:00:00Z",
        "subject": "Out of the office",
        "textBody": "I am on holiday until 5 January and will reply when I return.",
        "htmlBody": "<p>I am on holiday until 5 January and will reply when I return.</p>"
    })
}

/// RFC 8621 §8 — the disabled singleton with no window and no bodies.
/// `fromDate`/`toDate` null means "immediately" / "indefinitely" if it were
/// ever enabled; a null `subject`/`textBody`/`htmlBody` means the server
/// supplies its own.
fn disabled_singleton() -> Value {
    json!({
        "id": "singleton",
        "isEnabled": false,
        "fromDate": null,
        "toDate": null,
        "subject": null,
        "textBody": null,
        "htmlBody": null
    })
}

// ---------------------------------------------------------------------------
// 1. VacationResponse/get (RFC 8621 §8.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vacation_response_get_parses_the_enabled_singleton() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(vec![enabled_singleton()], vec![]),
    )
    .await;

    let vacation = client
        .vacation_response_get(None::<VacationProps>)
        .await
        .unwrap()
        .expect("the singleton must come back");

    assert_eq!(vacation.id(), Some("singleton"));
    assert!(vacation.is_enabled());
    // 2024-12-24T00:00:00Z / 2025-01-05T00:00:00Z
    assert_eq!(vacation.from_date(), Some(1_734_998_400));
    assert_eq!(vacation.to_date(), Some(1_736_035_200));
    assert_eq!(vacation.subject(), Some("Out of the office"));
    assert_eq!(
        vacation.text_body(),
        Some("I am on holiday until 5 January and will reply when I return.")
    );
    assert_eq!(
        vacation.html_body(),
        Some("<p>I am on holiday until 5 January and will reply when I return.</p>")
    );

    // --- what actually went over the wire (RFC 8620 §3.2/§5.1) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "VacationResponse/get",
        "method name must be on the wire"
    );
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        call[1]["ids"],
        json!(["singleton"]),
        "RFC 8621 §8.1: the object's id is always `singleton`"
    );
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn vacation_response_get_parses_the_disabled_singleton() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(vec![disabled_singleton()], vec![]),
    )
    .await;

    let vacation = client
        .vacation_response_get(None::<VacationProps>)
        .await
        .unwrap()
        .expect("the singleton must come back");

    assert!(!vacation.is_enabled());
    assert_eq!(vacation.from_date(), None);
    assert_eq!(vacation.to_date(), None);
    assert_eq!(vacation.subject(), None);
    assert_eq!(vacation.text_body(), None);
    assert_eq!(vacation.html_body(), None);
}

#[tokio::test]
async fn vacation_response_get_sends_only_the_requested_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(
            vec![json!({ "id": "singleton", "isEnabled": true })],
            vec![],
        ),
    )
    .await;

    let vacation = client
        .vacation_response_get(Some([Property::Id, Property::IsEnabled]))
        .await
        .unwrap()
        .expect("the singleton must come back");

    assert_eq!(vacation.id(), Some("singleton"));
    assert!(vacation.is_enabled());
    // Not requested, so absent.
    assert_eq!(vacation.subject(), None);
    assert_eq!(vacation.from_date(), None);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "VacationResponse/get");
    assert_eq!(
        call[1]["properties"],
        json!(["id", "isEnabled"]),
        "the RFC property names must go on the wire verbatim"
    );
}

#[tokio::test]
async fn vacation_response_get_returns_none_when_the_singleton_is_missing() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A server that has never had a vacation response configured.
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(vec![], vec!["singleton"]),
    )
    .await;

    assert!(client
        .vacation_response_get(None::<VacationProps>)
        .await
        .unwrap()
        .is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "VacationResponse/get");
}

#[tokio::test]
async fn vacation_response_get_missing_a_required_member_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1 makes `list` mandatory (Finding #8).
    mount_one(
        &server,
        "VacationResponse/get",
        json!({ "accountId": ACCOUNT_ID, "state": "42", "notFound": [] }),
    )
    .await;

    let err = client
        .vacation_response_get(None::<VacationProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. VacationResponse/set — enable / disable (RFC 8621 §8.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vacation_response_enable_sends_the_full_patch() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "42",
            "newState": "43",
            "updated": { "singleton": enabled_singleton() }
        }),
    )
    .await;

    let vacation = client
        .vacation_response_enable(
            "Out of the office",
            Some("I am on holiday until 5 January and will reply when I return."),
            Some("<p>I am on holiday until 5 January and will reply when I return.</p>"),
        )
        .await
        .unwrap()
        .expect("the updated singleton must come back");

    assert!(vacation.is_enabled());
    assert_eq!(vacation.subject(), Some("Out of the office"));

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "VacationResponse/set",
        "method name must be on the wire"
    );
    assert_eq!(
        call[1]["update"]["singleton"],
        json!({
            "isEnabled": true,
            "subject": "Out of the office",
            "textBody": "I am on holiday until 5 January and will reply when I return.",
            "htmlBody": "<p>I am on holiday until 5 January and will reply when I return.</p>"
        }),
        "the seeded empty strings/dates must be suppressed, and the singleton \
         id must be the patch key"
    );
    assert!(call[1]["update"]["singleton"].get("fromDate").is_none());
    assert!(call[1]["update"]["singleton"].get("toDate").is_none());
}

#[tokio::test]
async fn vacation_response_enable_with_a_null_body_sends_an_explicit_null() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "42",
            "newState": "43",
            "updated": { "singleton": null }
        }),
    )
    .await;

    let vacation = client
        .vacation_response_enable("Out of the office", None::<String>, Some("<p>Away</p>"))
        .await
        .unwrap();
    // The RFC-legal `null` update echo surfaces as `Ok(None)`.
    assert!(vacation.is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["update"]["singleton"],
        json!({
            "isEnabled": true,
            "subject": "Out of the office",
            "textBody": null,
            "htmlBody": "<p>Away</p>"
        }),
        "an unset Option<String> member serialises as explicit `null`, which \
         RFC 8620 §5.3 defines as \"clear this member\" (cf. Finding #39)"
    );
}

#[tokio::test]
async fn vacation_response_disable_sends_is_enabled_false() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "43",
            "newState": "44",
            "updated": { "singleton": disabled_singleton() }
        }),
    )
    .await;

    let vacation = client
        .vacation_response_disable()
        .await
        .unwrap()
        .expect("the updated singleton must come back");
    assert!(!vacation.is_enabled());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "VacationResponse/set", "method name must be on the wire");
    assert_eq!(
        call[1]["update"]["singleton"],
        json!({ "isEnabled": false }),
        "disabling touches only `isEnabled`; the bodies and window are kept"
    );
}

#[tokio::test]
async fn vacation_response_set_dates_emits_both_timestamps() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "44",
            "newState": "45",
            "updated": { "singleton": enabled_singleton() }
        }),
    )
    .await;

    let vacation = client
        .vacation_response_set_dates(Some(1_734_998_400), Some(1_736_035_200))
        .await
        .unwrap()
        .expect("the updated singleton must come back");
    assert!(vacation.is_enabled());
    assert_eq!(vacation.from_date(), Some(1_734_998_400));
    assert_eq!(vacation.to_date(), Some(1_736_035_200));

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "VacationResponse/set",
        "method name must be on the wire"
    );
    assert_eq!(
        call[1]["update"]["singleton"],
        json!({
            "isEnabled": true,
            "fromDate": "2024-12-24T00:00:00Z",
            "toDate": "2025-01-05T00:00:00Z"
        }),
        "unix seconds must convert to an RFC 3339 UTCDate, never an integer"
    );
}

#[tokio::test]
async fn vacation_response_set_dates_with_none_clears_both_members() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "45",
            "newState": "46",
            "updated": {
                "singleton": {
                    "id": "singleton",
                    "isEnabled": true,
                    "fromDate": null,
                    "toDate": null,
                    "subject": "Out of the office",
                    "textBody": null,
                    "htmlBody": null
                }
            }
        }),
    )
    .await;

    // §8: a null fromDate is "effective immediately", a null toDate is
    // "effective indefinitely".
    let vacation = client
        .vacation_response_set_dates(None, None)
        .await
        .unwrap()
        .expect("the updated singleton must come back");
    assert!(vacation.is_enabled());
    assert_eq!(vacation.from_date(), None);
    assert_eq!(vacation.to_date(), None);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["update"]["singleton"],
        json!({
            "isEnabled": true,
            "fromDate": null,
            "toDate": null
        }),
        "passing None clears the member with an explicit `null` rather than \
         omitting it — the date analogue of Finding #39"
    );
}

// ---------------------------------------------------------------------------
// 3. VacationResponse/set — create / destroy (RFC 8621 §8.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vacation_response_create_uses_a_creation_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv(
                "VacationResponse/set",
                "s0",
                json!({
                    "accountId": ACCOUNT_ID,
                    "oldState": "46",
                    "newState": "47",
                    "created": { "c0": enabled_singleton() }
                }),
            )],
            json!({ "c0": "singleton" }),
            SESSION_STATE,
        ),
    )
    .await;

    let created = client
        .vacation_response_create(
            "Out of the office",
            Some("I am on holiday."),
            Some("<p>I am on holiday.</p>"),
        )
        .await
        .unwrap();

    assert_eq!(created.id(), Some("singleton"));
    assert!(created.is_enabled());
    assert_eq!(created.subject(), Some("Out of the office"));

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "VacationResponse/set",
        "method name must be on the wire"
    );
    assert_eq!(
        call[1]["create"]["c0"],
        json!({
            "isEnabled": true,
            "subject": "Out of the office",
            "textBody": "I am on holiday.",
            "htmlBody": "<p>I am on holiday.</p>"
        })
    );
    assert!(call[1].get("update").is_none());
    assert!(call[1].get("destroy").is_none());
}

#[tokio::test]
async fn vacation_response_create_reports_the_singleton_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §8.1: "There MUST only be exactly one VacationResponse object
    // in an account" — a second create is rejected with `singleton`.
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "47",
            "newState": "47",
            "notCreated": {
                "c0": {
                    "type": "singleton",
                    "description": "A vacation response already exists"
                }
            }
        }),
    )
    .await;

    let err = client
        .vacation_response_create("Another one", None::<String>, None::<String>)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::Singleton);
    assert_eq!(
        set_err.description(),
        Some("A vacation response already exists")
    );
}

#[tokio::test]
async fn vacation_response_set_reports_an_invalid_date_range() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §8.2 defines no dedicated error for an inverted window, so a
    // conformant server answers with the standard `invalidProperties`
    // (RFC 8620 §5.3) naming both members.
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "47",
            "newState": "47",
            "notUpdated": {
                "singleton": {
                    "type": "invalidProperties",
                    "properties": ["fromDate", "toDate"],
                    "description": "toDate must be after fromDate"
                }
            }
        }),
    )
    .await;

    let err = client
        .vacation_response_set_dates(Some(1_736_035_200), Some(1_734_998_400))
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::InvalidProperties);
    assert_eq!(
        set_err.properties(),
        Some(&["fromDate".to_string(), "toDate".to_string()][..])
    );
    assert_eq!(
        set_err.description(),
        Some("toDate must be after fromDate")
    );
}

#[tokio::test]
async fn vacation_response_destroy_removes_the_singleton() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "47",
            "newState": "48",
            "destroyed": ["singleton"]
        }),
    )
    .await;

    client.vacation_response_destroy().await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "VacationResponse/set",
        "method name must be on the wire"
    );
    assert_eq!(call[1]["destroy"], json!(["singleton"]));
}

#[tokio::test]
async fn vacation_response_destroy_reports_not_destroyed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "48",
            "newState": "48",
            "notDestroyed": {
                "singleton": { "type": "forbidden", "description": "Server-managed" }
            }
        }),
    )
    .await;

    let err = client.vacation_response_destroy().await.unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::Forbidden);
    assert_eq!(set_err.description(), Some("Server-managed"));
}

#[tokio::test]
async fn vacation_response_set_with_a_stale_if_in_state_reports_state_mismatch() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "stateMismatch")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request
        .set_vacation_response()
        .if_in_state("42")
        .update("singleton")
        .is_enabled(true)
        .subject(Some("Out of the office"));
    let err = request.send_set_vacation_response().await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}")
    };
    assert_eq!(
        method_err.error(),
        &jmap_client::core::error::MethodErrorType::StateMismatch
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "VacationResponse/set");
    assert_eq!(call[1]["ifInState"], "42");
}

// ---------------------------------------------------------------------------
// 4. Cross-method chain — read then change in one request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vacation_response_get_then_set_in_one_request() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A heterogeneous batch: `send_single` cannot read this (Finding #49), so
    // the test goes through `Request::send()` and unwraps per call id.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "VacationResponse/get",
                    "s0",
                    vacation_get_body(vec![disabled_singleton()], vec![]),
                ),
                inv(
                    "VacationResponse/set",
                    "s1",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "oldState": "48",
                        "newState": "49",
                        "updated": { "singleton": enabled_singleton() }
                    }),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_vacation_response().ids(["singleton"]);
    request
        .set_vacation_response()
        .update("singleton")
        .is_enabled(true)
        .subject(Some("Out of the office"))
        .from_date(Some(1_734_998_400))
        .to_date(Some(1_736_035_200));

    let mut response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 2);

    let mut get_response = response
        .method_response_by_pos(0)
        .unwrap_get_vacation_response()
        .unwrap();
    let list = get_response.take_list();
    assert_eq!(list.len(), 1);
    assert!(!list[0].is_enabled());

    let mut set_response = response
        .method_response_by_pos(0)
        .unwrap_set_vacation_response()
        .unwrap();
    let updated = set_response
        .updated("singleton")
        .unwrap()
        .expect("the updated singleton must come back");
    assert!(updated.is_enabled());
    assert_eq!(updated.from_date(), Some(1_734_998_400));
    assert_eq!(set_response.new_state(), "49");

    // Both calls must be on the wire, in order, with distinct call ids.
    let bodies = common::jmap_request_bodies(&server).await;
    let calls = bodies[0]["methodCalls"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "VacationResponse/get");
    assert_eq!(calls[0][2], "s0");
    assert_eq!(calls[1][0], "VacationResponse/set");
    assert_eq!(calls[1][2], "s1");
    assert_eq!(calls[1][1]["update"]["singleton"]["isEnabled"], true);
    assert_eq!(
        calls[1][1]["update"]["singleton"]["fromDate"],
        "2024-12-24T00:00:00Z"
    );
}

// ---------------------------------------------------------------------------
// 5. Enum coverage
// ---------------------------------------------------------------------------

#[test]
fn vacation_response_property_display_matches_the_rfc_property_names() {
    // RFC 8621 §8 property names.
    let expected = [
        (Property::Id, "id"),
        (Property::IsEnabled, "isEnabled"),
        (Property::FromDate, "fromDate"),
        (Property::ToDate, "toDate"),
        (Property::Subject, "subject"),
        (Property::TextBody, "textBody"),
        (Property::HtmlBody, "htmlBody"),
    ];
    for (property, name) in expected {
        assert_eq!(property.to_string(), name);
        assert_eq!(
            serde_json::to_value(property).unwrap(),
            json!(name),
            "serde and Display must agree"
        );
    }
}

// ---------------------------------------------------------------------------
// The `Request::send_get_vacation_response` convenience wrapper
//
// `Request::send_get_vacation_response` is a thin `self.send_single().await`
// (src/vacation_response/helpers.rs:144-146). `Client::vacation_response_get`
// calls `send_single` directly, so nothing else in the crate reaches the
// wrapper by name. The test asks for the singleton through the typed builder,
// sends it through the wrapper, asserts the parsed record, and spies on the
// wire so it cannot pass on an empty `methodCalls` array (Finding #31).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_get_vacation_response_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(vec![enabled_singleton()], vec![]),
    )
    .await;

    let mut request = client.build();
    request.get_vacation_response().ids(["singleton"]);
    let mut response = request.send_get_vacation_response().await.unwrap();

    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    assert_eq!(response.not_found(), [] as [&str; 0]);
    let list = response.take_list();
    assert_eq!(list.len(), 1, "the singleton is the only record");
    assert!(list[0].is_enabled());
    assert_eq!(list[0].subject(), Some("Out of the office"));

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "VacationResponse/get",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    assert_eq!(body["methodCalls"][0][1]["ids"], json!(["singleton"]));
}

#[tokio::test]
async fn send_get_vacation_response_of_a_missing_singleton_is_an_empty_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §8: an account with no vacation response configured answers
    // with an empty `list`, which the wrapper hands back intact.
    mount_one(
        &server,
        "VacationResponse/get",
        vacation_get_body(vec![], vec!["singleton"]),
    )
    .await;

    let mut request = client.build();
    request.get_vacation_response().ids(["singleton"]);
    let mut response = request.send_get_vacation_response().await.unwrap();

    assert!(response.take_list().is_empty());
    assert_eq!(response.not_found(), ["singleton"]);
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "VacationResponse/get"
    );
}
