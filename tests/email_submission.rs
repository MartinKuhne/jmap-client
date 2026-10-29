//! P5 — submission-domain integration tests: **EmailSubmission**.
//!
//! Drives the `Client::email_submission_*` helpers from
//! `src/email_submission/helpers.rs` and the matching `Request::` builders
//! against a `wiremock` HTTP server, using the shared fixture layer in
//! `tests/common/mod.rs`. Canned payloads are the RFC 8621 §7 example bodies
//! wherever the RFC has one (each fixture cites its section); where the RFC
//! gives a shape but no example, the payload is the minimal RFC-correct object.
//!
//! Coverage targets: `email_submission/helpers.rs` (every `Client::` helper
//! plus the `Request::{get,changes,query,query_changes,set}_email_submission`
//! builders), `email_submission/get.rs` (record + `Address`/`DeliveryStatus`
//! accessors), `email_submission/set.rs` (patch keys, `Envelope`,
//! `onSuccessUpdateEmail`/`onSuccessDestroyEmail`), `email_submission/query.rs`
//! (filters + comparators) and `email_submission/mod.rs` (`UndoStatus`,
//! `Delivered`, `Displayed`, `Property`).
//!
//! Two contract facts from `COVERAGE_PLAN.md` shape this file:
//!
//! * `Client::email_submission_get` is `take_list().pop()`, so it is only
//!   correct for a single-id fetch (same trap as `Client::thread_get`,
//!   Finding #48) — pinned by
//!   [`email_submission_get_pops_the_last_record_when_the_server_returns_several`].
//! * `Request::send_single<T>` deserialises EVERY response in the envelope as
//!   `T` and then pops the **last** one (Finding #49). RFC 8621 §7.5 says an
//!   `EmailSubmission/set` that asks for `onSuccessUpdateEmail`/
//!   `onSuccessDestroyEmail` returns a second, implicit `Email/set` response
//!   *after* the submission response under the same call id, so the client
//!   helper lands on the wrong body — pinned by
//!   [`email_submission_create_with_the_rfc_implicit_email_set_response_lands_on_the_wrong_body`].

mod common;

use common::{
    connected_client, jmap_response, jmap_response_with_created_ids, mount_jmap, jmap_request_bodies,
    single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::error::MethodErrorType;
use jmap_client::core::query::{Comparator, Filter};
use jmap_client::core::set::SetErrorType;
use jmap_client::email_submission::query::{
    Comparator as SubmissionComparator, Filter as SubmissionFilter,
};
use jmap_client::email_submission::{Address, Delivered, Displayed, Property, UndoStatus};
use jmap_client::{Error, Set};
use serde_json::{json, Value};
use wiremock::MockServer;

/// The property list `Client::email_submission_get` expects, so the
/// "no properties" case can name the type instead of leaving it ambiguous.
type SubmissionProps = Vec<Property>;

/// The sort list `Client::email_submission_query` expects.
type SubmissionSort = Vec<Comparator<SubmissionComparator>>;

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

/// A `EmailSubmission/get` result envelope (RFC 8620 §5.1: `state`, `list`
/// and `notFound` are all required members of the response — Finding #8).
fn submission_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "104",
        "list": list,
        "notFound": not_found
    })
}

/// A fully populated EmailSubmission per RFC 8621 §7: every property the
/// object model exposes, with the envelope `Address.parameters` map of §7
/// ("each key in the object is a parameter name, and the value is either the
/// parameter value or null if the parameter does not take a value") and the
/// per-recipient `deliveryStatus` map whose `smtpReply` is the RFC's own
/// collapsed multi-line SMTP response (§7, "550 5.7.1 Our system has detected
/// that this message is likely spam.").
fn rfc_submission(id: &str) -> Value {
    json!({
        "id": id,
        "identityId": "I64588216",
        "emailId": "M7f6ed5bcfd7e2604d1753f6c",
        "threadId": "Tf14a5b2c9d3e0a7b6c5d4e3f2",
        "envelope": {
            "mailFrom": {
                "email": "john@example.com",
                "parameters": { "SIZE": "24598" }
            },
            "rcptTo": [
                { "email": "jane@example.com", "parameters": null },
                { "email": "jim@example.org", "parameters": { "NOTIFY": "SUCCESS,FAILURE", "UTF8ONLY": null } }
            ]
        },
        "sendAt": "2024-06-01T09:15:00Z",
        "undoStatus": "pending",
        "deliveryStatus": {
            "jane@example.com": {
                "smtpReply": "250 2.1.5 Ok",
                "delivered": "yes",
                "displayed": "yes"
            },
            "jim@example.org": {
                "smtpReply": "550 5.7.1 Our system has detected that this message is likely spam.",
                "delivered": "no",
                "displayed": "unknown"
            }
        },
        "dsnBlobIds": ["Bdsn1", "Bdsn2"],
        "mdnBlobIds": ["Bmdn1"]
    })
}

