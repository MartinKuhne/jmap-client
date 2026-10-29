//! P5 — identity-domain integration tests: **Identity**.
//!
//! Drives the `Client::identity_*` helpers from `src/identity/helpers.rs` and
//! the matching `Request::` builders against a `wiremock` HTTP server, using
//! the shared fixture layer in `tests/common/mod.rs`. Canned payloads are the
//! RFC 8621 §6.4 example records verbatim (the RFC's line-wrapped
//! `htmlSignature` is joined onto one line, since a literal newline is not
//! legal inside a JSON string).
//!
//! Coverage targets: `identity/helpers.rs` (every `Client::` helper plus the
//! `Request::{get,set,changes}_identity` builders), `identity/get.rs`
//! (record accessors), `identity/set.rs` (patch keys, `replyTo`/`bcc` list
//! handling) and `identity/mod.rs` (`Property`).
//!
//! Deviations from the P5 brief, recorded in the `COVERAGE_PLAN.md` findings:
//!
//! * There is **no** `Client::identity_update`, **no** `Client::identity_query`
//!   and **no** "most likely identity" helper. Update goes through
//!   `Request::set_identity().update(id)`; the RFC §6 selection rule for the
//!   `*@domain` wildcard is implemented client-side in
//!   [`identity_selection_prefers_an_exact_address_over_a_wildcard_identity`].
//! * The `Identity` object has no `sortOrder`, `draftId` or `queueId` members;
//!   RFC 8621 §6 defines exactly
//!   `id/name/email/replyTo/bcc/textSignature/htmlSignature/mayDelete`.
//! * `Request::{get,set,changes}_identity` add **no** capability of their own.
//!   RFC 8621 §1.3.2 places the Identity data type under
//!   `urn:ietf:params:jmap:submission` (there is no §1.3.4). The client does not
//!   opt in per method: `Request::new` builds `using` from every capability the
//!   *session* advertises, so an Identity call carries `submission` whenever the
//!   server offers it and never advertises a capability the server withheld. See
//!   [`identity_using_is_filtered_to_the_session_capabilities`].

mod common;

use common::{
    connected_client, connected_client_json, jmap_response, jmap_response_with_created_ids,
    mount_jmap, session_json, single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::error::MethodErrorType;
use jmap_client::core::set::SetErrorType;
use jmap_client::identity::{Identity, Property};
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

/// The property list `Client::identity_get` expects, so the "no properties"
/// case can name the type instead of leaving it ambiguous.
type IdentityProps = Vec<Property>;

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

/// A `Identity/get` result envelope (RFC 8620 §5.1: `state`, `list` and
/// `notFound` are all required members — Finding #8).
fn identity_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "99401312ae-11-333",
        "list": list,
        "notFound": not_found
    })
}

/// RFC 8621 §6.4 — the first `Identity/get` example record, verbatim (the
/// two-line `htmlSignature` of the printed RFC joined into one JSON string).
fn rfc_identity_joe() -> Value {
    json!({
        "id": "XD-3301-222-11_22AAz",
        "name": "Joe Bloggs",
        "email": "joe@example.com",
        "replyTo": null,
        "bcc": [{
            "name": null,
            "email": "joe+archive@example.com"
        }],
        "textSignature": "-- \nJoe Bloggs\nMaster of Email",
        "htmlSignature": "<div><b>Joe Bloggs</b></div><div>Master of Email</div>",
        "mayDelete": false
    })
}

/// RFC 8621 §6.4 — the second example record: the `*@example.com` wildcard
/// identity with empty signatures and `mayDelete: true`.
fn rfc_identity_wildcard() -> Value {
    json!({
        "id": "XD-9911312-11_22AAz",
        "name": "Joe B",
        "email": "*@example.com",
        "replyTo": null,
        "bcc": null,
        "textSignature": "",
        "htmlSignature": "",
        "mayDelete": true
    })
}

