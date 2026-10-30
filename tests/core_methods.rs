//! P3 — core JMAP wire-protocol integration tests.
//!
//! Drives `jmap_client::client::Client` against a `wiremock` HTTP server using
//! the shared fixtures in `tests/common/mod.rs`. Every canned payload is taken
//! from the RFC example bodies where one exists (comments give the section);
//! where the RFC has no example the payload is the minimal RFC-correct shape,
//! also cited.
//!
//! Coverage targets: `client.rs` (connect / send / handle_error /
//! refresh_session / session staleness / account + timeout + headers),
//! `core/request.rs` (`using` filtering, call ids, result references),
//! `core/response.rs` (Response / TaggedMethodResponse / Get / Query / Set /
//! createdIds), `core/error.rs` (ProblemDetails / MethodError).

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use std::time::Duration;

use common::{
    connected_client, connected_client_json, expected_basic_auth, jmap_request_bodies,
    jmap_response, jmap_response_with_created_ids, mount_jmap, mount_session, mount_session_json,
    mount_session_template, problem_response, problem_response_with_charset, session_json,
    session_json_with_sieve, single_jmap_request, ACCOUNT_ID, API_PATH, SESSION_PATH,
    SESSION_STATE, USERNAME,
};
use jmap_client::client::Client;
use jmap_client::core::error::{JMAPError, MethodErrorType, ProblemType};
use jmap_client::core::request::ResultReference;
use jmap_client::core::set::SetErrorType;
use jmap_client::mailbox::{Property, Role};
use jmap_client::{Error, Method};
use serde_json::{json, Value};
use wiremock::MockServer;

// ---------------------------------------------------------------------------
// Fixtures
//
// A JMAP method response is an *Invocation* — the three-element array
// `[name, arguments, callId]` (RFC 8620 §3.2). `TaggedMethodResponse` is
// deserialised from that array, never from the bare arguments object, so every
// fixture below is wrapped with [`inv`].
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

/// Sends one `Mailbox/get` and returns whatever the server answered.
///
/// `Request::send_get_mailbox()` only sends the method calls that were added
/// to the builder, so the call has to be added explicitly — a bare
/// `client.build().send_get_mailbox()` would post `methodCalls: []`.
async fn send_mailbox_get(
    client: &Client,
) -> jmap_client::Result<jmap_client::core::response::MailboxGetResponse> {
    let mut request = client.build();
    request.get_mailbox();
    request.send_get_mailbox().await
}

/// RFC 8621 §2.6 — the `Mailbox/get` arguments for "fetch all Mailboxes in
/// an account" (the RFC's `Inbox` entry verbatim; the `…`-elided siblings are
/// dropped rather than invented).
fn mailbox_get_body(account_id: &str, state: &str) -> Value {
    json!({
        "accountId": account_id,
        "state": state,
        "list": [{
            "id": "MB23cfa8094c0f41e6",
            "name": "Inbox",
            "parentId": null,
            "role": "inbox",
            "sortOrder": 10,
            "totalEmails": 16307,
            "unreadEmails": 13905,
            "totalThreads": 5833,
            "unreadThreads": 5128,
            "myRights": {
                "mayAddItems": true,
                "mayRename": false,
                "maySubmit": true,
                "mayDelete": false,
                "maySetKeywords": true,
                "mayRemoveItems": true,
                "mayCreateChild": true,
                "maySetSeen": true,
                "mayReadItems": true
            },
            "isSubscribed": true
        }],
        "notFound": []
    })
}

/// RFC 8621 §5.5 (multi-call walkthrough) — `Email/query` arguments, with the
/// account id aligned to this suite's session fixture.
fn email_query_body(account_id: &str) -> Value {
    json!({
        "accountId": account_id,
        "queryState": "09aa9a075588-780599:0",
        "canCalculateChanges": true,
        "position": 0,
        "total": 115,
        "ids": [
            "Ma783e5cdf5f2deffbc97930a",
            "M9bd17497e2a99cb345fc1d0a"
        ]
    })
}

/// RFC 8620 §5.3 — `Mailbox/set` arguments for a call that created one
/// Mailbox. `created` is `Id[Id]` of creation id → the created object, which
/// MUST carry the server-assigned `id`.
fn mailbox_set_created_body(account_id: &str, old_state: &str, new_state: &str) -> Value {
    json!({
        "accountId": account_id,
        "oldState": old_state,
        "newState": new_state,
        "created": {
            "c1": {
                "id": "MBdfe8ab4d4e0c4f0",
                "name": "Maybe important mail",
                "parentId": null,
                "sortOrder": 0,
                "totalEmails": 0,
                "unreadEmails": 0,
                "totalThreads": 0,
                "unreadThreads": 0,
                "isSubscribed": false
            }
        }
    })
}