/// The `EmailSubmission/set` success body for the RFC 8621 §7.5.1 walkthrough.
///
/// The RFC's own creation id is `k1490`; the client's `SetRequest::create()`
/// emits `c0` (Finding #51), so the fixture keys on `c0` to keep the
/// round-trip about the *response shape* rather than the id spelling.
fn submission_created_body() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "012421s6-8nrq-4ps4-n0p4-9330r951ns21",
        "newState": "355421f6-8aed-4cf4-a0c4-7377e951af36",
        "created": {
            "c0": { "id": "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0" }
        }
    })
}

/// The implicit `Email/set` response RFC 8621 §7.5.1 says MUST follow the
/// `EmailSubmission/set` response under the *same* method call id.
fn implicit_email_set_response() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "778193",
        "newState": "778197",
        "updated": {
            "M7f6ed5bcfd7e2604d1753f6c": null
        }
    })
}

/// A `EmailSubmission/query` result envelope (RFC 8620 §5.5: `queryState`,
/// `position` and `ids` are required).
fn query_body(ids: &[&str]) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "queryState": "104:0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ids,
        "total": ids.len()
    })
}

// ---------------------------------------------------------------------------
// 1. EmailSubmission/get (RFC 8621 §7.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_get_parses_the_full_rfc_object() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(vec![rfc_submission("ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0")], vec![]),
    )
    .await;

    let submission = client
        .email_submission_get(
            "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0",
            None::<SubmissionProps>,
        )
        .await
        .unwrap()
        .expect("the submission record must come back");

    // Identity of the submission (§7).
    assert_eq!(
        submission.id(),
        Some("ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0")
    );
    assert_eq!(submission.identity_id(), Some("I64588216"));
    assert_eq!(submission.email_id(), Some("M7f6ed5bcfd7e2604d1753f6c"));
    assert_eq!(submission.thread_id(), Some("Tf14a5b2c9d3e0a7b6c5d4e3f2"));

    // sendAt: "the date the submission was/will be released for delivery".
    assert_eq!(submission.send_at(), Some(1_717_233_300));
    assert_eq!(submission.undo_status(), Some(&UndoStatus::Pending));

    // Envelope: MAIL FROM with an ESMTP parameter (§7, Address.parameters).
    let mail_from = submission.mail_from().expect("mailFrom must be present");
    assert_eq!(mail_from.email(), "john@example.com");
    assert_eq!(mail_from.parameter("SIZE"), Some("24598"));
    assert!(mail_from.has_parameter("SIZE"));
    assert!(!mail_from.has_parameter("UTF8ONLY"));

    // Envelope: RCPT TO list, one with parameters, one without.
    let rcpt_to = submission.rcpt_to().expect("rcptTo must be present");
    assert_eq!(rcpt_to.len(), 2);
    assert_eq!(rcpt_to[0].email(), "jane@example.com");
    assert!(!rcpt_to[0].has_parameter("NOTIFY"));
    assert_eq!(rcpt_to[1].email(), "jim@example.org");
    assert_eq!(rcpt_to[1].parameter("NOTIFY"), Some("SUCCESS,FAILURE"));
    // A value-less parameter is present but carries no string (§7).
    assert!(rcpt_to[1].has_parameter("UTF8ONLY"));
    assert_eq!(rcpt_to[1].parameter("UTF8ONLY"), None);

    // deliveryStatus: map from recipient address → DeliveryStatus (§7).
    let delivery = submission
        .delivery_status()
        .expect("deliveryStatus must be present");
    assert_eq!(delivery.len(), 2);

    let delivered = submission
        .delivery_status_email("jane@example.com")
        .expect("jane must have a delivery status");
    assert_eq!(delivered.smtp_reply(), "250 2.1.5 Ok");
    assert_eq!(delivered.delivered(), &Delivered::Yes);
    assert_eq!(delivered.displayed(), &Displayed::Yes);

    let bounced = submission
        .delivery_status_email("jim@example.org")
        .expect("jim must have a delivery status");
    assert_eq!(
        bounced.smtp_reply(),
        "550 5.7.1 Our system has detected that this message is likely spam."
    );
    assert_eq!(bounced.delivered(), &Delivered::No);
    assert_eq!(bounced.displayed(), &Displayed::Unknown);

    // An unknown recipient is simply absent from the map.
    assert!(submission.delivery_status_email("nobody@example.com").is_none());

    // DSN/MDN blob id lists (§7, "in order of receipt, oldest first").
    assert_eq!(submission.dsn_blob_ids(), Some(&["Bdsn1".to_string(), "Bdsn2".to_string()][..]));
    assert_eq!(submission.mdn_blob_ids(), Some(&["Bmdn1".to_string()][..]));

    // --- what actually went over the wire (RFC 8620 §3.2/§5.1) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/get", "method name must be on the wire");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["ids"], json!(["ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"]));
    // No `properties` argument was given, so none may be serialised (§5.1:
    // "if null … the server must return all properties").
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn email_submission_get_sends_only_the_requested_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A server honouring `properties` returns just those members.
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(
            vec![json!({
                "id": "ES-1",
                "undoStatus": "final",
                "deliveryStatus": null
            })],
            vec![],
        ),
    )
    .await;

    let submission = client
        .email_submission_get(
            "ES-1",
            Some(vec![
                Property::Id,
                Property::UndoStatus,
                Property::DeliveryStatus,
            ]),
        )
        .await
        .unwrap()
        .expect("the record must come back");

    assert_eq!(submission.id(), Some("ES-1"));
    assert_eq!(submission.undo_status(), Some(&UndoStatus::Final));
    // §7: "This property MAY not be supported by all servers, in which case it
    // will remain null."
    assert!(submission.delivery_status().is_none());
    assert!(submission.mail_from().is_none());
    assert!(submission.rcpt_to().is_none());
    assert_eq!(submission.send_at(), None);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/get");
    assert_eq!(
        call[1]["properties"],
        json!(["id", "undoStatus", "deliveryStatus"]),
        "the RFC property names must go on the wire verbatim"
    );
}