/// A `Identity/set` response that created one Identity under creation id `c0`
/// (RFC 8620 §5.3: `created` maps creation id → the created object, which
/// MUST carry the server-assigned `id`).
fn identity_created_body() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "99401312ae-11-333",
        "newState": "99401312ae-11-334",
        "created": {
            "c0": {
                "id": "ID-4f2a91c0",
                "name": "Joe Bloggs",
                "email": "joe@example.com",
                "mayDelete": true
            }
        }
    })
}

// ---------------------------------------------------------------------------
// 1. Identity/get (RFC 8621 §6.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_get_parses_the_first_rfc_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(vec![rfc_identity_joe()], vec![]),
    )
    .await;

    let identity = client
        .identity_get("XD-3301-222-11_22AAz", None::<IdentityProps>)
        .await
        .unwrap()
        .expect("the Identity record must come back");

    assert_eq!(identity.id(), Some("XD-3301-222-11_22AAz"));
    assert_eq!(identity.name(), Some("Joe Bloggs"));
    assert_eq!(identity.email(), Some("joe@example.com"));

    // `replyTo: null` is an absent list, not an empty one (§6: default null).
    assert!(identity.reply_to().is_none());

    // `bcc` carries one address with a null display name.
    let bcc = identity.bcc().expect("bcc must be present");
    assert_eq!(bcc.len(), 1);
    assert_eq!(bcc[0].email(), "joe+archive@example.com");
    assert_eq!(bcc[0].name(), None);

    assert_eq!(
        identity.text_signature(),
        Some("-- \nJoe Bloggs\nMaster of Email")
    );
    assert_eq!(
        identity.html_signature(),
        Some("<div><b>Joe Bloggs</b></div><div>Master of Email</div>")
    );

    // §6: "Attempts to destroy an Identity with mayDelete: false will be
    // rejected with a standard forbidden SetError."
    assert!(!identity.may_delete());

    // --- what actually went over the wire (RFC 8620 §3.2/§5.1) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/get", "method name must be on the wire");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["ids"], json!(["XD-3301-222-11_22AAz"]));
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn identity_get_parses_the_wildcard_record_with_empty_signatures() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(vec![rfc_identity_wildcard()], vec![]),
    )
    .await;

    let identity = client
        .identity_get("XD-9911312-11_22AAz", None::<IdentityProps>)
        .await
        .unwrap()
        .expect("the wildcard Identity must come back");

    assert_eq!(identity.id(), Some("XD-9911312-11_22AAz"));
    assert_eq!(identity.name(), Some("Joe B"));
    // §6: "If the mailbox part of the address is the single character `*` …
    // the client may use any valid address ending in that domain."
    assert_eq!(identity.email(), Some("*@example.com"));
    assert!(identity.bcc().is_none());
    assert!(identity.reply_to().is_none());
    // Empty strings are *present* values, distinct from null.
    assert_eq!(identity.text_signature(), Some(""));
    assert_eq!(identity.html_signature(), Some(""));
    assert!(identity.may_delete());
}

#[tokio::test]
async fn identity_get_returns_none_when_the_id_is_not_found() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(vec![], vec!["ID-gone"]),
    )
    .await;

    assert!(client
        .identity_get("ID-gone", None::<IdentityProps>)
        .await
        .unwrap()
        .is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/get");
}

