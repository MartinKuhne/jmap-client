//! P6 — principal-domain integration tests: **Principal**.
//!
//! Drives the `Client`/`Request` Principal helpers from
//! `src/principal/helpers.rs` against a `wiremock` HTTP server using the shared
//! fixtures in `tests/common/mod.rs`. Canned payloads are the RFC example bodies
//! where the RFC has one (the section is cited on the fixture); otherwise the
//! minimal RFC-correct shape, also cited.
//!
//! Coverage targets: `principal/helpers.rs` (every `Client::principal_*` helper
//! plus the `Request::{get,changes,query,query_changes,set}_principal`
//! builders), `principal/get.rs` (record accessors), `principal/set.rs` (patch
//! keys, alias/member property patches, `DKIM`), `principal/query.rs` (filters
//! and comparators) and `principal/mod.rs` (`Type`, `ACL`, `Property` and the
//! `Display` names each of them puts on the wire).
//!
//! ## A note on how closely the fixtures track RFC 9670
//!
//! The `Principal` type is specified in RFC 9670 §2, but this crate's model
//! predates and diverges from it in three ways that the fixtures have to
//! accommodate (both recorded in the `COVERAGE_PLAN.md` Findings log):
//!
//! * RFC 9670 spells the time zone `timeZone`; the crate serialises `timezone`,
//!   so the RFC's own spelling would be silently dropped on read.
//! * RFC 9670 makes `capabilities` a *map* of URI → object; the crate models it
//!   as a flat `String[]`.
//! * RFC 9670 has no `secret`, `aliases`, `members`, `dkim`, `quota`,
//!   `picture` or `acl`, and no `domain`/`list` principal types; those are the
//!   Stalwart extensions the helpers in this file exercise.
//!
//! So the fixtures keep the RFC's *ids, names and shape of the exchange* verbatim
//! and adjust only the member spellings the crate can actually parse.

mod common;

use common::{
    connected_client, jmap_response, jmap_response_with_created_ids, mount_jmap,
    single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::query::{Comparator, Filter};
use jmap_client::core::set::SetErrorType;
use jmap_client::principal::{query as principal_query, ACL, DKIM, Property, Type};
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

/// The sort list `Client::principal_query` / `Request::query_principal` expect,
/// so a test that supplies no sort can name the type instead of leaving it
/// ambiguous.
type PrincipalSort = Vec<Comparator<principal_query::Comparator>>;

/// The property list `Client::principal_get` expects, for the "no properties"
/// case.
type PrincipalProps = Vec<Property>;

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

/// RFC 9670 §4.1 Figure 3 — the `Joe Bloggs` entry of the `Principal/get`
/// example, with the three member spellings this crate requires adjusted (see the
/// module header).
fn joe_bloggs() -> Value {
    json!({
        "id": "P2342fnddd20",
        "type": "individual",
        "name": "Joe Bloggs",
        "description": null,
        "email": "joe.bloggs@example.com",
        "timezone": "Australia/Melbourne",
        "capabilities": ["urn:ietf:params:jmap:mail"],
        "aliases": ["joe.bloggs@alternate.example.com"],
        "secret": "hunter2",
        "quota": 1_048_576,
        "picture": "https://example.org/avatar/joe.png",
        "members": [],
        "acl": {
            "P674pp24095qo49pr": ["mayReadItems", "mayAddItems"]
        }
    })
}

/// A `Principal/get` result envelope (RFC 8620 §5.1: `state`, `list` and
/// `notFound` are all required members of the response).
fn principal_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "7b8eff5zz",
        "list": list,
        "notFound": not_found
    })
}

/// A `Principal/set` response that created one Principal under creation id `c0`
/// (RFC 8620 §5.3: `created` maps creation id → the created object, which MUST
/// carry the server-assigned `id`).
fn created_principal(object: Value) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "4242",
        "newState": "4243",
        "created": { "c0": object }
    })
}

/// A `Principal/set` response that updated `P1` to `object` (RFC 8620 §5.3).
fn updated_principal(object: Value) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "4243",
        "newState": "4244",
        "updated": { "P1": object }
    })
}

