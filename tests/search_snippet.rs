//! P4 — mail-domain integration tests: **Search snippets**.
//!
//! Drives `Client::search_snippet_get` and the
//! `Request::{get_search_snippet, send_get_search_snippet}` builders from
//! `src/email/helpers.rs` against a `wiremock` HTTP server using the shared
//! fixtures in `tests/common/mod.rs`. The happy-path payload is the RFC 8621
//! §5.2 example, verbatim.
//!
//! Coverage targets: `email/search_snippet.rs` (`SearchSnippet` accessors,
//! `SearchSnippetGetRequest` builders including the `#emailIds` result
//! reference, `SearchSnippetGetResponse` lookup) plus the `SearchSnippet/get`
//! wiring in `core::request` / `core::response`.
//!
//! RFC 8621 §5.1 defines the object: `emailId` (the id of the Email the
//! snippet is for), `subject` (String|null, with `<mark>` around matches) and
//! `preview` (String|null). Note there is deliberately **no** `partId` /
//! `emailSnippet` member in this client's model — see the findings log.

mod common;

use common::{
    connected_client, jmap_response, mount_jmap, single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::query::Filter;
use jmap_client::core::request::ResultReference;
use jmap_client::email::query as email_query;
use jmap_client::Method;
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

/// A filter list for the "no filter" case, where `None` would be uninferable.
type SnippetFilter = Filter<email_query::Filter>;

// ---------------------------------------------------------------------------
// Fixtures
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

/// RFC 8621 §5.2 — the `SearchSnippet/get` response, verbatim (the RFC's
/// `...` continuation lines are elided; the three shown records are kept).
fn rfc_search_snippet_response() -> Value {
    json!([
        {
            "emailId": "M44200ec123de277c0c1ce69c",
            "subject": null,
            "preview": null
        },
        {
            "emailId": "M7bcbcb0b58d7729686e83d99",
            "subject": "The <mark>Foo</mark>sball competition",
            "preview": "...year the <mark>foo</mark>sball competition will be held in the Stadium de ..."
        },
        {
            "emailId": "M28d12783a0969584b6deaac0",
            "subject": null,
            "preview": "...the <mark>Foo</mark>/bar method results often returns &lt;1 widget rather than the complete..."
        }
    ])
}

/// RFC 8621 §5.2 — the full `SearchSnippet/get` response envelope.
fn rfc_snippet_get_body() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "list": rfc_search_snippet_response(),
        "notFound": null
    })
}

// ---------------------------------------------------------------------------
// 1. SearchSnippet/get through the Client helper
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_snippet_get_returns_the_rfc_8621_example() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "SearchSnippet/get", rfc_snippet_get_body()).await;

    let response = client
        .search_snippet_get(
            Some(email_query::Filter::text("foo")),
            [
                "M44200ec123de277c0c1ce69c",
                "M7bcbcb0b58d7729686e83d99",
                "M28d12783a0969584b6deaac0",
            ],
        )
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.list().len(), 3);
    assert!(response.not_found().is_none());

    // The `<mark>` highlighting is carried through verbatim; the client does
    // not interpret it.
    let hit = response
        .snippet("M7bcbcb0b58d7729686e83d99")
        .expect("the second snippet must be findable by emailId");
    assert_eq!(hit.email_id(), "M7bcbcb0b58d7729686e83d99");
    assert_eq!(hit.subject(), Some("The <mark>Foo</mark>sball competition"));
    assert_eq!(
        hit.preview(),
        Some(
            "...year the <mark>foo</mark>sball competition will be held in the Stadium de ..."
        )
    );

    // A record whose subject and preview are both null is still a snippet.
    let miss = response
        .snippet("M44200ec123de277c0c1ce69c")
        .expect("the first snippet must be findable");
    assert_eq!(miss.subject(), None);
    assert_eq!(miss.preview(), None);
}

#[tokio::test]
async fn search_snippet_get_puts_the_filter_and_ids_on_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "SearchSnippet/get", rfc_snippet_get_body()).await;

    client
        .search_snippet_get(
            Some(email_query::Filter::text("foo")),
            ["M44200ec123de277c0c1ce69c", "M7bcbcb0b58d7729686e83d99"],
        )
        .await
        .unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "SearchSnippet/get");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["filter"], json!({ "text": "foo" }));
    assert_eq!(
        call[1]["emailIds"],
        json!(["M44200ec123de277c0c1ce69c", "M7bcbcb0b58d7729686e83d99"])
    );
    // A plain id list was used, so the `#emailIds` reference member is absent.
    assert!(call[1].get("#emailIds").is_none());
}