#[tokio::test]
async fn identity_get_pops_the_last_record_when_the_server_returns_several() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // A spec-following server answers every id it was given.
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(
            vec![rfc_identity_joe(), rfc_identity_wildcard()],
            vec![],
        ),
    )
    .await;

    // `identity_get` is `take_list().pop()`, so the *last* record wins
    // regardless of the id asked for (same trap as `Client::thread_get`,
    // Finding #48). Multi-id reads must index the list instead.
    let popped = client
        .identity_get("XD-3301-222-11_22AAz", None::<IdentityProps>)
        .await
        .unwrap()
        .expect("a record must come back");
    assert_eq!(popped.id(), Some("XD-9911312-11_22AAz"));

    let mut request = client.build();
    request
        .get_identity()
        .ids(["XD-3301-222-11_22AAz", "XD-9911312-11_22AAz"]);
    let response = request.send_get_identity().await.unwrap();
    assert_eq!(response.list().len(), 2);
    assert_eq!(response.list()[0].email(), Some("joe@example.com"));
    assert_eq!(response.list()[1].email(), Some("*@example.com"));
    assert_eq!(response.state(), "99401312ae-11-333");
    assert_eq!(response.not_found(), &[] as &[String]);
}

#[tokio::test]
async fn identity_get_sends_only_the_requested_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(
            vec![json!({ "id": "ID-1", "email": "joe@example.com", "mayDelete": true })],
            vec![],
        ),
    )
    .await;

    let identity = client
        .identity_get(
            "ID-1",
            Some(vec![
                Property::Id,
                Property::Email,
                Property::MayDelete,
            ]),
        )
        .await
        .unwrap()
        .expect("the record must come back");

    assert_eq!(identity.id(), Some("ID-1"));
    assert_eq!(identity.email(), Some("joe@example.com"));
    assert!(identity.may_delete());
    // Not requested, so absent.
    assert_eq!(identity.name(), None);
    assert_eq!(identity.text_signature(), None);
    assert_eq!(identity.html_signature(), None);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/get");
    assert_eq!(
        call[1]["properties"],
        json!(["id", "email", "mayDelete"]),
        "the RFC property names must go on the wire verbatim"
    );
}

#[tokio::test]
async fn identity_get_missing_a_required_member_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1 makes `list` mandatory; `GetResponse` declares it
    // non-Option, so its absence fails deserialisation (Finding #8).
    mount_one(
        &server,
        "Identity/get",
        json!({ "accountId": ACCOUNT_ID, "state": "1", "notFound": [] }),
    )
    .await;

    let err = client
        .identity_get("ID-1", None::<IdentityProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Identity/set — create (RFC 8621 §6.3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_create_round_trips_the_creation_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv("Identity/set", "s0", identity_created_body())],
            json!({ "c0": "ID-4f2a91c0" }),
            SESSION_STATE,
        ),
    )
    .await;

    let created = client
        .identity_create("Joe Bloggs", "joe@example.com")
        .await
        .unwrap();

    assert_eq!(created.id(), Some("ID-4f2a91c0"));
    assert_eq!(created.name(), Some("Joe Bloggs"));
    assert_eq!(created.email(), Some("joe@example.com"));
    assert!(created.may_delete());

    // --- the request the server actually received ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set", "method name must be on the wire");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        call[1]["create"]["c0"],
        json!({ "name": "Joe Bloggs", "email": "joe@example.com" }),
        "`Identity::new()` seeds replyTo/bcc with empty lists, and \
         `list_not_set` suppresses them (cf. Finding #39), so only name and \
         email may be on the wire"
    );
    assert!(call[1].get("update").is_none());
    assert!(call[1].get("destroy").is_none());
}

#[tokio::test]
async fn identity_create_sends_reply_to_bcc_and_both_signatures() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Identity/set", identity_created_body()).await;

    let mut request = client.build();
    request
        .set_identity()
        .create()
        .name("Joe Bloggs")
        .email("joe@example.com")
        .reply_to(Some(
            ["reply@example.com", "joe+alt@example.com"].into_iter(),
        ))
        .bcc(Some(["joe+archive@example.com"].into_iter()))
        .text_signature("-- \nJoe Bloggs")
        .html_signature("<div>Joe Bloggs</div>");
    let response = request.send_set_identity().await.unwrap();
    assert!(response.has_created());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set");
    assert_eq!(
        call[1]["create"]["c0"],
        json!({
            "name": "Joe Bloggs",
            "email": "joe@example.com",
            "replyTo": [
                { "name": null, "email": "reply@example.com" },
                { "name": null, "email": "joe+alt@example.com" }
            ],
            "bcc": [
                { "name": null, "email": "joe+archive@example.com" }
            ],
            "textSignature": "-- \nJoe Bloggs",
            "htmlSignature": "<div>Joe Bloggs</div>"
        }),
        "replyTo/bcc serialise as EmailAddress objects per RFC 8621 §4.1.2.3"
    );
}

