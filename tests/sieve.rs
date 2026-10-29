//! P6 — sieve-domain integration tests: **SieveScript**.
//!
//! Drives the `Client`/`Request` Sieve helpers from `src/sieve/helpers.rs`
//! against a `wiremock` HTTP server using the shared fixtures in
//! `tests/common/mod.rs`. Canned payloads are the example bodies of
//! draft-ietf-jmap-sieve-12 (the document the crate was written against; the
//! capability has since been published as RFC 9661) wherever it has one, with
//! the section cited on the fixture.
//!
//! Coverage targets: `sieve/helpers.rs` (every `Client::sieve_script_*` helper
//! plus the `Request::{get,set,query,validate}_sieve_script` builders),
//! `sieve/get.rs` (record accessors), `sieve/set.rs` (the
//! `onSuccessActivateScript` `#`-prefix rule and the create/update patch keys),
//! `sieve/validate.rs` (the success-carries-an-error shape), `sieve/query.rs`
//! (filters and comparators) and `sieve/mod.rs` (`Property`).
//!
//! Every create/replace/validate helper starts with a blob upload, so these
//! tests mount *both* legs — `POST /upload/{ACCOUNT_ID}` and `POST /jmap` —
//! and assert each one separately.

mod common;

use common::{
    connected_client_json, jmap_response, mount_jmap, session_json_with_sieve,
    single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::query::{Comparator, Filter};
use jmap_client::core::set::SetErrorType;
use jmap_client::sieve::{query as sieve_query, Property};
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The sort list `Client::sieve_script_query` expects, so a test that supplies
/// no sort can name the type instead of leaving it ambiguous.
type SieveSort = Vec<Comparator<sieve_query::Comparator>>;

/// The property list `Client::sieve_script_get` expects, for the "no
/// properties" case.
type SieveProps = Vec<Property>;

/// draft-ietf-jmap-sieve-12 §2.1.1 — the script from the list-all example.
const SCRIPT_ID: &str = "2d647053-dded-418d-917a-63eda3ac8f7b";

/// draft-ietf-jmap-sieve-12 §2.2.1 — the blob id the upload example returns.
const UPLOADED_BLOB_ID: &str = "Gabcc83e44a6e19991c4568d0b94e1767c83dd123";

/// The `require "imapflags"` script of the draft's upload example.
const SCRIPT: &str = "require \"imapflags\";\n\nif address :is [\"To\", \"Cc\"] \"jmap@ietf.org\" {\n  setflag \"\\\\Flagged\";\n}\n";

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

/// A client whose Session advertises `urn:ietf:params:jmap:sieve`.
async fn sieve_client(server: &MockServer) -> jmap_client::client::Client {
    connected_client_json(server, session_json_with_sieve(server).await).await
}

/// Answers `POST /upload/{ACCOUNT_ID}` with the draft §2.2.1 upload response
/// (HTTP 201 Created, per RFC 8620 §6.1).
async fn mount_upload(server: &MockServer, blob_id: &str) {
    Mock::given(method("POST"))
        .and(path(format!("/upload/{ACCOUNT_ID}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "accountId": ACCOUNT_ID,
            "blobId": blob_id,
            "type": "application/octet-stream",
            "size": SCRIPT.len()
        })))
        .mount(server)
        .await
}

/// A `SieveScript/get` result envelope (RFC 8620 §5.1: `state`, `list` and
/// `notFound` are all required members).
fn sieve_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "1634915373.240633104-120",
        "list": list,
        "notFound": not_found
    })
}

/// draft-ietf-jmap-sieve-12 §2.1.1 — the `test1` record, verbatim.
fn test1_script() -> Value {
    json!({
        "id": SCRIPT_ID,
        "name": "test1",
        "isActive": true,
        "blobId": "S123"
    })
}

/// A `SieveScript/set` response that created `c0` as `object`.
fn created(body: Value) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "1603741717.50737918-4096",
        "newState": "1603741751.227268529-4096",
        "created": { "c0": body }
    })
}

/// A `SieveScript/set` response that updated [`SCRIPT_ID`] to `body`.
fn updated(body: Value) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "1603742603.309607868-4096",
        "newState": "1603742967.852315428-4096",
        "updated": { SCRIPT_ID: body }
    })
}

/// The method-call arguments of the only JMAP request made so far.
async fn only_call(server: &MockServer) -> Value {
    single_jmap_request(server).await["methodCalls"][0][1].clone()
}