#[tokio::test]
async fn search_snippet_get_without_a_filter_omits_the_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "SearchSnippet/get",
        json!({
            "accountId": ACCOUNT_ID,
            "list": [
                { "emailId": "M1", "subject": "no filter", "preview": "preview text" }
            ],
            "notFound": []
        }),
    )
    .await;

    let response = client
        .search_snippet_get(None::<SnippetFilter>, ["M1"])
        .await
        .unwrap();

    assert_eq!(response.list().len(), 1);
    assert_eq!(
        response.snippet("M1").and_then(|s| s.subject()),
        Some("no filter")
    );
    // `notFound: []` deserialises to `Some(empty)`, distinct from `None`.
    assert_eq!(response.not_found(), Some(&[][..]));

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "SearchSnippet/get");
    assert!(
        call[1].get("filter").is_none(),
        "an absent filter must not be serialised as null: {}",
        call[1]
    );
    assert_eq!(call[1]["emailIds"], json!(["M1"]));
}

#[tokio::test]
async fn snippet_lookup_of_an_absent_email_id_is_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "SearchSnippet/get", rfc_snippet_get_body()).await;

    let response = client
        .search_snippet_get(Some(email_query::Filter::text("foo")), ["M1"])
        .await
        .unwrap();

    assert!(response.snippet("M-does-not-exist").is_none());
    // `snippet()` matches on `emailId`, never on the position in the list.
    assert_eq!(response.list()[0].email_id(), "M44200ec123de277c0c1ce69c");
}

// ---------------------------------------------------------------------------
// 2. notFound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_snippet_get_reports_not_found_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1: ids with no snippet go in `notFound`.
    mount_one(
        &server,
        "SearchSnippet/get",
        json!({
            "accountId": ACCOUNT_ID,
            "list": [],
            "notFound": [ "Mgone1", "Mgone2" ]
        }),
    )
    .await;

    let response = client
        .search_snippet_get(Some(email_query::Filter::text("foo")), ["Mgone1", "Mgone2"])
        .await
        .unwrap();

    assert!(response.list().is_empty());
    assert_eq!(
        response.not_found().map(<[String]>::to_vec),
        Some(vec!["Mgone1".to_string(), "Mgone2".to_string()])
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "SearchSnippet/get"
    );
}

// ---------------------------------------------------------------------------
// 3. The Request builder, including the #emailIds result reference
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_get_search_snippet_accumulates_ids_and_filter() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "SearchSnippet/get", rfc_snippet_get_body()).await;

    let mut request = client.build();
    {
        let snippet = request.get_search_snippet();
        snippet.filter(email_query::Filter::text("foo"));
        snippet.email_id("M44200ec123de277c0c1ce69c");
        snippet.email_ids(["M7bcbcb0b58d7729686e83d99", "M28d12783a0969584b6deaac0"]);
    }
    let response = request.send_get_search_snippet().await.unwrap();
    assert_eq!(response.list().len(), 3);

    // `email_id` pushes and `email_ids` extends, so the order is preserved.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "SearchSnippet/get");
    assert_eq!(
        call[1]["emailIds"],
        json!([
            "M44200ec123de277c0c1ce69c",
            "M7bcbcb0b58d7729686e83d99",
            "M28d12783a0969584b6deaac0"
        ])
    );
}

#[tokio::test]
async fn email_ids_ref_replaces_the_id_list_with_a_result_reference() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "SearchSnippet/get",
        json!({
            "accountId": ACCOUNT_ID,
            "list": [
                { "emailId": "M44200ec123de277c0c1ce69c", "subject": "refed", "preview": "p" }
            ],
            "notFound": null
        }),
    )
    .await;

    let mut request = client.build();
    {
        let snippet = request.get_search_snippet();
        snippet.filter(email_query::Filter::text("foo"));
        // Seed a literal list first: `email_ids_ref` must clear it, otherwise
        // the request would carry both `emailIds` and `#emailIds`.
        snippet.email_id("Mstale");
        snippet.email_ids_ref(ResultReference::new(Method::QueryEmail, 0, "/ids"));
    }
    request.send_get_search_snippet().await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "SearchSnippet/get");
    assert!(
        call[1].get("emailIds").is_none(),
        "`email_ids_ref` must drop the literal id list: {}",
        call[1]
    );
    assert_eq!(
        call[1]["#emailIds"],
        json!({ "resultOf": "s0", "name": "Email/query", "path": "/ids" })
    );
}