// ---------------------------------------------------------------------------
// 1. connect() — happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connect_reads_the_session_resource() {
    let server = MockServer::start().await;
    mount_session(&server).await;

    let client = Client::new()
        .credentials((USERNAME, "secret"))
        .connect(&server.uri())
        .await
        .unwrap();

    // RFC 8620 §2.2: the Session resource is discovered at /.well-known/jmap,
    // and Client::connect builds exactly that from the base URL.
    assert_eq!(
        client.session_url(),
        format!("{}{}", server.uri(), SESSION_PATH)
    );
    assert!(client.session_url().ends_with("/.well-known/jmap"));

    let session = client.session();
    assert_eq!(session.username(), USERNAME);
    assert_eq!(session.state(), SESSION_STATE);

    // default_account_id is the first primaryAccounts value.
    assert_eq!(client.default_account_id(), ACCOUNT_ID);

    // The API URL comes from the session, not from the connect URL.
    assert_eq!(session.api_url(), format!("{}{}", server.uri(), API_PATH));

    // Capability objects parsed into their intended client variants.
    let core = session
        .core_capabilities()
        .expect("core must parse as Capabilities::Core");
    assert_eq!(core.max_size_upload(), 50_000_000);
    assert_eq!(core.max_concurrent_upload(), 8);
    assert_eq!(core.max_size_request(), 10_000_000);
    assert_eq!(core.max_concurrent_requests(), 8);
    assert_eq!(core.max_calls_in_request(), 32);
    assert_eq!(core.max_objects_in_get(), 256);
    assert_eq!(core.max_objects_in_set(), 128);
    assert_eq!(
        core.collation_algorithms(),
        ["i;ascii-numeric", "i;ascii-casemap", "i;unicode-casemap"]
    );

    let mail = session
        .mail_capabilities()
        .expect("mail must parse as Capabilities::Mail");
    assert_eq!(mail.max_mailboxes_per_email(), None);
    assert_eq!(mail.max_mailbox_depth(), Some(10));
    assert_eq!(mail.max_size_mailbox_name(), 100);
    assert!(mail.may_create_top_level_mailbox());

    let submission = session
        .submission_capabilities()
        .expect("submission must parse as Capabilities::Submission");
    assert_eq!(
        submission
            .submission_extensions()
            .get("SIZE")
            .map(Vec::as_slice),
        Some(&["1229782938".to_string()][..]),
        "RFC 8621 1.3.2 submissionExtensions is a map of ehlo-name to ehlo-args"
    );
    assert_eq!(submission.submission_extensions().len(), 3);

    let ws = session
        .websocket_capabilities()
        .expect("websocket must parse as Capabilities::WebSocket");
    assert!(ws.supports_push());
    assert!(ws.url().starts_with("ws://"));

    // Sieve is deliberately not advertised (see common::DEFAULT_CAPABILITIES).
    assert!(!session.has_capability("urn:ietf:params:jmap:sieve"));
    assert!(session.sieve_capabilities().is_none());

    // The account record itself.
    let account = session.account(ACCOUNT_ID).unwrap();
    assert_eq!(account.name(), USERNAME);
    assert!(account.is_personal());
    assert!(!account.is_read_only());

    // The session fetch went out with Basic auth over the well-known path.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "GET");
    assert_eq!(requests[0].url.path(), SESSION_PATH);
    assert_eq!(
        requests[0].headers["Authorization"],
        expected_basic_auth(USERNAME, "secret")
    );
}

#[tokio::test]
async fn an_rfc_shaped_session_leaves_the_session_level_limit_accessors_empty() {
    // RFC 8621 §1.3.1/§1.3.2 (and draft-ietf-jmap-sieve §1.3.1) say the
    // *session-level* value of the mail and submission capabilities is an EMPTY
    // object; the limits belong in `accounts/<id>/accountCapabilities`. The
    // shared fixture duplicates the account object at session level so the
    // typed accessors have something to read (see `tests/common/mod.rs`), which
    // means no other test shows what a compliant server actually sends. This
    // pins that gap so it stays visible instead of being hidden by the fixture.
    let server = MockServer::start().await;
    let mut session = session_json(&server).await;
    session["capabilities"]["urn:ietf:params:jmap:mail"] = json!({});
    session["capabilities"]["urn:ietf:params:jmap:submission"] = json!({});
    let client = connected_client_json(&server, session).await;
    let session = client.session();

    // A compliant session still parses, and the capabilities are advertised...
    assert!(session.has_capability("urn:ietf:params:jmap:mail"));
    assert!(session.has_capability("urn:ietf:params:jmap:submission"));

    // ...but an empty session-level object is filed under
    // `Capabilities::Empty`, so the session-level typed limit accessors find
    // nothing. A caller that reads limits from the session rather than from the
    // account gets None against a spec-conformant server.
    assert!(session.mail_capabilities().is_none());
    assert!(session.submission_capabilities().is_none());

    // The limits are reachable where RFC 8621 §1.3.1/§1.3.2 actually put them:
    // on the account's `accountCapabilities`.
    let account = session.account(ACCOUNT_ID).expect("the fixture account");
    let mail = account
        .mail_capabilities()
        .expect("the account carries the RFC 8621 1.3.1 mail limits");
    assert_eq!(mail.max_mailbox_depth(), Some(10));
    assert_eq!(mail.max_size_mailbox_name(), 100);
    assert_eq!(mail.max_size_attachments_per_email(), Some(50_000_000));

    let submission = account
        .submission_capabilities()
        .expect("the account carries the RFC 8621 1.3.2 submission limits");
    assert_eq!(submission.max_delayed_send(), 4_294_967_295);
    assert_eq!(submission.submission_extensions().len(), 3);
}