#[tokio::test]
async fn identity_create_reports_forbidden_from() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §6.3: "For create: forbiddenFrom — The user is not allowed to
    // send from the address given as the email property of the Identity."
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "99401312ae-11-333",
            "newState": "99401312ae-11-333",
            "notCreated": {
                "c0": {
                    "type": "forbiddenFrom",
                    "description": "Not permitted to send from that address"
                }
            }
        }),
    )
    .await;

    let err = client
        .identity_create("Mallory", "mallory@elsewhere.test")
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::ForbiddenFrom);
    assert_eq!(
        set_err.description(),
        Some("Not permitted to send from that address")
    );
}

#[tokio::test]
async fn identity_create_reports_invalid_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notCreated": {
                "c0": { "type": "invalidProperties", "properties": ["email"] }
            }
        }),
    )
    .await;

    let err = client.identity_create("Nope", "not-an-address").await.unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::InvalidProperties);
    assert_eq!(set_err.properties(), Some(&["email".to_string()][..]));
}

// ---------------------------------------------------------------------------
// 3. Identity/set — update / destroy (RFC 8621 §6.3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_update_patches_the_html_signature() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.6 spells an updated entry `null`; the client models that as
    // `Option<Identity<Get>>` and surfaces `Ok(None)`.
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "99401312ae-11-333",
            "newState": "99401312ae-11-334",
            "updated": { "XD-3301-222-11_22AAz": null }
        }),
    )
    .await;

    let mut request = client.build();
    request
        .set_identity()
        .update("XD-3301-222-11_22AAz")
        .html_signature("<div>Master of Email</div>");
    let mut response = request.send_set_identity().await.unwrap();

    assert!(response.has_updated());
    assert!(
        response
            .updated("XD-3301-222-11_22AAz")
            .unwrap()
            .is_none(),
        "a null update entry means \"updated, no echo\""
    );
    assert!(response.unwrap_update_errors().is_ok());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set", "method name must be on the wire");
    assert_eq!(
        call[1]["update"]["XD-3301-222-11_22AAz"],
        json!({ "htmlSignature": "<div>Master of Email</div>" }),
        "an update patch carries only the member being changed"
    );
}

#[tokio::test]
async fn identity_update_returns_the_updated_object() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "2",
            "updated": {
                "ID-1": {
                    "id": "ID-1",
                    "name": "Joe B",
                    "email": "joe@example.com",
                    "replyTo": [{ "name": "Joe Bloggs", "email": "reply@example.com" }],
                    "mayDelete": false
                }
            }
        }),
    )
    .await;

    let mut request = client.build();
    request
        .set_identity()
        .update("ID-1")
        .name("Joe B")
        .reply_to(Some(
            [("Joe Bloggs".to_string(), "reply@example.com".to_string())].into_iter(),
        ));
    let mut response = request.send_set_identity().await.unwrap();

    let updated = response
        .updated("ID-1")
        .unwrap()
        .expect("the echoed Identity must come back");
    assert_eq!(updated.id(), Some("ID-1"));
    assert_eq!(updated.name(), Some("Joe B"));
    assert!(!updated.may_delete());
    let reply_to = updated.reply_to().expect("replyTo must be present");
    assert_eq!(reply_to.len(), 1);
    assert_eq!(reply_to[0].name(), Some("Joe Bloggs"));
    assert_eq!(reply_to[0].email(), "reply@example.com");
}