/// The `urn:ietf:params:jmap:principals` URI, as it must appear in `using`.
const PRINCIPALS_URI: &str = "urn:ietf:params:jmap:principals";

/// The method-call arguments of the only JMAP request made so far.
async fn only_call(server: &MockServer) -> Value {
    single_jmap_request(server).await["methodCalls"][0][1].clone()
}

// ---------------------------------------------------------------------------
// 1. Principal/get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn principal_get_parses_a_full_principal_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Principal/get", principal_get_body(vec![joe_bloggs()], vec![]))
        .await;

    let principal = client
        .principal_get("P2342fnddd20", None::<PrincipalProps>)
        .await
        .unwrap()
        .expect("the example record must come back");

    assert_eq!(principal.id(), Some("P2342fnddd20"));
    assert_eq!(principal.ptype(), Some(&Type::Individual));
    assert_eq!(principal.name(), Some("Joe Bloggs"));
    assert_eq!(principal.description(), None);
    assert_eq!(principal.email(), Some("joe.bloggs@example.com"));
    assert_eq!(principal.timezone(), Some("Australia/Melbourne"));
    assert_eq!(principal.secret(), Some("hunter2"));
    assert_eq!(principal.quota(), Some(1_048_576));
    assert_eq!(
        principal.picture(),
        Some("https://example.org/avatar/joe.png")
    );
    assert_eq!(
        principal.capabilities().unwrap(),
        ["urn:ietf:params:jmap:mail"]
    );
    assert_eq!(
        principal.aliases().unwrap(),
        ["joe.bloggs@alternate.example.com"]
    );
    assert!(principal.members().unwrap().is_empty());
    assert_eq!(
        principal.acl().unwrap()["P674pp24095qo49pr"],
        vec![ACL::ReadItems, ACL::AddItems]
    );

    // The method name really went on the wire, and the capability was declared.
    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "Principal/get");
    assert!(body["using"]
        .as_array()
        .unwrap()
        .contains(&json!(PRINCIPALS_URI)));
    assert_eq!(body["methodCalls"][0][1]["accountId"], ACCOUNT_ID);
}

#[tokio::test]
async fn principal_get_sends_only_the_requested_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Principal/get", principal_get_body(vec![], vec!["P404"])).await;

    let found = client
        .principal_get(
            "P404",
            Some([Property::Id, Property::Name, Property::ShareWith]),
        )
        .await
        .unwrap();

    assert!(found.is_none(), "an empty list must surface as None");

    let args = only_call(&server).await;
    assert_eq!(
        args["properties"],
        json!(["id", "name", "shareWith"]),
        "`Property` must serialise under its RFC 9670 §4 wire name"
    );
    assert_eq!(args["ids"], json!(["P404"]));
    // `properties` was given, so the member must be present at all.
    assert!(args.get("properties").is_some());
}

#[tokio::test]
async fn principal_get_omits_properties_when_none_is_given() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Principal/get", principal_get_body(vec![], vec![])).await;

    client
        .principal_get("P1", None::<PrincipalProps>)
        .await
        .unwrap();

    let args = only_call(&server).await;
    assert!(
        args.get("properties").is_none(),
        "a `None` property list must not appear on the wire: {args}"
    );
}

// ---------------------------------------------------------------------------
// 2. Principal/set — creates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn individual_create_sends_a_minimal_patch_and_returns_the_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response_with_created_ids(
            vec![inv(
                "Principal/set",
                "s0",
                created_principal(joe_bloggs()),
            )],
            json!({ "c0": "P2342fnddd20" }),
            SESSION_STATE,
        ),
    )
    .await;

    let created = client
        .individual_create("joe.bloggs@example.com", "hunter2", "Joe Bloggs")
        .await
        .unwrap();

    assert_eq!(created.id(), Some("P2342fnddd20"));
    assert_eq!(created.ptype(), Some(&Type::Individual));

    let args = only_call(&server).await;
    assert_eq!(
        args["create"]["c0"],
        json!({
            "type": "individual",
            "name": "Joe Bloggs",
            "secret": "hunter2",
            "email": "joe.bloggs@example.com"
        }),
        "the create patch must carry exactly what was set: seeded empty \
         strings/lists are skipped by `string_not_set`/`list_not_set`"
    );
    assert_eq!(args["accountId"], ACCOUNT_ID);
}