#[tokio::test]
async fn email_submission_get_returns_none_when_the_id_is_not_found() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(vec![], vec!["ES-gone"]),
    )
    .await;

    let submission = client
        .email_submission_get("ES-gone", None::<SubmissionProps>)
        .await
        .unwrap();
    assert!(submission.is_none(), "a notFound id must surface as None");

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/get");
}

#[tokio::test]
async fn email_submission_get_pops_the_last_record_when_the_server_returns_several() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A spec-following server answers every id it was given.
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(
            vec![
                rfc_submission("ES-first"),
                rfc_submission("ES-second"),
            ],
            vec![],
        ),
    )
    .await;

    // `email_submission_get` is `take_list().pop()`, so the *last* record is
    // returned regardless of which id was asked for. Multi-id reads must go
    // through `Request::get_email_submission()` and index the list instead.
    let submission = client
        .email_submission_get("ES-first", None::<SubmissionProps>)
        .await
        .unwrap()
        .expect("a record must come back");
    assert_eq!(submission.id(), Some("ES-second"));

    // The Request-level builder sees both records, in order.
    let mut request = client.build();
    request
        .get_email_submission()
        .ids(["ES-first", "ES-second"]);
    let response = request.send_get_email_submission().await.unwrap();
    assert_eq!(response.list().len(), 2);
    assert_eq!(response.list()[0].id(), Some("ES-first"));
    assert_eq!(response.list()[1].id(), Some("ES-second"));
    assert_eq!(response.not_found(), &[] as &[String]);
    assert_eq!(response.state(), "104");
}

#[tokio::test]
async fn email_submission_get_missing_a_required_member_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1 makes `notFound` mandatory; `GetResponse` declares it
    // non-Option, so its absence fails deserialisation (Finding #8).
    mount_one(
        &server,
        "EmailSubmission/get",
        json!({ "accountId": ACCOUNT_ID, "state": "104", "list": [] }),
    )
    .await;

    let err = client
        .email_submission_get("ES-1", None::<SubmissionProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. EmailSubmission/set — create (RFC 8621 §7.5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_create_round_trips_the_creation_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv("EmailSubmission/set", "s0", submission_created_body())],
            json!({ "c0": "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0" }),
            SESSION_STATE,
        ),
    )
    .await;

    let created = client
        .email_submission_create("M7f6ed5bcfd7e2604d1753f6c", "I64588216")
        .await
        .unwrap();

    assert_eq!(
        created.id(),
        Some("ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0")
    );

    // --- the request the server actually received (RFC 8621 §7.5.1) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/set", "method name must be on the wire");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        call[1]["create"]["c0"],
        json!({
            "emailId": "M7f6ed5bcfd7e2604d1753f6c",
            "identityId": "I64588216"
        }),
        "no envelope was requested, so only emailId + identityId may be sent \
         (§7: the server MUST generate the envelope from the Email in that case)"
    );
    assert!(call[1].get("update").is_none());
    assert!(call[1].get("destroy").is_none());
}