#[tokio::test]
async fn identity_update_reports_not_updated_invalid_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notUpdated": {
                "ID-1": { "type": "invalidProperties", "properties": ["email"] }
            }
        }),
    )
    .await;

    let mut request = client.build();
    request.set_identity().update("ID-1").email("other@example.com");
    let response = request.send_set_identity().await.unwrap();

    // Check the aggregate error *before* the per-record lookup: `updated()`
    // removes the id from whichever map holds it (Finding #26).
    let err = response.unwrap_update_errors().unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::InvalidProperties);
    assert_eq!(set_err.properties(), Some(&["email".to_string()][..]));
    assert_eq!(
        response.not_updated_ids().unwrap().collect::<Vec<_>>(),
        vec!["ID-1"]
    );
}

#[tokio::test]
async fn identity_set_create_update_and_destroy_in_one_call() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv(
                "Identity/set",
                "s0",
                json!({
                    "accountId": ACCOUNT_ID,
                    "oldState": "99401312ae-11-333",
                    "newState": "99401312ae-11-335",
                    "created": { "c0": { "id": "ID-new", "email": "new@example.com" } },
                    "updated": { "ID-old": null },
                    "destroyed": ["ID-gone"]
                }),
            )],
            json!({ "c0": "ID-new" }),
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    {
        let set = request.set_identity();
        set.create().name("New").email("new@example.com");
        set.update("ID-old").name("Renamed");
        set.destroy(["ID-gone"]);
    }
    let mut response = request.send().await.unwrap();

    // The envelope-level `createdIds` maps the client creation id to the
    // server id (RFC 8620 §3.3/§3.4).
    let created_ids: Vec<(&String, &String)> = response.created_ids().unwrap().collect();
    assert_eq!(created_ids, vec![(&"c0".to_string(), &"ID-new".to_string())]);

    let mut set_response = response
        .method_response_by_pos(0)
        .unwrap_set_identity()
        .unwrap();
    assert!(set_response.has_created());
    assert!(set_response.has_updated());
    assert!(set_response.has_destroyed());
    assert_eq!(set_response.created("c0").unwrap().id(), Some("ID-new"));
    assert!(set_response.updated("ID-old").unwrap().is_none());
    set_response.destroyed("ID-gone").unwrap();
    assert_eq!(
        set_response.new_state(),
        "99401312ae-11-335"
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set", "method name must be on the wire");
    assert_eq!(call[1]["create"]["c0"]["email"], "new@example.com");
    assert_eq!(call[1]["update"]["ID-old"]["name"], "Renamed");
    assert_eq!(call[1]["destroy"], json!(["ID-gone"]));
}

#[tokio::test]
async fn identity_destroy_succeeds() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "2",
            "destroyed": ["ID-9"]
        }),
    )
    .await;

    client.identity_destroy("ID-9").await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set", "method name must be on the wire");
    assert_eq!(call[1]["destroy"], json!(["ID-9"]));
}

#[tokio::test]
async fn identity_destroy_reports_forbidden_when_may_delete_is_false() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §6: destroying a `mayDelete: false` Identity is rejected with
    // a standard `forbidden` SetError.
    mount_one(
        &server,
        "Identity/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "1",
            "newState": "1",
            "notDestroyed": {
                "XD-3301-222-11_22AAz": {
                    "type": "forbidden",
                    "description": "mayDelete is false"
                }
            }
        }),
    )
    .await;

    let err = client
        .identity_destroy("XD-3301-222-11_22AAz")
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::Forbidden);
    assert_eq!(set_err.description(), Some("mayDelete is false"));
}

#[tokio::test]
async fn identity_set_with_if_in_state_reports_state_mismatch() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.3: a stale `ifInState` is a method-level `stateMismatch`.
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "stateMismatch")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request
        .set_identity()
        .if_in_state("99401312ae-11-333")
        .update("ID-1")
        .name("Stale rename");
    let err = request.send_set_identity().await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}")
    };
    assert_eq!(method_err.error(), &MethodErrorType::StateMismatch);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/set");
    assert_eq!(call[1]["ifInState"], "99401312ae-11-333");
}