#[tokio::test]
async fn individual_create_reports_already_exists() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4242",
            "newState": "4242",
            "notCreated": {
                "c0": {
                    "type": "alreadyExists",
                    "description": "A principal with that name already exists",
                    "properties": ["name"]
                }
            }
        }),
    )
    .await;

    let err = client
        .individual_create("dup@example.com", "hunter2", "Joe Bloggs")
        .await
        .unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::AlreadyExists);
    assert_eq!(
        err.description(),
        Some("A principal with that name already exists")
    );
    assert_eq!(err.properties().unwrap(), ["name"]);
}

#[tokio::test]
async fn domain_create_sets_the_domain_type() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        created_principal(json!({
            "id": "example.org",
            "type": "domain",
            "name": "example.org"
        })),
    )
    .await;

    let domain = client.domain_create("example.org").await.unwrap();

    assert_eq!(domain.ptype(), Some(&Type::Domain));
    assert_eq!(domain.name(), Some("example.org"));
    assert_eq!(
        only_call(&server).await["create"]["c0"],
        json!({ "type": "domain", "name": "example.org" })
    );
}

#[tokio::test]
async fn domain_enable_dkim_patches_secret_and_dkim_together() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        updated_principal(json!({
            "id": "example.org",
            "type": "domain",
            "dkim": {
                "dkimSelector": "sel1",
                "dkimExpiration": 1_893_456_000
            }
        })),
    )
    .await;

    let domain = client
        .domain_enable_dkim(
            "P1",
            "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8A",
            "sel1",
            Some(1_893_456_000),
        )
        .await
        .unwrap()
        .expect("the updated record must come back");

    let dkim = domain.dkim().expect("dkim must be present");
    assert_eq!(dkim.selector(), Some("sel1"));
    assert_eq!(dkim.expiration(), Some(1_893_456_000));

    assert_eq!(
        only_call(&server).await["update"]["P1"],
        json!({
            "secret": "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8A",
            "dkim": {
                "dkimSelector": "sel1",
                "dkimExpiration": 1_893_456_000
            }
        })
    );
}

#[tokio::test]
async fn domain_enable_dkim_sends_an_explicit_null_expiration() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Principal/set", updated_principal(json!({ "id": "P1" })))
        .await;

    client
        .domain_enable_dkim("P1", "key-material", "sel1", None)
        .await
        .unwrap();

    // `DKIM`'s members carry a rename but no `skip_serializing_if`, so an
    // absent expiration rides the wire as an explicit JSON null rather than
    // being omitted (recorded as a Finding).
    assert_eq!(
        only_call(&server).await["update"]["P1"]["dkim"],
        json!({ "dkimSelector": "sel1", "dkimExpiration": null })
    );
}

#[tokio::test]
async fn list_create_carries_its_members() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        created_principal(json!({
            "id": "P-list",
            "type": "list",
            "name": "Everyone",
            "email": "everyone@example.org",
            "members": ["P1", "P2"]
        })),
    )
    .await;

    let list = client
        .list_create("everyone@example.org", "Everyone", ["P1", "P2"])
        .await
        .unwrap();

    assert_eq!(list.ptype(), Some(&Type::List));
    assert_eq!(list.members().unwrap(), ["P1", "P2"]);
    assert_eq!(
        only_call(&server).await["create"]["c0"],
        json!({
            "type": "list",
            "name": "Everyone",
            "email": "everyone@example.org",
            "members": ["P1", "P2"]
        })
    );
}

#[tokio::test]
async fn group_create_carries_its_members() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        created_principal(json!({
            "id": "P-group",
            "type": "group",
            "name": "Team",
            "email": "team@example.org",
            "members": ["P1"]
        })),
    )
    .await;

    let group = client
        .group_create("team@example.org", "Team", ["P1"])
        .await
        .unwrap();

    assert_eq!(group.ptype(), Some(&Type::Group));
    assert_eq!(group.members().unwrap(), ["P1"]);
    assert_eq!(only_call(&server).await["create"]["c0"]["type"], "group");
}