#[tokio::test]
async fn email_submission_create_envelope_puts_mail_from_and_rcpt_to_on_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "EmailSubmission/set", submission_created_body()).await;

    let created = client
        .email_submission_create_envelope(
            "M7f6ed5bcfd7e2604d1753f6c",
            "I64588216",
            Address::<Set>::new("john@example.com").parameter("SIZE", Some("24598")),
            ["jane@example.com", "jim@example.org"],
        )
        .await
        .unwrap();
    assert_eq!(
        created.id(),
        Some("ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0")
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/set");
    assert_eq!(
        call[1]["create"]["c0"]["envelope"],
        json!({
            "mailFrom": {
                "email": "john@example.com",
                "parameters": { "SIZE": "24598" }
            },
            "rcptTo": [
                { "email": "jane@example.com", "parameters": null },
                { "email": "jim@example.org", "parameters": null }
            ]
        }),
        "the envelope must be the SMTP MAIL FROM / RCPT TO pair of RFC 8621 §7"
    );
}

#[tokio::test]
async fn email_submission_create_reports_forbidden_to_send_from_the_rfc_example() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7.5.1: the localised `forbiddenToSend` SetError, verbatim.
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "012421s6-8nrq-4ps4-n0p4-9330r951ns21",
            "newState": "012421s6-8nrq-4ps4-n0p4-9330r951ns21",
            "notCreated": {
                "c0": {
                    "type": "forbiddenToSend",
                    "description": "Verzeihung, wegen verdaechtiger Aktivitaeten Ihres Benutzerkontos haben wir den Versand von Nachrichten gesperrt. Bitte wenden Sie sich fuer Hilfe an unser Support Team."
                }
            }
        }),
    )
    .await;

    let err = client
        .email_submission_create("M7f6ed5bcfd7e2604d1753f6c", "I64588216")
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::ForbiddenToSend);
    assert!(set_err
        .description()
        .unwrap()
        .starts_with("Verzeihung, wegen verdaechtiger"));
}

#[tokio::test]
async fn email_submission_create_reports_invalid_properties_for_an_unknown_email_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7.5: "If the Email or Identity id given cannot be found, the
    // submission creation is rejected with a standard invalidProperties
    // SetError."
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "104",
            "newState": "104",
            "notCreated": {
                "c0": { "type": "invalidProperties", "properties": ["emailId"] }
            }
        }),
    )
    .await;

    let err = client
        .email_submission_create("MdoesNotExist", "I64588216")
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::InvalidProperties);
    assert_eq!(set_err.properties(), Some(&["emailId".to_string()][..]));
}

#[tokio::test]
async fn email_submission_create_reports_no_recipients() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7.5: "noRecipients" — the envelope has no rcptTo addresses.
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "104",
            "newState": "104",
            "notCreated": {
                "c0": { "type": "noRecipients", "description": "Envelope has no recipients" }
            }
        }),
    )
    .await;

    let err = client
        .email_submission_create_envelope(
            "M7f6ed5bcfd7e2604d1753f6c",
            "I64588216",
            "john@example.com",
            Vec::<String>::new(),
        )
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::NoRecipients);
    assert_eq!(
        set_err.description(),
        Some("Envelope has no recipients")
    );
}

#[tokio::test]
async fn email_submission_create_with_the_rfc_implicit_email_set_response_lands_on_the_wrong_body() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7.5.1: "Note that there are two responses due to the implicit
    // Email/set call, but both have the same method call id".
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("EmailSubmission/set", "s0", submission_created_body()),
                inv("Email/set", "s0", implicit_email_set_response()),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    // `send_single` parses the whole envelope and pops the LAST response
    // (Finding #49), so the helper ends up holding the implicit Email/set body,
    // which has no `created` member at all.
    let err = client
        .email_submission_create("M7f6ed5bcfd7e2604d1753f6c", "I64588216")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Internal(_)),
        "expected Error::Internal(\"Id c0 not found.\"), got {err:?}"
    );

    // The RFC-correct way to read this batch is `Request::send()` plus an
    // explicit positional unwrap of the *first* response.
    let mut request = client.build();
    request
        .set_email_submission()
        .create()
        .email_id("M7f6ed5bcfd7e2604d1753f6c")
        .identity_id("I64588216");
    let mut response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 2);

    let mut submission_response = response
        .method_response_by_pos(0)
        .unwrap_set_email_submission()
        .expect("the first response is the EmailSubmission/set result");
    assert!(submission_response.has_created());
    assert_eq!(
        submission_response
            .created("c0")
            .unwrap()
            .id()
            .unwrap()
            .to_string(),
        "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"
    );
    assert_eq!(
        submission_response.new_state(),
        "355421f6-8aed-4cf4-a0c4-7377e951af36"
    );

    // And the implicit Email/set response is still there to be read.
    let email_response = response
        .method_response_by_pos(0)
        .unwrap_set_email()
        .expect("the second response is the implicit Email/set result");
    assert!(email_response.has_updated());
    assert_eq!(
        email_response.updated_ids().unwrap().collect::<Vec<_>>(),
        vec!["M7f6ed5bcfd7e2604d1753f6c"]
    );
}