// ---------------------------------------------------------------------------
// 4. Identity/changes (RFC 8621 §6.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_changes_round_trips_every_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "99401312ae-11-333",
            "newState": "99401312ae-11-336",
            "hasMoreChanges": false,
            "created": ["ID-new"],
            "updated": ["XD-3301-222-11_22AAz"],
            "destroyed": ["ID-gone"]
        }),
    )
    .await;

    let response = client
        .identity_changes("99401312ae-11-333", 100)
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "99401312ae-11-333");
    assert_eq!(response.new_state(), "99401312ae-11-336");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["ID-new"]);
    assert_eq!(response.updated(), ["XD-3301-222-11_22AAz"]);
    assert_eq!(response.destroyed(), ["ID-gone"]);
    assert_eq!(response.total_changes(), 3);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[0],
        "Identity/changes",
        "method name must be on the wire"
    );
    assert_eq!(call[1]["sinceState"], "99401312ae-11-333");
    assert_eq!(call[1]["maxChanges"], 100);
}

#[tokio::test]
async fn identity_changes_reports_cannot_calculate_changes() {
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

    let err = client.identity_changes("1", 10).await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}")
    };
    assert_eq!(method_err.error(), &MethodErrorType::CannotCalculateChanges);
}

// ---------------------------------------------------------------------------
// 5. Capability surface
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identity_using_is_filtered_to_the_session_capabilities() {
    let server = MockServer::start().await;
    // What this proves: `Request::new` builds `using` by filtering a fixed
    // candidate list against what the session advertises, so a client never
    // opts in to a capability the server withheld (RFC 8620 §3.3).
    //
    // Note this is NOT a conformance claim about Identity: RFC 8621 §1.3.2
    // puts Identity under `urn:ietf:params:jmap:submission`, so a strict server
    // would reject Identity/get on this trimmed mail-only session with
    // `accountNotSupportedByMethod`. The mock answers regardless; the assertion
    // below is about `using` construction, not about server acceptance.
    let mut session = session_json(&server).await;
    let capabilities = session["capabilities"].as_object_mut().unwrap();
    capabilities.retain(|name, _| {
        name == "urn:ietf:params:jmap:core" || name == "urn:ietf:params:jmap:mail"
    });
    let primary_accounts = session["primaryAccounts"].as_object_mut().unwrap();
    primary_accounts.retain(|name, _| {
        name == "urn:ietf:params:jmap:core" || name == "urn:ietf:params:jmap:mail"
    });
    let client = connected_client_json(&server, session).await;

    mount_one(
        &server,
        "Identity/get",
        identity_get_body(vec![rfc_identity_joe()], vec![]),
    )
    .await;
    let identity = client
        .identity_get("XD-3301-222-11_22AAz", None::<IdentityProps>)
        .await
        .unwrap()
        .expect("the mock answers, so the record must parse");
    assert_eq!(identity.email(), Some("joe@example.com"));

    let call = single_jmap_request(&server).await;
    assert_eq!(
        call["using"],
        json!(["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"]),
        "`using` must be exactly the session-advertised capabilities (plus core)"
    );
}

// ---------------------------------------------------------------------------
// 6. Enum coverage
// ---------------------------------------------------------------------------