// ---------------------------------------------------------------------------
// 3. Principal/set — property patches
//
// All five scalar helpers go against one server: the mount answers every call
// with `updated: { "P1": null }`, so each helper's *request* is what differs
// and the spy log shows them side by side.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scalar_setters_patch_only_the_named_property() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Principal/set",
                "s0",
                updated_principal(Value::Null),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    // `updated: { "P1": null }` means "updated, nothing to report back", which
    // the helpers surface as `Ok(None)`.
    assert!(client
        .principal_set_name("P1", "New Name")
        .await
        .unwrap()
        .is_none());
    assert!(client
        .principal_set_secret("P1", "s3cret")
        .await
        .unwrap()
        .is_none());
    assert!(client
        .principal_set_email("P1", "new@example.org")
        .await
        .unwrap()
        .is_none());
    assert!(client
        .principal_set_timezone("P1", Some("Europe/Berlin"))
        .await
        .unwrap()
        .is_none());
    assert!(client
        .principal_set_timezone("P1", None::<String>)
        .await
        .unwrap()
        .is_none());

    let patches: Vec<Value> = common::jmap_request_bodies(&server)
        .await
        .iter()
        .map(|body| body["methodCalls"][0][1]["update"]["P1"].clone())
        .collect();

    assert_eq!(
        patches,
        vec![
            json!({ "name": "New Name" }),
            json!({ "secret": "s3cret" }),
            json!({ "email": "new@example.org" }),
            json!({ "timezone": "Europe/Berlin" }),
            // An explicit `None` rides the wire as JSON null: that *is* the
            // "unset this property" patch (RFC 8620 §5.3).
            json!({ "timezone": null }),
        ]
    );
}

#[tokio::test]
async fn collection_setters_replace_wholesale_or_null_out() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv(
                "Principal/set",
                "s0",
                updated_principal(Value::Null),
            )],
            SESSION_STATE,
        ),
    )
    .await;

    client
        .principal_set_members("P1", Some(vec!["P2", "P3"]))
        .await
        .unwrap();
    client
        .principal_set_aliases("P1", Some(vec!["alias@example.org"]))
        .await
        .unwrap();
    client
        .principal_set_capabilities("P1", Some(vec!["urn:ietf:params:jmap:mail"]))
        .await
        .unwrap();
    client
        .principal_set_members("P1", None::<Vec<String>>)
        .await
        .unwrap();

    let patches: Vec<Value> = common::jmap_request_bodies(&server)
        .await
        .iter()
        .map(|body| body["methodCalls"][0][1]["update"]["P1"].clone())
        .collect();

    assert_eq!(
        patches,
        vec![
            json!({ "members": ["P2", "P3"] }),
            json!({ "aliases": ["alias@example.org"] }),
            json!({ "capabilities": ["urn:ietf:params:jmap:mail"] }),
            json!({ "members": null }),
        ]
    );
}

#[tokio::test]
async fn principal_set_reports_not_updated() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4243",
            "newState": "4243",
            "notUpdated": {
                "P1": {
                    "type": "forbidden",
                    "description": "You may not rename another principal",
                    "properties": ["name"]
                }
            }
        }),
    )
    .await;

    let err = client
        .principal_set_name("P1", "Mallory")
        .await
        .unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::Forbidden);
    assert_eq!(err.properties().unwrap(), ["name"]);
    assert!(
        err.to_string()
            .contains("You may not rename another principal"),
        "Display must carry the description: {err}"
    );
}

// ---------------------------------------------------------------------------
// 4. Principal/set — destroy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn principal_destroy_sends_the_destroy_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4244",
            "newState": "4245",
            "destroyed": ["P1"]
        }),
    )
    .await;

    client.principal_destroy("P1").await.unwrap();

    let args = only_call(&server).await;
    assert_eq!(args["destroy"], json!(["P1"]));
    assert!(
        args.get("update").is_none() && args.get("create").is_none(),
        "a destroy-only set must not carry create/update: {args}"
    );
}