// ---------------------------------------------------------------------------
// 3. EmailSubmission/set — update / destroy (RFC 8621 §7.5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_change_status_cancels_a_pending_submission() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // §7: "if in pending state, a client can attempt to cancel the submission
    // by setting this property to canceled".
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "104",
            "newState": "105",
            "updated": {
                "ES-1": {
                    "id": "ES-1",
                    "identityId": "I64588216",
                    "emailId": "M7f6ed5bcfd7e2604d1753f6c",
                    "undoStatus": "canceled"
                }
            }
        }),
    )
    .await;

    let updated = client
        .email_submission_change_status("ES-1", UndoStatus::Canceled)
        .await
        .unwrap()
        .expect("the updated submission must come back");
    assert_eq!(updated.id(), Some("ES-1"));
    assert_eq!(updated.undo_status(), Some(&UndoStatus::Canceled));

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/set", "method name must be on the wire");
    assert_eq!(
        call[1]["update"]["ES-1"],
        json!({ "undoStatus": "canceled" }),
        "an update patch carries only the member being changed"
    );
}

#[tokio::test]
async fn email_submission_change_status_reports_cannot_unsend() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7.5 "For update: cannotUnsend".
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "104",
            "newState": "104",
            "notUpdated": {
                "ES-1": {
                    "type": "cannotUnsend",
                    "description": "The message has already been relayed"
                }
            }
        }),
    )
    .await;

    let err = client
        .email_submission_change_status("ES-1", UndoStatus::Canceled)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::CannotUnsend);
    assert_eq!(
        set_err.description(),
        Some("The message has already been relayed")
    );
}

#[tokio::test]
async fn email_submission_destroy_removes_the_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "105",
            "newState": "106",
            "destroyed": ["ES-1"]
        }),
    )
    .await;

    client.email_submission_destroy("ES-1").await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/set", "method name must be on the wire");
    assert_eq!(call[1]["destroy"], json!(["ES-1"]));
    // §7.5: destroying the record must not touch the deliveries it represents,
    // so no onSuccess* argument may be invented by the client.
    assert!(call[1].get("onSuccessDestroyEmail").is_none());
    assert!(call[1].get("onSuccessUpdateEmail").is_none());
}

#[tokio::test]
async fn email_submission_destroy_reports_not_destroyed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // §7.5: a server "MAY forbid the client from manually destroying
    // EmailSubmission objects".
    mount_one(
        &server,
        "EmailSubmission/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "105",
            "newState": "105",
            "notDestroyed": {
                "ES-1": { "type": "forbidden", "description": "Server-managed records" }
            }
        }),
    )
    .await;

    let err = client.email_submission_destroy("ES-1").await.unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::Forbidden);
    assert_eq!(set_err.description(), Some("Server-managed records"));
}

#[tokio::test]
async fn email_submission_set_carries_the_on_success_email_arguments() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "EmailSubmission/set", submission_created_body()).await;

    // RFC 8621 §7.5.1: send the draft, and on success move it out of Drafts
    // into Sent and clear the `$draft` keyword.
    let mut request = client.build();
    {
        let set = request.set_email_submission();
        set.create()
            .email_id("M7f6ed5bcfd7e2604d1753f6c")
            .identity_id("I64588216");
        set.arguments()
            .on_success_update_email("c0")
            .mailbox_id("7cb4e8ee-df87-4757-b9c4-2ea1ca41b38e", false)
            .mailbox_id("73dbcb4b-bffc-48bd-8c2a-a2e91ca672f6", true)
            .keyword("$draft", false);
        set.arguments().on_success_destroy_email("c1");
    }
    let response = request.send().await.unwrap();
    assert!(
        !response.method_response_by_id("s0").unwrap().is_error(),
        "the create must not be an error"
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/set");
    assert_eq!(
        call[1]["onSuccessUpdateEmail"]["#c0"],
        json!({
            "mailboxIds/7cb4e8ee-df87-4757-b9c4-2ea1ca41b38e": false,
            "mailboxIds/73dbcb4b-bffc-48bd-8c2a-a2e91ca672f6": true,
            "keywords/$draft": false
        }),
        "creation references are prefixed with `#` (RFC 8620 §5.3 / RFC 8621 §7.5), \
         and the client spells a removal as `false` rather than the RFC's `null` \
         because the patch map is `String -> bool` (cf. Finding #39)"
    );
    assert_eq!(call[1]["onSuccessDestroyEmail"], json!(["#c1"]));
}