#[tokio::test]
async fn connect_sends_requests_to_the_sessions_api_url() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut response = send_mailbox_get(&client).await.unwrap();

    assert_eq!(response.state(), "78540");
    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    assert!(response.not_found().is_empty());
    let mailbox = response.take_list().pop().unwrap();
    assert_eq!(mailbox.id(), Some("MB23cfa8094c0f41e6"));
    assert_eq!(mailbox.name(), Some("Inbox"));
    assert_eq!(mailbox.role(), Role::Inbox);
    assert_eq!(mailbox.total_emails(), 16307);
    assert_eq!(mailbox.unread_emails(), 13905);
    assert!(mailbox.is_subscribed());
    assert!(mailbox.my_rights().unwrap().may_read_items());

    // The POST landed on apiUrl, not on the base URL or the session URL.
    let post = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(post.url.path(), API_PATH);
}

// ---------------------------------------------------------------------------
// 2/3. connect() — error mapping
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connect_maps_problem_json_to_error_problem() {
    let server = MockServer::start().await;
    // RFC 8620 §3.6.1 problem document shape; `unauthorized` is what a JMAP
    // server returns for bad credentials.
    mount_session_template(
        &server,
        problem_response(
            401,
            json!({
                "type": "urn:ietf:params:jmap:error:unauthorized",
                "status": 401,
                "title": "Unauthorized",
                "detail": "The supplied credentials are not valid."
            }),
        ),
    )
    .await;

    let err = match Client::new()
        .credentials((USERNAME, "wrong"))
        .connect(&server.uri())
        .await
    {
        Ok(_) => panic!("connect() must fail against a 401 problem response"),
        Err(err) => err,
    };

    let Error::Problem(problem) = err else {
        panic!("expected Error::Problem, got {err:?}");
    };
    // `unauthorized` is not one of the four URIs registered in `JMAPError`, so
    // the untagged ProblemType lands in `Other` and keeps the URI verbatim.
    match problem.error() {
        ProblemType::Other(uri) => {
            assert_eq!(uri, "urn:ietf:params:jmap:error:unauthorized")
        }
        ProblemType::JMAP(other) => panic!("expected ProblemType::Other, got {other:?}"),
    }
    assert_eq!(problem.status(), Some(401));
    assert_eq!(problem.title(), Some("Unauthorized"));
    assert_eq!(
        problem.detail(),
        Some("The supplied credentials are not valid.")
    );
    assert_eq!(problem.limit(), None);
}

#[tokio::test]
async fn connect_maps_non_problem_error_body_to_error_server() {
    let server = MockServer::start().await;
    // A plain text/plain 500: handle_error cannot match the problem content
    // type, so it must fall through to Error::Server.
    mount_session_template(
        &server,
        wiremock::ResponseTemplate::new(500).set_body_string("Internal Server Error"),
    )
    .await;

    let err = match Client::new()
        .credentials((USERNAME, "secret"))
        .connect(&server.uri())
        .await
    {
        Ok(_) => panic!("connect() must fail against a 500 response"),
        Err(err) => err,
    };

    let Error::Server(message) = err else {
        panic!("expected Error::Server, got {err:?}");
    };
    assert!(
        message.contains("500"),
        "Error::Server should carry the HTTP status, got {message:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. single-method request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn single_method_request_uses_advertised_capabilities_and_call_id_s0() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox().properties([
        Property::Id,
        Property::Name,
        Property::ParentId,
        Property::Role,
        Property::SortOrder,
        Property::TotalEmails,
        Property::UnreadEmails,
        Property::TotalThreads,
        Property::UnreadThreads,
        Property::MyRights,
        Property::IsSubscribed,
        Property::ShareWith,
    ]);
    let get = request.send_get_mailbox().await.unwrap();
    assert_eq!(get.list().len(), 1);

    // --- what actually went over the wire (RFC 8620 §3.3) ---
    let body = single_jmap_request(&server).await;

    // `using` = the canonical order of the crate's KNOWN_CAPABILITIES filtered
    // to what the session advertised. Contacts/Calendars/Sieve are absent from
    // the session, so they must not appear.
    assert_eq!(
        body["using"],
        json!([
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:websocket",
            "urn:ietf:params:jmap:blob",
            "urn:ietf:params:jmap:quota",
            "urn:ietf:params:jmap:principals"
        ]),
        "using must be the canonical order of advertised capabilities"
    );
    assert_eq!(body["using"][0], "urn:ietf:params:jmap:core");

    // RFC 8620 §3.2: methodCalls is an array of [name, arguments, callId].
    let calls = body["methodCalls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0], "Mailbox/get");
    assert_eq!(calls[0][2], "s0");
    assert_eq!(calls[0][1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        calls[0][1]["properties"],
        json!([
            "id",
            "name",
            "parentId",
            "role",
            "sortOrder",
            "totalEmails",
            "unreadEmails",
            "totalThreads",
            "unreadThreads",
            "myRights",
            "isSubscribed",
            "shareWith"
        ])
    );
    // No `ids` argument was set, so none is serialised (RFC 8620 §5.1: an
    // omitted ids means "all").
    assert!(calls[0][1].get("ids").is_none());
    assert!(calls[0][1].get("#ids").is_none());

    // The JMAP POST carries the same auth as the session fetch, plus JSON.
    let post = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.method.as_str() == "POST")
        .unwrap();
    assert_eq!(
        post.headers["Authorization"],
        expected_basic_auth(USERNAME, "secret")
    );
    assert_eq!(post.headers["Content-Type"], "application/json");
    assert!(post.headers["User-Agent"]
        .to_str()
        .unwrap()
        .starts_with("jmap-client/"));
}