#[tokio::test]
async fn principal_destroy_reports_not_destroyed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4244",
            "newState": "4244",
            "notDestroyed": {
                "P1": { "type": "notFound" }
            }
        }),
    )
    .await;

    let err = client.principal_destroy("P1").await.unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::NotFound);
}

#[tokio::test]
async fn a_destroy_the_server_never_mentions_is_an_internal_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // Neither `destroyed` nor `notDestroyed` names P1: the helper cannot prove
    // anything happened, and says so rather than pretending success.
    mount_one(
        &server,
        "Principal/set",
        json!({ "accountId": ACCOUNT_ID, "oldState": "1", "newState": "1" }),
    )
    .await;

    let err = client.principal_destroy("P1").await.unwrap_err();
    assert!(
        matches!(err, Error::Internal(_)),
        "an unreported id must be an Internal error, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. Principal/query
// ---------------------------------------------------------------------------

/// A `Principal/query` result envelope (RFC 8620 §5.5).
fn query_body(ids: Vec<&str>, total: usize) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "queryState": "507f1f77bcf86cd799439011",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ids,
        "total": total
    })
}

#[tokio::test]
async fn principal_query_sends_an_and_filter_and_a_sort() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/query",
        query_body(vec!["P2342fnddd20"], 1),
    )
    .await;

    let filter: Filter<principal_query::Filter> = Filter::and([
        Filter::from(principal_query::Filter::name("bloggs")),
        Filter::from(principal_query::Filter::ptype(Type::Individual)),
    ]);

    let response = client
        .principal_query(
            Some(filter),
            Some([principal_query::Comparator::name().descending()]),
        )
        .await
        .unwrap();

    assert_eq!(response.ids(), ["P2342fnddd20"]);
    assert_eq!(response.total(), Some(1));
    assert_eq!(response.query_state(), "507f1f77bcf86cd799439011");
    assert!(response.can_calculate_changes());

    let args = only_call(&server).await;
    assert_eq!(
        args["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "name": "bloggs" },
                { "type": "individual" }
            ]
        })
    );
    assert_eq!(
        args["sort"],
        json!([{ "property": "name", "isAscending": false }])
    );
}

#[tokio::test]
async fn principal_query_sends_the_window_arguments() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/query",
        json!({
            "accountId": ACCOUNT_ID,
            "queryState": "qs-2",
            "canCalculateChanges": false,
            "position": 10,
            "ids": ["P9", "P10"],
            "total": 42,
            "limit": 2
        }),
    )
    .await;

    let mut request = client.build();
    request
        .query_principal()
        .filter(principal_query::Filter::text("example"))
        .sort([principal_query::Comparator::email()])
        .anchor("P5")
        .anchor_offset(5)
        .limit(2)
        .calculate_total(true);
    let response = request.send_query_principal().await.unwrap();

    assert_eq!(response.position(), 10);
    assert_eq!(response.limit(), Some(2));
    assert_eq!(response.total(), Some(42));
    assert_eq!(response.ids().len(), 2);

    let args = only_call(&server).await;
    assert_eq!(args["anchor"], "P5");
    assert_eq!(args["anchorOffset"], 5);
    assert_eq!(args["limit"], 2);
    assert_eq!(args["calculateTotal"], true);
    assert_eq!(args["filter"], json!({ "text": "example" }));
    assert_eq!(
        args["sort"],
        json!([{ "property": "email", "isAscending": true }])
    );
}

#[tokio::test]
async fn principal_query_with_no_filter_or_sort_omits_both_members() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Principal/query", query_body(vec!["P1"], 1)).await;

    client
        .principal_query(None::<Filter<principal_query::Filter>>, None::<PrincipalSort>)
        .await
        .unwrap();

    let args = only_call(&server).await;
    assert!(args.get("filter").is_none(), "{args}");
    assert!(args.get("sort").is_none(), "{args}");
}