/// The bodies of every `POST /upload/{ACCOUNT_ID}` the server recorded.
async fn uploads(server: &MockServer) -> Vec<Vec<u8>> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            r.method.as_str() == "POST" && r.url.path() == format!("/upload/{ACCOUNT_ID}")
        })
        .map(|r| r.body.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 1. SieveScript/get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sieve_script_get_returns_the_draft_example_record() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/get",
        sieve_get_body(vec![test1_script()], vec![]),
    )
    .await;

    let script = client
        .sieve_script_get(SCRIPT_ID, None::<SieveProps>)
        .await
        .unwrap()
        .expect("the example record must come back");

    assert_eq!(script.id(), Some(SCRIPT_ID));
    assert_eq!(script.name(), Some("test1"));
    assert_eq!(script.blob_id(), Some("S123"));
    assert!(script.is_active());

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "SieveScript/get");
    assert!(body["using"]
        .as_array()
        .unwrap()
        .contains(&json!("urn:ietf:params:jmap:sieve")));
}

#[tokio::test]
async fn sieve_script_get_sends_only_the_requested_properties() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(&server, "SieveScript/get", sieve_get_body(vec![], vec!["nope"])).await;

    let missing = client
        .sieve_script_get("nope", Some([Property::Id, Property::IsActive]))
        .await
        .unwrap();

    assert!(missing.is_none());
    let args = only_call(&server).await;
    assert_eq!(args["properties"], json!(["id", "isActive"]));
    assert_eq!(args["ids"], json!(["nope"]));
}

#[tokio::test]
async fn an_absent_is_active_reads_as_inactive() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/get",
        sieve_get_body(
            vec![json!({ "id": SCRIPT_ID, "name": "idle", "blobId": "S9" })],
            vec![],
        ),
    )
    .await;

    let script = client
        .sieve_script_get(SCRIPT_ID, None::<SieveProps>)
        .await
        .unwrap()
        .unwrap();

    // draft §2: `isActive` defaults to false, and `SieveScript::is_active()`
    // maps a missing member onto that default rather than surfacing None.
    assert!(!script.is_active());
    assert_eq!(script.name(), Some("idle"));
}

// ---------------------------------------------------------------------------
// 2. SieveScript/set — create (upload leg + method leg)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sieve_script_create_uploads_then_creates_and_activates() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, UPLOADED_BLOB_ID).await;
    // draft-ietf-jmap-sieve-12 §2.2.1 — create + activate, with the created
    // record echoed back carrying `isActive: true`.
    mount_one(
        &server,
        "SieveScript/set",
        created(json!({
            "id": "dd1b164f-8cdc-448c-9f54",
            "name": "ken-20201210T171432-0",
            "blobId": UPLOADED_BLOB_ID,
            "isActive": true
        })),
    )
    .await;

    let script = client
        .sieve_script_create("myscript", SCRIPT.as_bytes().to_vec(), true)
        .await
        .unwrap();

    assert_eq!(script.id(), Some("dd1b164f-8cdc-448c-9f54"));
    assert_eq!(script.blob_id(), Some(UPLOADED_BLOB_ID));
    assert!(script.is_active());

    // The upload leg carried the raw script octets.
    assert_eq!(uploads(&server).await, vec![SCRIPT.as_bytes()]);

    // ...and the set leg referenced the *creation id* with the `#` prefix the
    // draft requires for creation-references (§2.2).
    let args = only_call(&server).await;
    assert_eq!(
        args["create"]["c0"],
        json!({ "name": "myscript", "blobId": UPLOADED_BLOB_ID })
    );
    assert_eq!(args["onSuccessActivateScript"], "#c0");
    assert_eq!(args["accountId"], ACCOUNT_ID);
}

#[tokio::test]
async fn sieve_script_create_without_activate_omits_the_argument() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, UPLOADED_BLOB_ID).await;
    mount_one(
        &server,
        "SieveScript/set",
        created(json!({
            "id": "dd1b164f-8cdc-448c-9f54",
            "name": "myscript",
            "blobId": UPLOADED_BLOB_ID,
            "isActive": false
        })),
    )
    .await;

    let script = client
        .sieve_script_create("myscript", SCRIPT.as_bytes().to_vec(), false)
        .await
        .unwrap();

    assert!(!script.is_active());
    let args = only_call(&server).await;
    assert!(
        args.get("onSuccessActivateScript").is_none(),
        "no activation requested means the argument must be absent: {args}"
    );
    assert!(args.get("onSuccessDeactivateScript").is_none(), "{args}");
}