// ---------------------------------------------------------------------------
// 5. multi-method request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multi_method_request_returns_both_responses_in_order() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("Mailbox/get", "s0", mailbox_get_body(ACCOUNT_ID, "78540")),
                inv("Email/query", "s1", email_query_body(ACCOUNT_ID)),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox().ids(["MB23cfa8094c0f41e6"]);
    request.query_email().calculate_total(true);

    let response = request.send().await.unwrap();
    assert_eq!(response.session_state(), SESSION_STATE);
    assert_eq!(response.method_responses().len(), 2);

    let mut responses = response.unwrap_method_responses();
    let second = responses.pop().unwrap();
    let first = responses.pop().unwrap();

    assert_eq!(first.call_id(), "s0");
    assert!(first.is_type(Method::GetMailbox));
    assert!(!first.is_error());

    assert_eq!(second.call_id(), "s1");
    assert!(second.is_type(Method::QueryEmail));
    let query = second.unwrap_query_email().unwrap();
    assert_eq!(query.account_id(), ACCOUNT_ID);
    assert_eq!(query.query_state(), "09aa9a075588-780599:0");
    assert_eq!(query.position(), 0);
    assert_eq!(query.total(), Some(115));
    assert!(query.can_calculate_changes());
    assert_eq!(query.ids().len(), 2);

    // --- wire check ---
    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "Mailbox/get");
    assert_eq!(calls[0][2], "s0");
    assert_eq!(calls[1][0], "Email/query");
    assert_eq!(calls[1][2], "s1");
    assert_eq!(calls[1][1]["calculateTotal"], true);
}

#[tokio::test]
async fn method_responses_can_be_looked_up_by_call_id_and_position() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("Mailbox/get", "s0", mailbox_get_body(ACCOUNT_ID, "78540")),
                inv("Email/query", "s1", email_query_body(ACCOUNT_ID)),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox();
    request.query_email();
    let mut response = request.send().await.unwrap();

    assert!(response.method_response_by_id("s1").is_some());
    assert!(response.method_response_by_id("s9").is_none());

    // method_response_by_pos removes from the list, so the ordering is visible.
    let by_pos = response.method_response_by_pos(0);
    assert!(by_pos.is_type(Method::GetMailbox));
    assert_eq!(response.method_responses().len(), 1);
    assert!(response
        .pop_method_response()
        .unwrap()
        .is_type(Method::QueryEmail));
    assert!(response.pop_method_response().is_none());
}

// ---------------------------------------------------------------------------
// 6. result references
// ---------------------------------------------------------------------------

#[tokio::test]
async fn last_result_reference_serialises_as_result_of_name_path() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §2.6 shows exactly this pattern: a get whose ids come from the
    // previous call's /list/*/id.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("Mailbox/get", "s0", mailbox_get_body(ACCOUNT_ID, "78540")),
                inv("Mailbox/get", "s1", mailbox_get_body(ACCOUNT_ID, "78541")),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox().ids(["MB23cfa8094c0f41e6"]);
    let reference = request.last_result_reference("/list/*/id");
    request.get_mailbox().ids_ref(reference);
    let response = request.send().await.unwrap();
    assert_eq!(response.method_responses().len(), 2);

    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 2);

    // RFC 8620 §3.7: the argument is named "#ids" and its value is a
    // ResultReference { resultOf, name, path }.
    assert_eq!(
        calls[1][1]["#ids"],
        json!({
            "resultOf": "s0",
            "name": "Mailbox/get",
            "path": "/list/*/id"
        })
    );
    // Setting a reference clears the literal form: RFC 8620 §3.7 makes having
    // both `ids` and `#ids` an invalidArguments error.
    assert!(calls[1][1].get("ids").is_none());
    // The first call is untouched.
    assert_eq!(calls[0][1]["ids"], json!(["MB23cfa8094c0f41e6"]));
    assert!(calls[0][1].get("#ids").is_none());
}

#[tokio::test]
async fn result_reference_can_target_an_earlier_call_by_index() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("Mailbox/get", "s0", mailbox_get_body(ACCOUNT_ID, "78540")),
                inv("Email/query", "s1", email_query_body(ACCOUNT_ID)),
                inv("Mailbox/get", "s2", mailbox_get_body(ACCOUNT_ID, "78541")),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox();
    request.query_email();
    // Explicit reference to call index 0 rather than the last call.
    request
        .get_mailbox()
        .ids_ref(ResultReference::new(Method::GetMailbox, 0, "/list/*/id"));
    request.send().await.unwrap();

    let calls = single_jmap_request(&server).await["methodCalls"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2][2], "s2");
    assert_eq!(calls[2][1]["#ids"]["resultOf"], "s0");
    assert_eq!(calls[2][1]["#ids"]["name"], "Mailbox/get");
    assert_eq!(calls[2][1]["#ids"]["path"], "/list/*/id");
}