#[tokio::test]
async fn search_snippet_can_reference_the_query_that_produced_the_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §5.2: "we did an Email/query to search for any Email in the
    // account containing the word 'foo'; now, we are fetching the search
    // snippets for some of the ids that were returned".
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Email/query",
                    "s0",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "queryState": "500",
                        "canCalculateChanges": true,
                        "ids": [ "M44200ec123de277c0c1ce69c" ],
                        "position": 0,
                        "total": 1
                    }),
                ),
                inv(
                    "SearchSnippet/get",
                    "s1",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "list": [
                            {
                                "emailId": "M44200ec123de277c0c1ce69c",
                                "subject": "The <mark>Foo</mark>sball competition",
                                "preview": "...the <mark>foo</mark> ball..."
                            }
                        ],
                        "notFound": null
                    })
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request
        .query_email()
        .filter(email_query::Filter::text("foo"))
        .arguments()
        .collapse_threads(false);
    let reference = request.last_result_reference("/ids");
    request.get_search_snippet().email_ids_ref(reference);

    // `send_single` deserialises *every* response in the envelope as the same
    // `T`, so a heterogeneous Query+Snippet batch must go through `send()` and
    // be unwrapped per method instead (see the findings log).
    let mut response = request.send().await.unwrap();
    let snippet_response = response
        .pop_method_response()
        .expect("the snippet response must be present")
        .unwrap_get_search_snippet()
        .unwrap();

    assert_eq!(snippet_response.list().len(), 1);
    assert_eq!(
        snippet_response
            .snippet("M44200ec123de277c0c1ce69c")
            .and_then(|s| s.subject()),
        Some("The <mark>Foo</mark>sball competition")
    );

    let bodies = common::jmap_request_bodies(&server).await;
    let calls = &bodies[0]["methodCalls"];
    assert_eq!(calls[0][0], "Email/query");
    assert_eq!(calls[1][0], "SearchSnippet/get");
    assert_eq!(
        calls[1][1]["#emailIds"],
        json!({ "resultOf": "s0", "name": "Email/query", "path": "/ids" })
    );
    // The referenced call carries no literal ids.
    assert!(calls[1][1].get("emailIds").is_none());
}

#[tokio::test]
async fn send_single_cannot_deserialise_a_heterogeneous_batch() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Email/query",
                    "s0",
                    json!({
                        "accountId": ACCOUNT_ID,
                        "queryState": "500",
                        "canCalculateChanges": true,
                        "ids": [ "M44200ec123de277c0c1ce69c" ],
                        "position": 0,
                        "total": 1
                    }),
                ),
                inv(
                    "SearchSnippet/get",
                    "s1",
                    rfc_snippet_get_body()
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.query_email().filter(email_query::Filter::text("foo"));
    request.get_search_snippet().email_id("M44200ec123de277c0c1ce69c");

    // `send_single::<SearchSnippetGetResponse>` pops the *last* response but
    // still deserialises the whole `methodResponses` array as
    // `SingleMethodResponse<SearchSnippetGetResponse>`, so the leading
    // `Email/query` body fails first and the batch errors out even though the
    // snippet response itself is perfectly well formed.
    let err = request
        .send_single::<jmap_client::email::search_snippet::SearchSnippetGetResponse>()
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 4. Error paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_snippet_get_method_error_is_propagated() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unknownMethod")], SESSION_STATE),
    )
    .await;

    let err = client
        .search_snippet_get(None::<SnippetFilter>, ["M1"])
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "SearchSnippet/get"
    );
}

#[tokio::test]
async fn search_snippet_get_rejects_a_malformed_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // `subject` must be a string or null; a number cannot deserialise.
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "SearchSnippet/get",
                "s0",
                json!({
                    "accountId": ACCOUNT_ID,
                    "list": [ { "emailId": "M1", "subject": 42, "preview": null } ],
                    "notFound": null
                }),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    let err = client
        .search_snippet_get(None::<SnippetFilter>, ["M1"])
        .await
        .unwrap_err();
    // The mock answered with a well-formed *success* invocation whose list
    // entry carries a non-string `subject`, so the only failure this fixture can
    // produce is deserialising the payload. Pin that exact variant: accepting
    // `Error::Method` too would let a regression pass as "some error happened".
    assert!(
        matches!(err, Error::Parse(_)),
        "expected a parse failure, got {err:?}"
    );
}