// ---------------------------------------------------------------------------
// 4. EmailSubmission/query (RFC 8621 §7.3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_query_sends_filter_and_sort_and_returns_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/query",
        query_body(&["ES-3bab7f9a", "ES-1c2d3e4f"]),
    )
    .await;

    let response = client
        .email_submission_query(
            Some(SubmissionFilter::undo_status(UndoStatus::Pending)),
            Some(vec![SubmissionComparator::sent_at().descending()]),
        )
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.query_state(), "104:0");
    assert_eq!(response.position(), 0);
    assert_eq!(response.ids(), ["ES-3bab7f9a", "ES-1c2d3e4f"]);
    assert_eq!(response.total(), Some(2));
    assert!(response.can_calculate_changes());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/query", "method name must be on the wire");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["filter"], json!({ "undoStatus": "pending" }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": false, "property": "sentAt" }]),
        "RFC 8621 §7.3 lists `sentAt` as a mandatory sort key"
    );
}

#[tokio::test]
async fn email_submission_query_with_and_operator_and_every_filter_variant() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "EmailSubmission/query", query_body(&["ES-1"])).await;

    // Every FilterCondition of RFC 8621 §7.3 in one AND group.
    let conditions: Vec<Filter<SubmissionFilter>> = vec![
        SubmissionFilter::identity_ids(["I64588216"]).into(),
        SubmissionFilter::email_ids(["M7f6ed5bcfd7e2604d1753f6c"]).into(),
        SubmissionFilter::thread_ids(["Tf14a5b2c9d3e0a7b6c5d4e3f2"]).into(),
        SubmissionFilter::undo_status(UndoStatus::Final).into(),
        SubmissionFilter::before(1_717_869_600).into(),
        SubmissionFilter::after(1_714_521_600).into(),
    ];
    let filter: Filter<SubmissionFilter> = Filter::and(conditions);
    let sort = vec![
        SubmissionComparator::email_id(),
        SubmissionComparator::thread_id().descending(),
    ];

    let response = client
        .email_submission_query(Some(filter), Some(sort))
        .await
        .unwrap();
    assert_eq!(response.ids(), ["ES-1"]);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/query");
    assert_eq!(
        call[1]["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "identityIds": ["I64588216"] },
                { "emailIds": ["M7f6ed5bcfd7e2604d1753f6c"] },
                { "threadIds": ["Tf14a5b2c9d3e0a7b6c5d4e3f2"] },
                { "undoStatus": "final" },
                { "before": "2024-06-08T18:00:00Z" },
                { "after": "2024-05-01T00:00:00Z" }
            ]
        }),
        "timestamps must serialise as RFC 3339 UTCDate, not as integers"
    );
    assert_eq!(
        call[1]["sort"],
        json!([
            { "isAscending": true, "property": "emailId" },
            { "isAscending": false, "property": "threadId" }
        ])
    );
}

#[tokio::test]
async fn email_submission_query_reports_unsupported_sort() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "unsupportedSort")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = client
        .email_submission_query(
            None::<SubmissionFilter>,
            Some(vec![SubmissionComparator::sent_at()]),
        )
        .await
        .unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}")
    };
    assert_eq!(method_err.error(), &MethodErrorType::UnsupportedSort);
    assert_eq!(method_err.to_string(), "Unsupported sort");
}

#[tokio::test]
async fn email_submission_query_with_no_arguments_sends_an_empty_filter_set() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "EmailSubmission/query", query_body(&[])).await;

    let response = client
        .email_submission_query(None::<SubmissionFilter>, None::<SubmissionSort>)
        .await
        .unwrap();
    assert!(response.ids().is_empty());

    // §7.3: "If zero properties are specified, it is automatically true for
    // all objects" — the client must not invent a filter.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/query");
    assert!(call[1].get("filter").is_none());
    assert!(call[1].get("sort").is_none());
}