#[tokio::test]
async fn sieve_script_create_fails_when_the_upload_is_rejected() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    // RFC 8620 §6.1: an upload error is an RFC 7807 problem document, and the
    // content type is byte-compared (Findings #7) — no charset suffix.
    Mock::given(method("POST"))
        .and(path(format!("/upload/{ACCOUNT_ID}")))
        .respond_with(ResponseTemplate::new(413).set_body_raw(
            json!({
                "type": "https://www.rfc-editor.org/rfc/rfc8620#section-6.1",
                "title": "TOO_LARGE",
                "detail": "Maximum upload size is 50000000 bytes"
            })
            .to_string(),
            "application/problem+json",
        ))
        .mount(&server)
        .await;

    let err = client
        .sieve_script_create("myscript", SCRIPT.as_bytes().to_vec(), true)
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::Problem(_)),
        "an upload problem document must surface as Error::Problem, got {err:?}"
    );
    // The method leg must never have been attempted.
    assert!(
        common::jmap_request_bodies(&server)
            .await
            .iter()
            .all(|b| !b.to_string().contains("SieveScript/set")),
        "a failed upload must short-circuit the set call"
    );
}

#[tokio::test]
async fn sieve_script_create_reports_already_exists() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, UPLOADED_BLOB_ID).await;
    // draft §2.2: a name clash is an `alreadyExists` SetError carrying the
    // `existingId` of the script that already owns the name.
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notCreated": {
                "c0": {
                    "type": "alreadyExists",
                    "description": "A script with that name already exists",
                    "properties": ["name"]
                }
            }
        }),
    )
    .await;

    let err = client
        .sieve_script_create("test1", SCRIPT.as_bytes().to_vec(), false)
        .await
        .unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::AlreadyExists);
    assert_eq!(err.properties().unwrap(), ["name"]);
}

// ---------------------------------------------------------------------------
// 3. SieveScript/set — replace / rename / activate / deactivate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sieve_script_replace_uploads_a_new_blob_and_reactivates_by_real_id() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, "G969c83e44a6e10871c4568d0b94e1767c83ddeae").await;
    mount_one(
        &server,
        "SieveScript/set",
        updated(json!({ "isActive": true })),
    )
    .await;

    let script = client
        .sieve_script_replace(SCRIPT_ID, b"redirect \"me@example.org\"\r\n;".to_vec(), true)
        .await
        .unwrap()
        .expect("the updated record must come back");

    assert_eq!(script.is_active(), true);

    let args = only_call(&server).await;
    assert_eq!(
        args["update"][SCRIPT_ID],
        json!({ "blobId": "G969c83e44a6e10871c4568d0b94e1767c83ddeae" })
    );
    // An existing script is activated by its *real* id, with no `#` prefix —
    // that is why the crate has two setters (`on_success_activate_script`
    // prefixes, `on_success_activate_script_id` does not).
    assert_eq!(args["onSuccessActivateScript"], SCRIPT_ID);
}

#[tokio::test]
async fn sieve_script_replace_without_activate_leaves_the_active_script_alone() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, "G969c83e44a6e10871c4568d0b94e1767c83ddeae").await;
    mount_one(&server, "SieveScript/set", updated(Value::Null)).await;

    let script = client
        .sieve_script_replace(SCRIPT_ID, b"# noop".to_vec(), false)
        .await
        .unwrap();

    assert!(
        script.is_none(),
        "an `updated: {{ id: null }}` response is Ok(None)"
    );
    let args = only_call(&server).await;
    assert!(args.get("onSuccessActivateScript").is_none(), "{args}");
}

#[tokio::test]
async fn sieve_script_rename_patches_only_the_name() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/set",
        updated(json!({ "isActive": false })),
    )
    .await;

    let script = client
        .sieve_script_rename(SCRIPT_ID, "myscript", false)
        .await
        .unwrap()
        .unwrap();

    assert!(!script.is_active());
    assert_eq!(
        only_call(&server).await["update"][SCRIPT_ID],
        json!({ "name": "myscript" })
    );
}