// ---------------------------------------------------------------------------
// 7. createdIds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn created_ids_deserialise_and_the_created_object_is_usable() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.4: createdIds is a map of client creation id to the
    // server-assigned id, returned alongside methodResponses.
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv(
                "Mailbox/set",
                "s0",
                mailbox_set_created_body(ACCOUNT_ID, "78542", "78549"),
            )],
            json!({ "c1": "MBdfe8ab4d4e0c4f0" }),
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request
        .set_mailbox()
        .create_with_id("c1")
        .name("Maybe important mail");

    let mut response = request.send().await.unwrap();

    // Top-level createdIds (RFC 8620 §3.3/§3.4).
    let mut created_ids: Vec<(String, String)> = response
        .created_ids()
        .expect("createdIds must deserialise")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    created_ids.sort();
    assert_eq!(
        created_ids,
        vec![("c1".to_string(), "MBdfe8ab4d4e0c4f0".to_string())]
    );

    // The per-record created object from the Mailbox/set response.
    let mut set_response = response
        .method_response_by_pos(0)
        .unwrap_set_mailbox()
        .unwrap();
    assert_eq!(set_response.old_state(), Some("78542"));
    assert_eq!(set_response.new_state(), "78549");
    assert!(set_response.has_created());

    let mut created = set_response.created("c1").unwrap();
    assert_eq!(created.id(), Some("MBdfe8ab4d4e0c4f0"));
    assert_eq!(created.take_id(), "MBdfe8ab4d4e0c4f0");
    assert_eq!(created.name(), Some("Maybe important mail"));
}

#[tokio::test]
async fn set_error_in_not_created_surfaces_as_error_set() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §5.3: a per-record failure is reported in notCreated with a
    // SetError object, not as a method-level error.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/set",
                "s0",
                json!({
                    "accountId": ACCOUNT_ID,
                    "oldState": "78542",
                    "newState": "78542",
                    "notCreated": {
                        "c1": {
                            "type": "mailboxHasChild",
                            "description": "Mailbox already has a child with that name",
                            "properties": ["name"]
                        }
                    }
                }),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.set_mailbox().create_with_id("c1").name("Inbox");

    let mut response = request.send().await.unwrap();
    let mut set_response = response
        .method_response_by_pos(0)
        .unwrap_set_mailbox()
        .unwrap();

    // Check the aggregate error first: `created()` *removes* the entry from
    // not_created, so a later `unwrap_create_errors()` would see nothing.
    assert!(set_response.unwrap_create_errors().is_err());
    assert_eq!(
        set_response.not_created_ids().unwrap().collect::<Vec<_>>(),
        vec!["c1"]
    );

    let err = set_response.created("c1").unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}");
    };
    assert_eq!(set_err.type_, SetErrorType::MailboxHasChild);
    assert_eq!(
        set_err.description(),
        Some("Mailbox already has a child with that name")
    );
    assert_eq!(set_err.properties(), Some(&["name".to_string()][..]));
}

// ---------------------------------------------------------------------------
// 8. capability filtering over the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unadvertised_capability_is_absent_from_using() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await; // no sieve in the session

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    send_mailbox_get(&client).await.unwrap();

    let using = single_jmap_request(&server).await["using"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        !using.contains(&json!("urn:ietf:params:jmap:sieve")),
        "a capability the server did not advertise must not be used: {using:?}"
    );
    // And the string never appears anywhere in the request body.
    assert!(jmap_request_bodies(&server)
        .await
        .iter()
        .all(|body| !body.to_string().contains("sieve")));
}

#[tokio::test]
async fn advertised_capability_is_present_in_using() {
    let server = MockServer::start().await;
    // Same server shape, but the session advertises sieve.
    let sieve_session = session_json_with_sieve(&server).await;
    assert!(sieve_session["capabilities"]
        .get("urn:ietf:params:jmap:sieve")
        .is_some());
    assert!(session_json(&server).await["capabilities"]
        .get("urn:ietf:params:jmap:sieve")
        .is_none());

    let client = connected_client_json(&server, sieve_session).await;
    assert!(client
        .session()
        .has_capability("urn:ietf:params:jmap:sieve"));

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    send_mailbox_get(&client).await.unwrap();

    let using = single_jmap_request(&server).await["using"]
        .as_array()
        .unwrap()
        .clone();
    let as_strings: Vec<&str> = using.iter().map(|v| v.as_str().unwrap()).collect();
    // Canonical order preserved: sieve sits between websocket and blob.
    assert_eq!(
        as_strings,
        [
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:websocket",
            "urn:ietf:params:jmap:sieve",
            "urn:ietf:params:jmap:blob",
            "urn:ietf:params:jmap:quota",
            "urn:ietf:params:jmap:principals",
        ]
    );
}

#[tokio::test]
async fn core_capability_is_always_used_even_when_not_advertised() {
    let server = MockServer::start().await;
    // A session that advertises mail only.
    let mut session = session_json(&server).await;
    let mut caps = serde_json::Map::new();
    caps.insert(
        "urn:ietf:params:jmap:mail".to_string(),
        session["capabilities"]["urn:ietf:params:jmap:mail"].clone(),
    );
    session["capabilities"] = Value::Object(caps);
    let client = connected_client_json(&server, session).await;
    assert!(!client.session().has_capability("urn:ietf:params:jmap:core"));

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;
    send_mailbox_get(&client).await.unwrap();

    // RFC 8620 §3.6.1 (client behaviour): core is always included.
    assert_eq!(
        single_jmap_request(&server).await["using"],
        json!(["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"])
    );
}