#[tokio::test]
async fn every_principal_filter_variant_reaches_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![inv("Principal/query", "s0", query_body(vec![], 0))],
            SESSION_STATE,
        ),
    )
    .await;

    let filters = [
        (
            principal_query::Filter::email("joe@example.com"),
            json!({ "email": "joe@example.com" }),
        ),
        (
            principal_query::Filter::name("Joe"),
            json!({ "name": "Joe" }),
        ),
        (
            principal_query::Filter::domain_name("example.org"),
            json!({ "domainName": "example.org" }),
        ),
        (
            principal_query::Filter::text("bloggs"),
            json!({ "text": "bloggs" }),
        ),
        (
            principal_query::Filter::ptype(Type::Group),
            json!({ "type": "group" }),
        ),
        (
            principal_query::Filter::timezone("Australia/Melbourne"),
            json!({ "timezone": "Australia/Melbourne" }),
        ),
        (
            principal_query::Filter::members("P1"),
            json!({ "members": "P1" }),
        ),
        (
            principal_query::Filter::quota_lower_than(1024),
            json!({ "quotaLowerThan": 1024 }),
        ),
        (
            principal_query::Filter::quota_greater_than(4096),
            json!({ "quotaGreaterThan": 4096 }),
        ),
    ];

    let expected: Vec<Value> = filters.iter().map(|(_, wire)| wire.clone()).collect();

    for (filter, _) in filters {
        client
            .principal_query(Some(filter), None::<PrincipalSort>)
            .await
            .unwrap();
    }

    let sent: Vec<Value> = common::jmap_request_bodies(&server)
        .await
        .iter()
        .map(|body| body["methodCalls"][0][1]["filter"].clone())
        .collect();

    assert_eq!(sent, expected);
}

// ---------------------------------------------------------------------------
// 6. Principal/changes and Principal/queryChanges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn principal_changes_reports_created_updated_and_destroyed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.2 — every member of the /changes response is required.
    mount_one(
        &server,
        "Principal/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4242",
            "newState": "4250",
            "hasMoreChanges": false,
            "created": ["P2342fnddd20"],
            "updated": ["P1"],
            "destroyed": ["P0"]
        }),
    )
    .await;

    let changes = client.principal_changes("4242", 128).await.unwrap();

    assert_eq!(changes.account_id(), ACCOUNT_ID);
    assert_eq!(changes.old_state(), "4242");
    assert_eq!(changes.new_state(), "4250");
    assert!(!changes.has_more_changes());
    assert_eq!(changes.created(), ["P2342fnddd20"]);
    assert_eq!(changes.updated(), ["P1"]);
    assert_eq!(changes.destroyed(), ["P0"]);

    let args = only_call(&server).await;
    assert_eq!(args["sinceState"], "4242");
    assert_eq!(args["maxChanges"], 128);
    assert_eq!(single_jmap_request(&server).await["methodCalls"][0][0], "Principal/changes");
}

#[tokio::test]
async fn principal_query_changes_via_the_request_builder() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.6 — the /queryChanges response shape.
    mount_one(
        &server,
        "Principal/queryChanges",
        json!({
            "accountId": ACCOUNT_ID,
            "oldQueryState": "qs-1",
            "newQueryState": "qs-2",
            "total": 3,
            "removed": ["P0"],
            "added": [
                { "id": "P1", "index": 0 },
                { "id": "P2", "index": 1 }
            ]
        }),
    )
    .await;

    let mut request = client.build();
    request
        .query_principal_changes("qs-1")
        .filter(principal_query::Filter::name("a"))
        .sort([principal_query::Comparator::name()])
        .max_changes(16)
        .up_to_id("P2")
        .calculate_total(true);
    let changes = request.send_query_principal_changes().await.unwrap();

    assert_eq!(changes.old_query_state(), "qs-1");
    assert_eq!(changes.new_query_state(), "qs-2");
    assert_eq!(changes.total(), Some(3));
    assert_eq!(changes.removed(), ["P0"]);
    assert_eq!(changes.added().len(), 2);
    assert_eq!(changes.added()[0].id(), "P1");
    assert_eq!(changes.added()[1].index(), 1);

    let args = only_call(&server).await;
    assert_eq!(args["sinceQueryState"], "qs-1");
    assert_eq!(args["maxChanges"], 16);
    assert_eq!(args["upToId"], "P2");
    assert_eq!(args["calculateTotal"], true);
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Principal/queryChanges"
    );
}