#[tokio::test]
async fn sieve_script_activate_sends_only_the_activation_argument() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    // draft §2.2.1 — the activate-only set has no create/update/destroy at all.
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1603742967.852315428-4096",
            "newState": "1603744460.316617118-4096",
            "updated": { SCRIPT_ID: { "isActive": true } }
        }),
    )
    .await;

    client.sieve_script_activate(SCRIPT_ID).await.unwrap();

    let args = only_call(&server).await;
    assert_eq!(args["onSuccessActivateScript"], SCRIPT_ID);
    assert!(args.get("create").is_none(), "{args}");
    assert!(args.get("update").is_none(), "{args}");
    assert!(args.get("destroy").is_none(), "{args}");
}

#[tokio::test]
async fn sieve_script_activate_reports_a_rejected_activation() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notUpdated": {
                SCRIPT_ID: {
                    "type": "invalidScript",
                    "description": "line 3: expected `;`"
                }
            }
        }),
    )
    .await;

    let err = client.sieve_script_activate(SCRIPT_ID).await.unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::InvalidScript);
    assert_eq!(err.description(), Some("line 3: expected `;`"));
}

#[tokio::test]
async fn sieve_script_deactivate_sends_the_deactivation_argument() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1603744460.316617118-4096",
            "newState": "1603744637.575375572-4096",
            "updated": { SCRIPT_ID: { "isActive": false } }
        }),
    )
    .await;

    client.sieve_script_deactivate().await.unwrap();

    let args = only_call(&server).await;
    // NOTE: draft §2.2 spells "deactivate" as `onSuccessActivateScript: null`;
    // the crate sends its own `onSuccessDeactivateScript: true` member instead
    // (recorded as a Finding).
    assert_eq!(args["onSuccessDeactivateScript"], true);
    assert!(args.get("onSuccessActivateScript").is_none(), "{args}");
}

// ---------------------------------------------------------------------------
// 4. SieveScript/set — destroy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sieve_script_destroy_sends_the_destroy_list() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    // draft §2.2.1 — the destroy that follows a separate deactivation.
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1603744637.575375572-4096",
            "newState": "1603744637.854390875-4096",
            "destroyed": [SCRIPT_ID]
        }),
    )
    .await;

    client.sieve_script_destroy(SCRIPT_ID).await.unwrap();

    let args = only_call(&server).await;
    assert_eq!(args["destroy"], json!([SCRIPT_ID]));
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "SieveScript/set"
    );
}

#[tokio::test]
async fn destroying_the_active_script_is_rejected_with_script_is_active() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    // draft §2.2 / §5.2.2 — "The client tried to destroy the active
    // SieveScript."
    mount_one(
        &server,
        "SieveScript/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notDestroyed": {
                SCRIPT_ID: {
                    "type": "scriptIsActive",
                    "description": "Deactivate the script before destroying it"
                }
            }
        }),
    )
    .await;

    let err = client.sieve_script_destroy(SCRIPT_ID).await.unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::ScriptIsActive);
    assert!(err.to_string().contains("Deactivate the script"));
}

// ---------------------------------------------------------------------------
// 5. SieveScript/validate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_valid_script_comes_back_without_an_error() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, UPLOADED_BLOB_ID).await;
    // draft §2.4 — validity is reported by `error: null` on a *successful*
    // response, not by an HTTP status.
    mount_one(
        &server,
        "SieveScript/validate",
        json!({ "accountId": ACCOUNT_ID, "error": null }),
    )
    .await;

    client
        .sieve_script_validate(SCRIPT.as_bytes().to_vec())
        .await
        .unwrap();

    let args = only_call(&server).await;
    assert_eq!(args["blobId"], UPLOADED_BLOB_ID);
    assert_eq!(args["accountId"], ACCOUNT_ID);
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "SieveScript/validate"
    );
}

#[tokio::test]
async fn an_invalid_script_is_reported_on_a_successful_response() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_upload(&server, UPLOADED_BLOB_ID).await;
    mount_one(
        &server,
        "SieveScript/validate",
        json!({
            "accountId": ACCOUNT_ID,
            "error": {
                "type": "invalidScript",
                "description": "line 3: syntax error near `addresss`"
            }
        }),
    )
    .await;

    let err = client
        .sieve_script_validate(b"require \"imapflags\";\naddresss {}".to_vec())
        .await
        .unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::InvalidScript);
    assert_eq!(
        err.description(),
        Some("line 3: syntax error near `addresss`")
    );
}

// ---------------------------------------------------------------------------
// 6. SieveScript/query
// ---------------------------------------------------------------------------