// ---------------------------------------------------------------------------
// 9. method-level errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn method_level_error_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.6.2: an error Invocation, delivered with HTTP 200.
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "accountNotFound")], SESSION_STATE),
    )
    .await;

    let mut request = client.build().account_id("doesNotExist");
    request.get_mailbox();
    let err = request.send_get_mailbox().await.unwrap_err();

    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(method_err.error(), &MethodErrorType::AccountNotFound);
    assert_eq!(method_err.to_string(), "Account not found");
}

#[tokio::test]
async fn method_level_error_is_visible_on_the_tagged_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv("Mailbox/get", "s0", mailbox_get_body(ACCOUNT_ID, "78540")),
                method_error("s1", "invalidArguments"),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox();
    request.query_email();
    let mut response = request.send().await.unwrap();

    assert!(!response.method_response_by_id("s0").unwrap().is_error());
    assert!(response.method_response_by_id("s1").unwrap().is_error());
    assert!(response
        .method_response_by_id("s1")
        .unwrap()
        .is_type(Method::Error));

    // A typed unwrap on an error response yields the method error rather than a
    // type-mismatch.
    let err = response
        .method_response_by_pos(1)
        .unwrap_query_email()
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
}

#[tokio::test]
async fn an_unrecognised_method_error_type_surfaces_as_a_method_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.6.2: "Should a client receive an error type it does not
    // understand, it MUST treat it the same as the `serverFail` type." The
    // client can only do that when the type parses, so an unmodelled type now
    // lands in `MethodErrorType::Other` and reaches the caller as
    // `Error::Method` instead of failing the parse of the whole Response
    // object as `Error::Parse`.
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "urn:ietf:params:jmap:error:notFound")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(
        method_err.error(),
        &MethodErrorType::Other("urn:ietf:params:jmap:error:notFound".to_string()),
        "the catch-all must carry the exact string the server sent"
    );
    assert!(
        method_err.is_server_fail_equivalent(),
        "an unrecognised type MUST be handled as serverFail"
    );
    assert!(
        method_err.to_string().contains("server fail"),
        "the rendering says so too: {method_err}"
    );
}

#[tokio::test]
async fn an_unrecognised_method_error_on_a_set_response_surfaces_as_a_method_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // The hazard this test used to pin (Finding #112), now fixed.
    // `SingleMethodResponse` is `#[serde(untagged)]` (src/core/response.rs),
    // so the `Error` arm is tried first and only *its* deserialisation failure
    // moves on to `Ok`. `SetResponse` makes every member optional, so an
    // `error` Invocation whose `type` was not modelled deserialised straight
    // into an all-`None` `SetResponse` and the failed write vanished without a
    // trace. RFC 8620 §3.6.2 requires the client to treat that type as
    // `serverFail`, which starts with parsing it: the catch-all makes the
    // `Error` arm win, so the caller gets `Error::Method`.
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "tooManySubscriptions")],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.set_mailbox().update("mf2").name("Renamed");
    let err = request
        .send_set_mailbox()
        .await
        .expect_err("an error Invocation must never read as an empty success");

    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(
        method_err.error(),
        &MethodErrorType::Other("tooManySubscriptions".to_string()),
        "the server's own error type reaches the caller"
    );
    assert!(
        method_err.is_server_fail_equivalent(),
        "an unrecognised type MUST be handled as serverFail"
    );

    // The call really did go out, so this is the response to *our* request
    // and not a fixture that never matched (Finding #31).
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/set"
    );
}

#[tokio::test]
async fn a_set_response_with_an_unmodelled_error_type_yields_error_method_when_unwrapped() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // Same RFC 8620 §3.6.2 requirement on the multi-call path: the error
    // Invocation is deserialised through `TaggedMethodResponse`, and
    // `unwrap_set_mailbox` must hand back `Error::Method` rather than a
    // silently empty `SetResponse`. A Set response is the dangerous shape
    // because every `SetResponse` member is optional.
    mount_jmap(
        &server,
        jmap_response(
            vec![
                // RFC 8621 §2.6 — a Mailbox/set that renamed one Mailbox.
                inv(
                    "Mailbox/set",
                    "s0",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "oldState": "78542",
                        "newState": "78549",
                        "updated": { "mf2": null }
                    }),
                ),
                method_error("s1", "urn:example:params:error:boom"),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.set_mailbox().update("mf2").name("Renamed");
    request.set_mailbox().update("mf3").name("Renamed too");
    let mut response = request.send().await.unwrap();

    assert!(!response.method_response_by_id("s0").unwrap().is_error());
    let err = response
        .method_response_by_pos(1)
        .unwrap_set_mailbox()
        .unwrap_err();
    let Error::Method(method_err) = err else {
        panic!("expected Error::Method, got {err:?}");
    };
    assert_eq!(
        method_err.error(),
        &MethodErrorType::Other("urn:example:params:error:boom".to_string())
    );
    assert!(method_err.is_server_fail_equivalent());
}

