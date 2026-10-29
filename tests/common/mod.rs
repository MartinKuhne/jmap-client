//! Shared [wiremock] fixtures for the JMAP HTTP integration tests.
//!
//! This is the transport layer for the hybrid mock strategy recorded in
//! `COVERAGE_PLAN.md`: **wiremock** serves plain HTTP for the P3-P6 domain
//! tests, while the hand-rolled `mock-jmap` crate is reserved for the
//! WebSocket / EventSource legs (P7), which wiremock structurally cannot do.
//!
//! The canned payloads here are taken **verbatim from the RFC example bodies**
//! wherever the RFC has one, so the oracle is the RFC text rather than our
//! interpretation of it. Each fixture carries a `// RFC …` comment pointing at
//! the source section.
//!
//! ## API overview
//!
//! | Helper | Purpose |
//! |---|---|
//! | [`session_json`] / [`session_json_with_sieve`] | the Session resource (`GET /.well-known/jmap`) |
//! | [`mount_session`] / [`mount_session_json`] | make a Session resource answer `GET /.well-known/jmap` |
//! | [`connected_client`] / [`connected_client_json`] | mount a Session and run `Client::new()…connect()` against it |
//! | [`jmap_response`] | `200 application/json` `{"methodResponses": […], "sessionState": "…"}` |
//! | [`jmap_response_with_created_ids`] | the same, plus the optional `createdIds` member |
//! | [`problem_response`] | an RFC 7807 `application/problem+json` error body |
//! | [`problem_response_with_charset`] | the same, with `; charset=utf-8` on the media type |
//! | [`mount_jmap`] | answer `POST /jmap` with a template |
//! | [`jmap_request_bodies`] / [`single_jmap_request`] | spy on `POST /jmap` only |
//! | [`expected_basic_auth`] | the `Authorization` header `Client` builds for Basic auth |
//!
//! ## Contract facts a fixture must respect
//!
//! These come from the Findings log in `COVERAGE_PLAN.md`; breaking any of them
//! makes the client fail in a way that looks like a bug in the test:
//!
//! * **Only the problem media type is matched.** `Client::handle_error` compares
//!   the *media type* of `Content-Type`, ignoring parameters and case, so a
//!   problem body may be served bare ([`problem_response`]) or with a
//!   `; charset=utf-8` parameter ([`problem_response_with_charset`]); both must
//!   surface as `Error::Problem`.
//! * **Required response members.** A response missing any of
//!   `GetResponse.{state,list,notFound}`,
//!   `ChangesResponse.{accountId,oldState,newState,hasMoreChanges,created,updated,destroyed}`,
//!   `QueryResponse.{accountId,queryState,position,ids}`,
//!   `QueryChangesResponse.{accountId,oldQueryState,newQueryState,removed,added}`,
//!   `CopyResponse.{fromAccountId,accountId,newState}` fails to deserialise.
//! * **`sessionState` is mandatory** on every JMAP response
//!   (`Response::session_state` is a non-`Option` `String`), and it drives
//!   `Client::is_session_updated`.
//! * **`primaryAccounts` values are single strings** here. The client models
//!   them as `String[Id]` (RFC 8620 §2.1 shows arrays for some servers), and
//!   `Client::default_account_id` takes the *first* value of a `HashMap`, so
//!   every capability in the fixture maps to the *same* account to keep the
//!   choice deterministic.
//!
//! ## Hermeticity
//!
//! Every test owns its own `MockServer::start()` and mounts only what it needs.
//! An unmatched request gets wiremock's default `404`, which surfaces as
//! `jmap_client::Error::Server` — a loud failure rather than a silent hang.
//!
//! [wiremock]: https://docs.rs/wiremock

#![allow(dead_code)] // each test binary uses a subset of these helpers

use jmap_client::client::{Client, Credentials};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The single account the fixtures serve (RFC 8621 §2.6 example account id).
pub const ACCOUNT_ID: &str = "u33084183";

/// Username of [`ACCOUNT_ID`] (RFC 8620 §2.1 uses `john@example.com`; we use
/// the address the credentials in [`connected_client`] authenticate as).
pub const USERNAME: &str = "test@example.org";

/// The `state` of the Session resource (RFC 8620 §2.1 example value).
pub const SESSION_STATE: &str = "75128aab4b1b";