/// A `SieveScript/query` result envelope (RFC 8620 §5.5).
fn query_body(ids: Vec<&str>, total: usize) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "queryState": "1634915373.240633104-120",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ids,
        "total": total
    })
}

#[tokio::test]
async fn sieve_script_query_sends_filter_and_sort() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/query",
        query_body(vec![SCRIPT_ID], 1),
    )
    .await;

    let filter: Filter<sieve_query::Filter> = Filter::and([
        Filter::from(sieve_query::Filter::name("test")),
        Filter::from(sieve_query::Filter::is_active(true)),
    ]);

    let response = client
        .sieve_script_query(Some(filter), Some([sieve_query::Comparator::name()]))
        .await
        .unwrap();

    assert_eq!(response.ids(), [SCRIPT_ID]);
    assert_eq!(response.total(), Some(1));

    let args = only_call(&server).await;
    assert_eq!(
        args["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "name": "test" },
                { "isActive": true }
            ]
        })
    );
    assert_eq!(
        args["sort"],
        json!([{ "property": "name", "isAscending": true }])
    );
}

#[tokio::test]
async fn sieve_script_query_with_no_criteria_omits_filter_and_sort() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(&server, "SieveScript/query", query_body(vec![], 0)).await;

    client
        .sieve_script_query(None::<Filter<sieve_query::Filter>>, None::<SieveSort>)
        .await
        .unwrap();

    let args = only_call(&server).await;
    assert!(args.get("filter").is_none(), "{args}");
    assert!(args.get("sort").is_none(), "{args}");
}

// ---------------------------------------------------------------------------
// 7. Method-level errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_method_level_error_short_circuits_the_helper() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unknownMethod")], SESSION_STATE),
    )
    .await;

    let err = client
        .sieve_script_get(SCRIPT_ID, None::<SieveProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "an `error` Invocation must surface as Error::Method, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 8. Wire names
// ---------------------------------------------------------------------------

#[test]
fn sieve_property_wire_names() {
    for (property, wire) in [
        (Property::Id, "id"),
        (Property::Name, "name"),
        (Property::BlobId, "blobId"),
        (Property::IsActive, "isActive"),
    ] {
        assert_eq!(serde_json::to_value(&property).unwrap(), json!(wire));
        assert_eq!(property.to_string(), wire);
        assert_eq!(
            serde_json::from_value::<Property>(json!(wire)).unwrap(),
            property
        );
    }
}

#[test]
fn the_sieve_script_record_has_no_order_member() {
    // ManageSieve has no ordering either; the crate's `SieveScript` is exactly
    // the four draft §2 properties, so nothing else may leak in.
    let script: jmap_client::sieve::SieveScript<jmap_client::Get> =
        serde_json::from_value(test1_script()).unwrap();
    let mut keys: Vec<String> = serde_json::to_value(&script)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(keys, ["blobId", "id", "isActive", "name"]);
}

// ---------------------------------------------------------------------------
// 9. `rename` with `activate = true`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sieve_script_rename_with_activate_renames_and_activates_in_one_call() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/set",
        updated(json!({ "name": "renamed", "isActive": true })),
    )
    .await;

    let script = client
        .sieve_script_rename(SCRIPT_ID, "renamed", true)
        .await
        .unwrap()
        .expect("the updated record must come back");

    assert_eq!(script.name(), Some("renamed"));
    assert!(script.is_active());

    // The patch carries the new name *and* the activation argument keyed by the
    // real script id — the same `on_success_activate_script_id` form `activate`
    // uses, so the rename and the activation are atomic (draft §2.2).
    let args = only_call(&server).await;
    assert_eq!(args["update"][SCRIPT_ID], json!({ "name": "renamed" }));
    assert_eq!(args["onSuccessActivateScript"], SCRIPT_ID);
    assert!(
        args.get("onSuccessDeactivateScript").is_none(),
        "an activating rename must not also ask to deactivate: {args}"
    );
}