/// RFC 8620 §3.6.2 — a method-level error must never be read as a success,
/// whatever the shape of its payload.
///
/// The root cause behind Finding #112: `SingleMethodResponse` was
/// `#[serde(untagged)]`, so serde tried the `Error` arm and, on *any* failure of
/// that arm, silently retried the `Ok` arm. `Ok` binds element 0 as a bare
/// `String`, so `["error", <payload>, "s0"]` became `Ok(("error", <payload>,
/// "s0"))` whenever `<payload>` was not a valid method error — and every member
/// of a `SetResponse` is optional, so a `Set` refusal deserialised into an
/// all-`None` success and the caller was told the write had worked. The arm is
/// now chosen on element 0 alone, so a payload that will not parse fails loudly
/// instead of becoming a fake success.
#[tokio::test]
async fn a_malformed_method_error_on_a_set_response_is_never_a_silent_success() {
    // `type` is a required member of the method error object (RFC 8620 §3.6.2).
    // The last field says whether the payload is still a parseable error.
    let cases = [
        (json!({}), "carries no `type` member", false),
        (json!({ "type": 42 }), "spells `type` as a number", false),
        (
            json!({ "type": "serverFail", "description": 999 }),
            "carries a wrong-typed extra member",
            true,
        ),
        (json!("serverFail"), "is not an object at all", false),
        (Value::Null, "is null", false),
    ];

    for (payload, why, is_a_parseable_error) in cases {
        let server = MockServer::start().await;
        let client = connected_client(&server).await;
        mount_jmap(
            &server,
            jmap_response(vec![inv("error", "s0", payload)], SESSION_STATE),
        )
        .await;

        let mut request = client.build();
        request.set_mailbox().update("mf2").name("Renamed");
        let err = request.send_set_mailbox().await.expect_err(&format!(
            "an error Invocation that {why} must never read as an empty success"
        ));

        if is_a_parseable_error {
            let Error::Method(method_err) = err else {
                panic!("expected Error::Method, got {err:?}");
            };
            assert_eq!(
                method_err.error(),
                &MethodErrorType::ServerFail,
                "the server's own error type must survive"
            );
        } else {
            assert!(
                matches!(err, Error::Parse(_)),
                "expected Error::Parse, got {err:?}"
            );
        }

        // The call really did go out, so this is the response to *our* request
        // and not a fixture that never matched (Finding #31).
        assert_eq!(
            single_jmap_request(&server).await["methodCalls"][0][0],
            "Mailbox/set"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. request-level problem on POST
// ---------------------------------------------------------------------------

#[tokio::test]
async fn problem_json_on_post_maps_to_error_problem() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.6.1.1, first example, verbatim.
    mount_jmap(
        &server,
        problem_response(
            400,
            json!({
                "type": "urn:ietf:params:jmap:error:unknownCapability",
                "status": 400,
                "detail": "The Request object used capability 'https://example.com/apis/foobar', which is not supported by this server."
            }),
        ),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    let Error::Problem(problem) = err else {
        panic!("expected Error::Problem, got {err:?}");
    };
    assert!(matches!(
        problem.error(),
        ProblemType::JMAP(JMAPError::UnknownCapability)
    ));
    assert_eq!(problem.status(), Some(400));
    assert!(problem
        .detail()
        .unwrap()
        .contains("not supported by this server"));
    // Display renders the type, the status and the detail.
    let rendered = problem.to_string();
    assert!(rendered.contains("Unknown capability"), "{rendered}");
    assert!(rendered.contains("status 400"), "{rendered}");
}

#[tokio::test]
async fn limit_problem_carries_the_limit_name() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.6.1.1, second example, verbatim.
    mount_jmap(
        &server,
        problem_response(
            400,
            json!({
                "type": "urn:ietf:params:jmap:error:limit",
                "limit": "maxSizeRequest",
                "status": 400,
                "detail": "The request is larger than the server is willing to process."
            }),
        ),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    let Error::Problem(problem) = err else {
        panic!("expected Error::Problem, got {err:?}");
    };
    assert!(matches!(
        problem.error(),
        ProblemType::JMAP(JMAPError::Limit)
    ));
    assert_eq!(problem.limit(), Some("maxSizeRequest"));
    assert_eq!(problem.status(), Some(400));
}

#[tokio::test]
async fn problem_json_with_a_charset_parameter_still_maps_to_error_problem() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8620 §3.6.1 serves every request-level error as an RFC 7807 problem
    // document, and RFC 7231 §3.1.1.1 makes a media type case-insensitive and
    // its parameters a separate thing. `application/problem+json;
    // charset=utf-8` is therefore the same media type, and the client must
    // parse the body into ProblemDetails either way. Byte-comparing the header
    // used to answer this with the generic Error::Server and throw `type`,
    // `status` and `detail` away (Finding #7).
    mount_jmap(
        &server,
        problem_response_with_charset(
            400,
            // RFC 8620 §3.6.1.1, first example, verbatim.
            json!({
                "type": "urn:ietf:params:jmap:error:unknownCapability",
                "status": 400,
                "detail": "The Request object used capability 'https://example.com/apis/foobar', which is not supported by this server."
            }),
        ),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    let Error::Problem(problem) = err else {
        panic!(
            "a charset parameter on the media type must not lose the problem \
             details, got {err:?}"
        );
    };
    assert!(
        matches!(
            problem.error(),
            ProblemType::JMAP(JMAPError::UnknownCapability)
        ),
        "the problem type must survive the charset parameter"
    );
    assert_eq!(problem.status(), Some(400));
    assert!(problem
        .detail()
        .unwrap()
        .contains("not supported by this server"));
}

// ---------------------------------------------------------------------------
// 11. malformed success body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn garbage_body_with_200_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // 2xx means handle_error passes the body straight to serde_json, which must
    // fail with Error::Parse.
    mount_jmap(
        &server,
        wiremock::ResponseTemplate::new(200)
            .set_body_raw("<html>not json</html>", "application/json"),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

#[tokio::test]
async fn a_valid_json_non_response_body_is_a_parse_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // Well-formed JSON that is not a JMAP Response object. RFC 8620 §3.6.1
    // would call this `notRequest`, but the client has no such mapping and
    // reports the deserialisation failure instead.
    mount_jmap(
        &server,
        wiremock::ResponseTemplate::new(200).set_body_json(json!({ "hello": "world" })),
    )
    .await;

    let err = send_mailbox_get(&client).await.unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 12. sessionState staleness + refresh
// ---------------------------------------------------------------------------

#[tokio::test]
async fn session_state_mismatch_marks_the_session_stale_until_refreshed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    assert!(client.is_session_updated());
    assert_eq!(client.session().state(), SESSION_STATE);

    // A response carrying a different sessionState (RFC 8620 §3.4) makes the
    // cached Session out of date.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78541"),
            )],
            "75128aab4b1c-different",
        ),
    )
    .await;
    send_mailbox_get(&client).await.unwrap();
    assert!(
        !client.is_session_updated(),
        "a differing sessionState must mark the session as not updated"
    );

    // Another matching-but-still-different response does not clear the flag.
    send_mailbox_get(&client).await.unwrap();
    assert!(!client.is_session_updated());

    // Re-mount the Session resource at a new state and refresh.
    server.reset().await;
    let mut refreshed = session_json(&server).await;
    refreshed["state"] = json!("75128aab4b1d-refreshed");
    mount_session_json(&server, refreshed).await;

    client.refresh_session().await.unwrap();
    assert!(client.is_session_updated());
    assert_eq!(client.session().state(), "75128aab4b1d-refreshed");
    assert_eq!(client.default_account_id(), ACCOUNT_ID);
}