// ---------------------------------------------------------------------------
// 5. EmailSubmission/queryChanges (RFC 8621 §7.4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_query_changes_round_trips_added_and_removed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/queryChanges",
        json!({
            "accountId": ACCOUNT_ID,
            "oldQueryState": "104:0",
            "newQueryState": "107:0",
            "total": 3,
            "removed": ["ES-removed"],
            "added": [
                { "id": "ES-new-1", "index": 0 },
                { "id": "ES-new-2", "index": 1 }
            ]
        }),
    )
    .await;

    let mut request = client.build();
    {
        let query = request.query_email_submission_changes("104:0");
        query.filter(SubmissionFilter::undo_status(UndoStatus::Pending));
        query.sort(vec![SubmissionComparator::sent_at().descending()]);
        query.up_to_id("ES-new-2");
        query.max_changes(10);
        query.calculate_total(true);
    }
    let response = request
        .send_query_email_submission_changes()
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_query_state(), "104:0");
    assert_eq!(response.new_query_state(), "107:0");
    assert_eq!(response.total(), Some(3));
    assert_eq!(response.removed(), ["ES-removed"]);
    assert_eq!(response.added().len(), 2);
    assert_eq!(response.added()[0].id(), "ES-new-1");
    assert_eq!(response.added()[0].index(), 0);
    assert_eq!(response.added()[1].id(), "ES-new-2");
    assert_eq!(response.added()[1].index(), 1);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "EmailSubmission/queryChanges",
        "method name must be on the wire"
    );
    assert_eq!(call[1]["sinceQueryState"], "104:0");
    assert_eq!(call[1]["upToId"], "ES-new-2");
    assert_eq!(call[1]["maxChanges"], 10);
    assert_eq!(call[1]["calculateTotal"], true);
    assert_eq!(call[1]["filter"], json!({ "undoStatus": "pending" }));
}

// ---------------------------------------------------------------------------
// 6. EmailSubmission/changes (RFC 8621 §7.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_changes_round_trips_every_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "EmailSubmission/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "104",
            "newState": "107",
            "hasMoreChanges": true,
            "created": ["ES-new"],
            "updated": ["ES-3bab7f9a"],
            "destroyed": ["ES-old"]
        }),
    )
    .await;

    let response = client.email_submission_changes("104", 100).await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "104");
    assert_eq!(response.new_state(), "107");
    assert!(response.has_more_changes());
    assert_eq!(response.created(), ["ES-new"]);
    assert_eq!(response.updated(), ["ES-3bab7f9a"]);
    assert_eq!(response.destroyed(), ["ES-old"]);
    assert_eq!(response.total_changes(), 3);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "EmailSubmission/changes",
        "method name must be on the wire"
    );
    assert_eq!(call[1]["sinceState"], "104");
    assert_eq!(call[1]["maxChanges"], 100);
}

#[tokio::test]
async fn email_submission_changes_reports_cannot_calculate_changes() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "cannotCalculateChanges")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = client.email_submission_changes("1", 10).await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}")
    };
    assert_eq!(method_err.error(), &MethodErrorType::CannotCalculateChanges);
}