// ---------------------------------------------------------------------------
// 10. The `Request::send_*` convenience wrappers
//
// Each `Request::send_<method>` is a thin `self.send_single().await` wrapper
// (src/sieve/helpers.rs). Nothing in the `Client::sieve_script_*` family calls
// them — the client helpers go straight to `send_single` — so without a test
// that reaches the wrapper by name it stays cold. Each test below builds the
// call through the typed builder, sends it through the wrapper, asserts the
// parsed result *and* spies on the wire so it cannot pass on an empty
// `methodCalls` array (Finding #31).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_get_sieve_script_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/get",
        sieve_get_body(vec![test1_script()], vec![]),
    )
    .await;

    let mut request = client.build();
    request.get_sieve_script().ids([SCRIPT_ID]);
    let mut response = request.send_get_sieve_script().await.unwrap();

    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    assert_eq!(response.not_found(), [] as [&str; 0]);
    let script = response.take_list().pop().expect("one record");
    assert_eq!(script.id(), Some(SCRIPT_ID));
    assert_eq!(script.name(), Some("test1"));

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "SieveScript/get",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    assert_eq!(body["methodCalls"][0][1]["ids"], json!([SCRIPT_ID]));
}

#[tokio::test]
async fn send_set_sieve_script_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/set",
        updated(json!({ "name": "renamed" })),
    )
    .await;

    let mut request = client.build();
    request.set_sieve_script().update(SCRIPT_ID).name("renamed");
    let mut response = request.send_set_sieve_script().await.unwrap();

    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    assert_eq!(response.new_state(), "1603742967.852315428-4096");
    let script = response.updated(SCRIPT_ID).unwrap().expect("updated record");
    assert_eq!(script.name(), Some("renamed"));

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "SieveScript/set");
    assert_eq!(
        body["methodCalls"][0][1]["update"][SCRIPT_ID],
        json!({ "name": "renamed" })
    );
}

#[tokio::test]
async fn send_validate_sieve_script_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    // draft §2.4 / Finding #4: a clean script answers with `error: null` on a
    // *successful* response.
    mount_one(
        &server,
        "SieveScript/validate",
        json!({ "accountId": ACCOUNT_ID, "error": null }),
    )
    .await;

    let mut request = client.build();
    request.validate_sieve_script(UPLOADED_BLOB_ID);
    request.send_validate_sieve_script().await.unwrap();

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "SieveScript/validate");
    assert_eq!(body["methodCalls"][0][1]["blobId"], UPLOADED_BLOB_ID);
    assert_eq!(body["methodCalls"][0][1]["accountId"], ACCOUNT_ID);
}

#[tokio::test]
async fn send_validate_sieve_script_surfaces_an_invalid_script_as_a_set_error() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/validate",
        json!({
            "accountId": ACCOUNT_ID,
            "error": {
                "type": "invalidScript",
                "description": "line 1: unknown command"
            }
        }),
    )
    .await;

    let mut request = client.build();
    request.validate_sieve_script(UPLOADED_BLOB_ID);
    // draft-ietf-jmap-sieve-12 §2.5 — an invalid script is reported in the
    // response body's `error` member, not as a method-level Invocation. The
    // wrapper therefore returns `Ok`, and the caller has to call
    // `unwrap_error()` to turn the body-level SetError into an `Err`.
    let response = request
        .send_validate_sieve_script()
        .await
        .expect("validate answers with a body-level error, not a method error");
    let err = response.unwrap_error().unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::InvalidScript);
    assert_eq!(err.description(), Some("line 1: unknown command"));
}

#[tokio::test]
async fn send_query_sieve_script_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_one(
        &server,
        "SieveScript/query",
        query_body(vec![SCRIPT_ID], 1),
    )
    .await;

    let mut request = client.build();
    request
        .query_sieve_script()
        .filter(sieve_query::Filter::is_active(true))
        .sort([sieve_query::Comparator::name()]);
    let response = request.send_query_sieve_script().await.unwrap();

    assert_eq!(response.ids(), [SCRIPT_ID]);
    assert_eq!(response.total(), Some(1));

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "SieveScript/query");
    assert_eq!(body["methodCalls"][0][1]["filter"], json!({ "isActive": true }));
    assert_eq!(
        body["methodCalls"][0][1]["sort"],
        json!([{ "property": "name", "isAscending": true }])
    );
}

#[tokio::test]
async fn send_get_sieve_script_surfaces_a_method_error() {
    let server = MockServer::start().await;
    let client = sieve_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unknownMethod")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request.get_sieve_script().ids([SCRIPT_ID]);
    let err = request.send_get_sieve_script().await.unwrap_err();

    assert!(
        matches!(err, Error::Method(_)),
        "the wrapper must propagate an `error` Invocation as Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "SieveScript/get"
    );
}