#[tokio::test]
async fn a_matching_session_state_keeps_the_session_fresh() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body(ACCOUNT_ID, "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;
    send_mailbox_get(&client).await.unwrap();
    assert!(client.is_session_updated());
}

// ---------------------------------------------------------------------------
// 13. default account id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_default_account_id_changes_the_account_id_on_the_wire() {
    let server = MockServer::start().await;
    let mut client = connected_client(&server).await;

    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Mailbox/get",
                "s0",
                mailbox_get_body("A97813", "78540"),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // RFC 8620 §2.1: "A97813" is the shape of a second (shared, read-only)
    // account in the RFC's example session.
    client.set_default_account_id("A97813");
    assert_eq!(client.default_account_id(), "A97813");

    send_mailbox_get(&client).await.unwrap();
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["accountId"],
        "A97813"
    );

    // A per-request override still wins over the client default.
    let mut request = client.build().account_id("A13824");
    request.get_mailbox();
    request.send().await.unwrap();
    assert_eq!(
        jmap_request_bodies(&server).await[1]["methodCalls"][0][1]["accountId"],
        "A13824"
    );
}

// ---------------------------------------------------------------------------
// 14. timeout + headers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn timeout_round_trips_through_the_builder_and_the_setter() {
    let server = MockServer::start().await;
    mount_session(&server).await;

    let mut client = Client::new()
        .credentials((USERNAME, "secret"))
        .timeout(Duration::from_secs(42))
        .connect(&server.uri())
        .await
        .unwrap();
    assert_eq!(client.timeout(), Duration::from_secs(42));

    client.set_timeout(Duration::from_millis(1234));
    assert_eq!(client.timeout(), Duration::from_millis(1234));

    // The builder default is 10s.
    let default_client = Client::new()
        .credentials((USERNAME, "secret"))
        .connect(&server.uri())
        .await
        .unwrap();
    assert_eq!(default_client.timeout(), Duration::from_secs(10));
}

#[tokio::test]
async fn headers_carry_authorization_user_agent_and_content_type() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    let headers = client.headers();
    assert_eq!(
        headers["Authorization"],
        expected_basic_auth(USERNAME, "secret")
    );
    assert!(headers["User-Agent"]
        .to_str()
        .unwrap()
        .starts_with("jmap-client/"));
    // connect() adds Content-Type after the session fetch, so POSTs are JSON.
    assert_eq!(headers["Content-Type"], "application/json");
    assert!(!headers.contains_key("Forwarded"));
}

#[tokio::test]
async fn bearer_credentials_produce_a_bearer_authorization_header() {
    let server = MockServer::start().await;
    mount_session(&server).await;

    let client = Client::new()
        .credentials("some-api-token")
        .connect(&server.uri())
        .await
        .unwrap();

    assert_eq!(client.headers()["Authorization"], "Bearer some-api-token");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers["Authorization"],
        "Bearer some-api-token"
    );
}

#[tokio::test]
async fn forwarded_for_is_sent_when_configured() {
    let server = MockServer::start().await;
    mount_session(&server).await;

    let client = Client::new()
        .credentials((USERNAME, "secret"))
        .forwarded_for("203.0.113.7".parse().unwrap())
        .connect(&server.uri())
        .await
        .unwrap();

    // The header must actually reach the wire, not just the client's header map:
    // the session GET above is the request that proves it was transmitted.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].headers["Forwarded"], "for=203.0.113.7");
    assert_eq!(client.headers()["Forwarded"], "for=203.0.113.7");
}