// ---------------------------------------------------------------------------
// 7. Method-level errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_method_level_error_short_circuits_the_helper() {
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

    let err = client
        .principal_get("P1", None::<PrincipalProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "an `error` Invocation must surface as Error::Method, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 8. Wire names of the enums
// ---------------------------------------------------------------------------

#[test]
fn principal_enum_wire_names() {
    // RFC 9670 §2 — the `type` values.
    for (value, wire) in [
        (Type::Individual, "individual"),
        (Type::Group, "group"),
        (Type::Resource, "resource"),
        (Type::Location, "location"),
        (Type::Domain, "domain"),
        (Type::List, "list"),
        (Type::Other, "other"),
    ] {
        assert_eq!(serde_json::to_value(&value).unwrap(), json!(wire));
        assert_eq!(serde_json::from_value::<Type>(json!(wire)).unwrap(), value);
    }

    // `Property` — the names `Principal/get`'s `properties` argument carries.
    // `shareWith` is the RFC 9670 §4 shared-data property; the rest are the
    // crate's own members.
    for (property, wire) in [
        (Property::Id, "id"),
        (Property::Type, "type"),
        (Property::Name, "name"),
        (Property::Description, "description"),
        (Property::Email, "email"),
        (Property::Timezone, "timezone"),
        (Property::Capabilities, "capabilities"),
        (Property::Aliases, "aliases"),
        (Property::Secret, "secret"),
        (Property::DKIM, "dkim"),
        (Property::Quota, "quota"),
        (Property::Picture, "picture"),
        (Property::Members, "members"),
        (Property::ShareWith, "shareWith"),
    ] {
        assert_eq!(serde_json::to_value(&property).unwrap(), json!(wire));
        assert_eq!(property.to_string(), wire);
        assert_eq!(serde_json::from_value::<Property>(json!(wire)).unwrap(), property);
    }

    // `ACL` — note the *serde* name is `mayXyz` while `Display` drops the `may`
    // prefix; the two are deliberately different (recorded as a Finding).
    for (acl, serde_name, display) in [
        (ACL::Rename, "mayRename", "rename"),
        (ACL::Delete, "mayDelete", "delete"),
        (ACL::ReadItems, "mayReadItems", "readItems"),
        (ACL::AddItems, "mayAddItems", "addItems"),
        (ACL::SetKeywords, "maySetKeywords", "setKeywords"),
        (ACL::RemoveItems, "mayRemoveItems", "removeItems"),
        (ACL::CreateChild, "mayCreateChild", "createChild"),
        (ACL::Administer, "mayShare", "administer"),
        (ACL::Submit, "maySubmit", "submit"),
        (ACL::SetSeen, "maySetSeen", "setSeen"),
    ] {
        assert_eq!(serde_json::to_value(&acl).unwrap(), json!(serde_name));
        assert_eq!(serde_json::from_value::<ACL>(json!(serde_name)).unwrap(), acl);
        assert_eq!(acl.to_string(), display);
    }

    // `DKIM` — both members always ride the wire; `None` becomes an explicit
    // JSON null because the struct has no `skip_serializing_if`.
    assert_eq!(
        serde_json::to_value(DKIM::new(None::<String>, None)).unwrap(),
        json!({ "dkimSelector": null, "dkimExpiration": null })
    );
    assert_eq!(
        serde_json::to_value(DKIM::new(Some("sel1"), None)).unwrap(),
        json!({ "dkimSelector": "sel1", "dkimExpiration": null })
    );
    assert_eq!(
        serde_json::to_value(DKIM::new(Some("sel1"), Some(1_893_456_000))).unwrap(),
        json!({ "dkimSelector": "sel1", "dkimExpiration": 1_893_456_000 })
    );
    let round_trip: DKIM =
        serde_json::from_value(json!({ "dkimSelector": "sel1", "dkimExpiration": 7 })).unwrap();
    assert_eq!(round_trip.selector(), Some("sel1"));
    assert_eq!(round_trip.expiration(), Some(7));
}

#[test]
fn the_acl_map_round_trips_under_its_own_member_name() {
    // The crate serialises the sharing map as `acl`, not as the RFC 9670 §4
    // `shareWith` name; `Property::ShareWith` still spells `shareWith`, so the
    // two must not be confused (recorded as a Finding).
    let wire = json!({
        "id": "P1",
        "type": "individual",
        "acl": { "P2": ["mayReadItems"] }
    });
    let principal: jmap_client::principal::Principal<jmap_client::Get> =
        serde_json::from_value(wire).unwrap();
    assert_eq!(principal.acl().unwrap()["P2"], vec![ACL::ReadItems]);
    assert_eq!(
        serde_json::to_value(&principal).unwrap()["acl"]["P2"],
        json!(["mayReadItems"])
    );
}

// ---------------------------------------------------------------------------
// The `Request::send_*` convenience wrappers
//
// `Request::send_{get,changes,set}_principal` are each a thin
// `self.send_single().await` (src/principal/helpers.rs). The
// `Client::principal_*` helpers call `send_single` directly, so nothing else
// in the crate reaches these wrappers by name. Each test builds the call
// through the typed builder, sends it through the wrapper, asserts the parsed
// result, and spies on the wire so it cannot pass on an empty `methodCalls`
// array (Finding #31).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_get_principal_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/get",
        principal_get_body(vec![joe_bloggs()], vec![]),
    )
    .await;

    let mut request = client.build();
    request.get_principal().ids(["P1"]);
    let mut response = request.send_get_principal().await.unwrap();

    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    assert_eq!(response.not_found(), [] as [&str; 0]);
    let principal = response.take_list().pop().expect("one record");
    assert_eq!(principal.id(), Some("P2342fnddd20"));
    assert_eq!(principal.name(), Some("Joe Bloggs"));

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "Principal/get",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    assert_eq!(body["methodCalls"][0][1]["ids"], json!(["P1"]));
}