/// `POST` target for JMAP method calls, relative to the server root.
pub const API_PATH: &str = "/jmap";

/// `GET` target for the Session resource, relative to the server root.
/// `Client::connect` builds this as `format!("{}/.well-known/jmap", url)`.
pub const SESSION_PATH: &str = "/.well-known/jmap";

/// The capability URIs the default fixture advertises.
///
/// Deliberately **excludes** `urn:ietf:params:jmap:sieve` so that the
/// client's capability filtering (`Request::new` filters
/// `KNOWN_CAPABILITIES` against the session) is observable on the wire.
/// [`session_json_with_sieve`] adds it back.
pub const DEFAULT_CAPABILITIES: &[&str] = &[
    "urn:ietf:params:jmap:core",
    "urn:ietf:params:jmap:mail",
    "urn:ietf:params:jmap:submission",
    "urn:ietf:params:jmap:vacationresponse",
    "urn:ietf:params:jmap:websocket",
    "urn:ietf:params:jmap:principals",
    "urn:ietf:params:jmap:blob",
    "urn:ietf:params:jmap:quota",
];

/// The `urn:ietf:params:jmap:core` capability object.
///
/// Numbers are RFC 8620 §2.1's example values. Note the RFC example spells the
/// fourth entry `maxConcurrentRequest` (a typo in the published example); the
/// client requires `maxConcurrentRequests`, which is the name used in the
/// prose of §2, so that is what we emit.
fn core_capability() -> Value {
    json!({
        "maxSizeUpload": 50_000_000,
        "maxConcurrentUpload": 8,
        "maxSizeRequest": 10_000_000,
        "maxConcurrentRequests": 8,
        "maxCallsInRequest": 32,
        "maxObjectsInGet": 256,
        "maxObjectsInSet": 128,
        "collationAlgorithms": [
            "i;ascii-numeric",
            "i;ascii-casemap",
            "i;unicode-casemap"
        ]
    })
}

/// The `urn:ietf:params:jmap:mail` account capability object
/// (RFC 8621 §1.3.1).
///
/// RFC 8621 §1.3.1 says the *session-level* value of this capability is an
/// empty object and only the per-account value carries the limits. The client's
/// `Capabilities` enum is untagged and would file a session-level `{}` under
/// `Empty`, making `Session::mail_capabilities()` return `None`; we therefore
/// put the full object at session level too so the accessor is testable.
fn mail_capability() -> Value {
    json!({
        "maxMailboxesPerEmail": null,
        "maxMailboxDepth": 10,
        "maxSizeMailboxName": 100,
        "maxSizeAttachmentsPerEmail": 50_000_000,
        "emailQuerySortOptions": [
            "arrival",
            "size",
            "subject",
            "ccRecipients",
            "toRecipients",
            "threadArrival",
            "threadSubject",
            "threadSize"
        ],
        "mayCreateTopLevelMailbox": true
    })
}

/// The `urn:ietf:params:jmap:submission` capability object (RFC 8621 §1.3.2).
///
/// `submissionExtensions` is an **array of strings** here: RFC 8621 §4.3 also
/// allows the object form `{"SIZE": {…}}`, but `jmap_client` models it as
/// `Vec<String>` and the object form fails to deserialise (Findings #2).
fn submission_capability() -> Value {
    json!({
        "maxDelayedSend": 4_294_967_295u64,
        "submissionExtensions": ["SIZE", "DSN", "UTF8ONLY"]
    })
}

/// The `urn:ietf:params:jmap:websocket` capability object (RFC 8887 §5.1).
fn websocket_capability(base_url: &str) -> Value {
    json!({
        "url": format!("{}/jmap/ws", base_url.replace("http://", "ws://")),
        "supportsPush": true
    })
}

/// The `urn:ietf:params:jmap:sieve` capability object
/// (draft-ietf-jmap-sieve-12 §2).
fn sieve_capability() -> Value {
    json!({
        "implementation": "Mock JMAP Server (draft-ietf-jmap-sieve-12)",
        "maxSizeScriptName": 512,
        "maxSizeScript": 1_048_576,
        "maxNumberScripts": 64,
        "maxNumberRedirects": 20,
        "sieveExtensions": [
            "fileinto",
            "mailbox",
            "imap4flags",
            "envelope",
            "body",
            "vacation",
            "reject",
            "editheader",
            "variables",
            "subaddress",
            "relational",
            "comparator-i;ascii-numeric"
        ],
        "notificationMethods": ["mailto", "sms"],
        "externalLists": ["https://example.org/lists/{listId}"]
    })
}