#[test]
fn identity_property_display_matches_the_rfc_property_names() {
    // RFC 8621 §6 property names.
    let expected = [
        (Property::Id, "id"),
        (Property::Name, "name"),
        (Property::Email, "email"),
        (Property::ReplyTo, "replyTo"),
        (Property::Bcc, "bcc"),
        (Property::TextSignature, "textSignature"),
        (Property::HtmlSignature, "htmlSignature"),
        (Property::MayDelete, "mayDelete"),
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
// 7. The RFC §6 selection rule, applied client-side
// ---------------------------------------------------------------------------

/// Picks the Identity a client should use for `address`: an exact address match
/// wins, otherwise a `*@domain` wildcard whose domain matches (RFC 8621 §6).
///
/// The crate exposes no such helper, so the rule lives here; the test pins the
/// behaviour the RFC prescribes and documents the gap.
fn most_likely_identity<'a>(
    identities: &'a [Identity],
    address: &str,
) -> Option<&'a Identity> {
    let (_, domain) = address.split_once('@')?;
    identities
        .iter()
        .find(|id| id.email() == Some(address))
        .or_else(|| {
            identities
                .iter()
                .find(|id| id.email() == Some(format!("*@{domain}").as_str()))
        })
}

#[tokio::test]
async fn identity_selection_prefers_an_exact_address_over_a_wildcard_identity() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Identity/get",
        identity_get_body(
            vec![rfc_identity_joe(), rfc_identity_wildcard()],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request.get_identity();
    let response = request.send_get_identity().await.unwrap();
    let identities = response.list();
    assert_eq!(identities.len(), 2);

    // Exact match beats the wildcard for the same domain.
    assert_eq!(
        most_likely_identity(identities, "joe@example.com")
            .unwrap()
            .id(),
        Some("XD-3301-222-11_22AAz")
    );
    // Any other address in the domain falls through to the wildcard record.
    assert_eq!(
        most_likely_identity(identities, "someone.else@example.com")
            .unwrap()
            .id(),
        Some("XD-9911312-11_22AAz")
    );
    // A different domain has no identity at all.
    assert!(most_likely_identity(identities, "joe@other.test").is_none());

    // `ids: null` (never set) is the RFC §6.1 "fetch all at once" form.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Identity/get", "method name must be on the wire");
    assert!(
        call[1].get("ids").is_none(),
        "an unset `ids` means \"all identities\" (RFC 8621 §6.1)"
    );
}

// ---------------------------------------------------------------------------
// The `Request::send_changes_identity` convenience wrapper
//
// `Request::send_changes_identity` is a thin `self.send_single().await`
// (src/identity/helpers.rs:124-126). `Client::identity_changes` calls
// `send_single` directly, so nothing else in the crate reaches the wrapper by
// name. The test builds the `/changes` call through the typed builder, sends
// it through the wrapper, asserts the parsed result, and spies on the wire so
// it cannot pass on an empty `methodCalls` array (Finding #31).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_changes_identity_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §6.2 / RFC 8620 §5.2 — every member required (Finding #8).
    mount_one(
        &server,
        "Identity/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "99401312ae-11-333",
            "newState": "99401312ae-11-336",
            "hasMoreChanges": false,
            "created": ["ID-new"],
            "updated": ["XD-3301-222-11_22AAz"],
            "destroyed": ["ID-gone"]
        }),
    )
    .await;

    let mut request = client.build();
    request.changes_identity("99401312ae-11-333").max_changes(100);
    let response = request.send_changes_identity().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "99401312ae-11-333");
    assert_eq!(response.new_state(), "99401312ae-11-336");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["ID-new"]);
    assert_eq!(response.updated(), ["XD-3301-222-11_22AAz"]);
    assert_eq!(response.destroyed(), ["ID-gone"]);

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "Identity/changes",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][1]["sinceState"], "99401312ae-11-333");
    assert_eq!(body["methodCalls"][0][1]["maxChanges"], 100);
}

#[tokio::test]
async fn send_changes_identity_surfaces_a_method_error() {
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

    let mut request = client.build();
    request.changes_identity("99401312ae-11-333");
    let err = request.send_changes_identity().await.unwrap_err();

    assert!(
        matches!(err, Error::Method(_)),
        "the wrapper must propagate an `error` Invocation as Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Identity/changes"
    );
}