#[tokio::test]
async fn send_changes_principal_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 9670 §4.5 / RFC 8620 §5.2 — every member required (Finding #8).
    mount_one(
        &server,
        "Principal/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "4242",
            "newState": "4250",
            "hasMoreChanges": false,
            "created": ["P2"],
            "updated": ["P1"],
            "destroyed": ["P0"]
        }),
    )
    .await;

    let mut request = client.build();
    request.changes_principal("4242").max_changes(32);
    let response = request.send_changes_principal().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "4242");
    assert_eq!(response.new_state(), "4250");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["P2"]);
    assert_eq!(response.updated(), ["P1"]);
    assert_eq!(response.destroyed(), ["P0"]);

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "Principal/changes");
    assert_eq!(body["methodCalls"][0][1]["sinceState"], "4242");
    assert_eq!(body["methodCalls"][0][1]["maxChanges"], 32);
}

#[tokio::test]
async fn send_set_principal_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Principal/set",
        updated_principal(json!({ "name": "Renamed" })),
    )
    .await;

    let mut request = client.build();
    request.set_principal().update("P1").name("Renamed");
    let mut response = request.send_set_principal().await.unwrap();

    assert_eq!(response.account_id(), Some(ACCOUNT_ID));
    let principal = response.updated("P1").unwrap().expect("updated record");
    assert_eq!(principal.name(), Some("Renamed"));

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "Principal/set");
    assert_eq!(
        body["methodCalls"][0][1]["update"]["P1"],
        json!({ "name": "Renamed" })
    );
}

#[tokio::test]
async fn send_set_principal_surfaces_a_method_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "unknownMethod")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request.set_principal().update("P1").name("Renamed");
    let err = request.send_set_principal().await.unwrap_err();

    assert!(
        matches!(err, Error::Method(_)),
        "the wrapper must propagate an `error` Invocation as Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Principal/set"
    );
}