/// Builds the Session resource for a server rooted at `base_url`.
///
/// Shape per RFC 8620 §2 (plus the capability blocks of RFC 8621 §1.3,
/// RFC 8887 §5.1 and draft-ietf-jmap-sieve-12 §2). When `sieve` is false the
/// `urn:ietf:params:jmap:sieve` capability is not advertised, which is what
/// makes capability filtering observable.
fn build_session(base_url: &str, sieve: bool) -> Value {
    let mut capabilities = serde_json::Map::new();
    capabilities.insert("urn:ietf:params:jmap:core".into(), core_capability());
    capabilities.insert("urn:ietf:params:jmap:mail".into(), mail_capability());
    capabilities.insert(
        "urn:ietf:params:jmap:submission".into(),
        submission_capability(),
    );
    capabilities.insert(
        "urn:ietf:params:jmap:vacationresponse".into(),
        json!({}),
    );
    capabilities.insert(
        "urn:ietf:params:jmap:websocket".into(),
        websocket_capability(base_url),
    );
    capabilities.insert("urn:ietf:params:jmap:principals".into(), json!({}));
    capabilities.insert("urn:ietf:params:jmap:blob".into(), json!({}));
    capabilities.insert("urn:ietf:params:jmap:quota".into(), json!({}));
    if sieve {
        capabilities.insert("urn:ietf:params:jmap:sieve".into(), sieve_capability());
    }

    // Every capability maps to the same account: `Client::default_account_id`
    // takes the first value of a HashMap iteration, so a single account keeps
    // the result deterministic.
    let mut primary_accounts = serde_json::Map::new();
    for capability in capabilities.keys() {
        primary_accounts.insert(capability.clone(), json!(ACCOUNT_ID));
    }

    json!({
        "capabilities": capabilities,
        "accounts": {
            ACCOUNT_ID: {
                "name": USERNAME,
                "isPersonal": true,
                "isReadOnly": false,
                "accountCapabilities": {
                    "urn:ietf:params:jmap:mail": mail_capability(),
                    "urn:ietf:params:jmap:submission": submission_capability(),
                    "urn:ietf:params:jmap:vacationresponse": {},
                    "urn:ietf:params:jmap:principals": {},
                    "urn:ietf:params:jmap:quota": {}
                }
            }
        },
        "primaryAccounts": primary_accounts,
        "username": USERNAME,
        "apiUrl": format!("{}{}", base_url, API_PATH),
        "downloadUrl": format!(
            "{}/download/{{accountId}}/{{blobId}}/{{name}}?accept={{type}}",
            base_url
        ),
        "uploadUrl": format!("{}/upload/{{accountId}}", base_url),
        "eventSourceUrl": format!(
            "{}/eventSource?types={{types}}&closeafter={{closeafter}}&ping={{ping}}",
            base_url
        ),
        "state": SESSION_STATE
    })
}

/// The Session resource for `server`, **without** the sieve capability.
///
/// `apiUrl` is `{server.uri()}/jmap`, and the download/upload/eventSource URL
/// templates follow RFC 8620 §2.
pub async fn session_json(server: &MockServer) -> Value {
    build_session(&server.uri(), false)
}

/// The Session resource for `server`, **including** the sieve capability.
///
/// Use this to show that a capability the server advertises *does* end up in
/// the request's `using` array, complementing the default fixture, which shows
/// that one it does not advertise does not.
pub async fn session_json_with_sieve(server: &MockServer) -> Value {
    build_session(&server.uri(), true)
}

/// Mounts [`session_json`] on `GET /.well-known/jmap` (200, JSON).
///
/// Mounted globally: it stays active for the lifetime of `server`, so
/// `Client::refresh_session()` works later in the same test. Call
/// [`MockServer::reset`] first to replace it.
pub async fn mount_session(server: &MockServer) {
    mount_session_json(server, session_json(server).await).await
}

/// Mounts an arbitrary Session resource on `GET /.well-known/jmap`.
pub async fn mount_session_json(server: &MockServer, session: Value) {
    mount_session_template(
        server,
        ResponseTemplate::new(200).set_body_json(session),
    )
    .await
}