// ---------------------------------------------------------------------------
// 7. Enum coverage — the RFC's closed value sets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_delivery_status_covers_every_rfc_value() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7: delivered ∈ {queued, yes, no, unknown};
    //              displayed ∈ {unknown, yes}.
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(
            vec![json!({
                "id": "ES-all",
                "identityId": "I64588216",
                "emailId": "M1",
                "deliveryStatus": {
                    "a@example.com": { "smtpReply": "250 2.0.0 queued", "delivered": "queued", "displayed": "unknown" },
                    "b@example.com": { "smtpReply": "250 2.1.5 Ok", "delivered": "yes", "displayed": "yes" },
                    "c@example.com": { "smtpReply": "550 5.1.1 no such user", "delivered": "no", "displayed": "unknown" },
                    "d@example.com": { "smtpReply": "250 2.0.0 relayed", "delivered": "unknown", "displayed": "yes" }
                }
            })],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request.get_email_submission().ids(["ES-all"]);
    let response = request.send_get_email_submission().await.unwrap();
    let submission = &response.list()[0];

    let statuses = submission.delivery_status().unwrap();
    assert_eq!(statuses.len(), 4);
    assert_eq!(
        statuses.get("a@example.com").unwrap().delivered(),
        &Delivered::Queued
    );
    assert_eq!(
        statuses.get("b@example.com").unwrap().delivered(),
        &Delivered::Yes
    );
    assert_eq!(
        statuses.get("c@example.com").unwrap().delivered(),
        &Delivered::No
    );
    assert_eq!(
        statuses.get("d@example.com").unwrap().delivered(),
        &Delivered::Unknown
    );
    assert_eq!(
        statuses.get("a@example.com").unwrap().displayed(),
        &Displayed::Unknown
    );
    assert_eq!(
        statuses.get("b@example.com").unwrap().displayed(),
        &Displayed::Yes
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/get");
}

#[tokio::test]
async fn email_submission_undo_status_covers_every_rfc_value() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §7: undoStatus ∈ {pending, final, canceled}.
    mount_one(
        &server,
        "EmailSubmission/get",
        submission_get_body(
            vec![
                json!({ "id": "ES-pending", "undoStatus": "pending" }),
                json!({ "id": "ES-final", "undoStatus": "final" }),
                json!({ "id": "ES-canceled", "undoStatus": "canceled" }),
            ],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request
        .get_email_submission()
        .ids(["ES-pending", "ES-final", "ES-canceled"]);
    let response = request.send_get_email_submission().await.unwrap();

    assert_eq!(response.list()[0].undo_status(), Some(&UndoStatus::Pending));
    assert_eq!(response.list()[1].undo_status(), Some(&UndoStatus::Final));
    assert_eq!(response.list()[2].undo_status(), Some(&UndoStatus::Canceled));
    // A record with no envelope at all reports None rather than panicking.
    assert!(response.list()[0].mail_from().is_none());
    assert!(response.list()[0].rcpt_to().is_none());
    assert!(response.list()[0].dsn_blob_ids().is_none());
    assert!(response.list()[0].mdn_blob_ids().is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "EmailSubmission/get");
}

#[test]
fn email_submission_property_display_matches_the_rfc_property_names() {
    // RFC 8621 §7 property names, as they must appear in a `properties`
    // argument and as `SetError.properties` entries.
    let expected = [
        (Property::Id, "id"),
        (Property::IdentityId, "identityId"),
        (Property::EmailId, "emailId"),
        (Property::ThreadId, "threadId"),
        (Property::Envelope, "envelope"),
        (Property::SendAt, "sendAt"),
        (Property::UndoStatus, "undoStatus"),
        (Property::DeliveryStatus, "deliveryStatus"),
        (Property::DsnBlobIds, "dsnBlobIds"),
        (Property::MdnBlobIds, "mdnBlobIds"),
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
// 8. Cross-method chain — query then get by result reference (§7.3 + RFC 8620 §3.6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_submission_query_then_get_by_result_reference() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A heterogeneous batch: `send_single` cannot read this (Finding #49), so
    // the test goes through `Request::send()` and unwraps per call id.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "EmailSubmission/query",
                    "s0",
                    query_body(&["ES-3bab7f9a", "ES-1c2d3e4f"]),
                ),
                inv(
                    "EmailSubmission/get",
                    "s1",
                    submission_get_body(vec![rfc_submission("ES-3bab7f9a")], vec![]),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    let ids_reference = {
        let query = request.query_email_submission();
        query.filter(SubmissionFilter::undo_status(UndoStatus::Pending));
        query.result_reference()
    };
    request
        .get_email_submission()
        .ids_ref(ids_reference)
        .properties([Property::Id, Property::EmailId, Property::UndoStatus]);

    let mut response = request.send().await.unwrap();
    assert_eq!(response.session_state(), SESSION_STATE);

    // `TaggedMethodResponse` is not `Clone` (Finding #25), so each response is
    // taken by position rather than by id.
    let query_response = response
        .method_response_by_pos(0)
        .unwrap_query_email_submission()
        .unwrap();
    assert_eq!(query_response.ids().len(), 2);

    let mut get_response = response
        .method_response_by_pos(0)
        .unwrap_get_email_submission()
        .unwrap();
    let list = get_response.take_list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id(), Some("ES-3bab7f9a"));
    assert_eq!(list[0].email_id(), Some("M7f6ed5bcfd7e2604d1753f6c"));

    // The reference must have been serialised in place of literal ids.
    let bodies = jmap_request_bodies(&server).await;
    let call = &bodies[0]["methodCalls"][1];
    assert_eq!(call[0], "EmailSubmission/get");
    assert!(call[1].get("ids").is_none(), "`#ids` replaces `ids`");
    assert_eq!(call[1]["#ids"]["resultOf"], "s0");
    assert_eq!(call[1]["#ids"]["name"], "EmailSubmission/query");
    assert_eq!(call[1]["#ids"]["path"], "/ids");
}