/// Mounts an arbitrary response template on `GET /.well-known/jmap`.
///
/// Use this to serve an error where the Session resource should be, e.g.
/// [`problem_response`] with a 401 to exercise `Client::connect`'s error path.
pub async fn mount_session_template(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(SESSION_PATH))
        .respond_with(template)
        .mount(server)
        .await
}

/// Mounts the default Session and connects a client to `server`.
///
/// Credentials are Basic `test@example.org` / `secret`.
pub async fn connected_client(server: &MockServer) -> Client {
    connected_client_json(server, session_json(server).await).await
}

/// Mounts `session` and connects a client to `server`.
pub async fn connected_client_json(server: &MockServer, session: Value) -> Client {
    mount_session_json(server, session).await;
    Client::new()
        .credentials((USERNAME, "secret"))
        .connect(&server.uri())
        .await
        .expect("connect() must succeed against the session fixture")
}

/// A `200 application/json` JMAP `Response` object (RFC 8620 §3.4).
///
/// `method_responses` are raw Invocations: `["Name", {…}, "callId"]`, or
/// `["error", {"type": "…"}, "callId"]` for a method-level error
/// (RFC 8620 §3.6.2).
pub fn jmap_response(method_responses: Vec<Value>, session_state: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "methodResponses": method_responses,
        "sessionState": session_state,
    }))
}

/// A `200` JMAP `Response` object that also carries `createdIds`
/// (RFC 8620 §3.3/§3.4: a map of client creation id → server-assigned id).
pub fn jmap_response_with_created_ids(
    method_responses: Vec<Value>,
    created_ids: Value,
    session_state: &str,
) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "methodResponses": method_responses,
        "createdIds": created_ids,
        "sessionState": session_state,
    }))
}

/// An RFC 7807 `application/problem+json` error body (RFC 8620 §3.6.1).
///
/// The content type is served as the bare `application/problem+json`. RFC 7807
/// and RFC 8620 §3.6.1 both allow parameters on it, so
/// [`problem_response_with_charset`] serves the same body with
/// `; charset=utf-8` appended; `Client::handle_error` matches the media type
/// alone and must route both to `Error::Problem`.
pub fn problem_response(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.to_string(), "application/problem+json")
}

/// [`problem_response`], but served with
/// `Content-Type: application/problem+json; charset=utf-8`.
///
/// Same media type, one extra parameter (RFC 7231 §3.1.1.1). A client that
/// byte-compares the header value downgrades this to `Error::Server` and loses
/// the structured problem details.
pub fn problem_response_with_charset(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(body.to_string(), "application/problem+json; charset=utf-8")
}

/// Answers `POST /jmap` with `template` for the rest of the server's life.
pub async fn mount_jmap(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(API_PATH))
        .respond_with(template)
        .mount(server)
        .await
}

/// The JSON bodies of every `POST /jmap` request recorded, in arrival order.
///
/// Filtering to `POST /jmap` matters once the client has also fetched the
/// Session resource: the `GET /.well-known/jmap` would otherwise shift the
/// indices of the JMAP calls.
pub async fn jmap_request_bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() == API_PATH)
        .map(|r| {
            r.body_json::<Value>()
                .unwrap_or_else(|e| panic!("recorded JMAP request body is not JSON: {e}"))
        })
        .collect()
}

/// The body of the only `POST /jmap` recorded so far.
///
/// Panics unless exactly one JMAP call was made.
pub async fn single_jmap_request(server: &MockServer) -> Value {
    let bodies = jmap_request_bodies(server).await;
    assert_eq!(
        bodies.len(),
        1,
        "expected exactly one POST {} against this server",
        API_PATH
    );
    bodies.into_iter().next().unwrap()
}

/// The `Authorization` header value `Client` sends for Basic credentials.
///
/// Mirrors `ClientBuilder::connect`, which stores `Credentials::Basic` as
/// base64 and prefixes it with `"Basic "`.
pub fn expected_basic_auth(username: &str, password: &str) -> String {
    match Credentials::basic(username, password) {
        Credentials::Basic(encoded) => format!("Basic {}", encoded),
        Credentials::Bearer(_) => unreachable!("basic() never yields a Bearer"),
    }
}
