//! P4 — mail-domain integration tests: **Mailboxes**.
//!
//! Drives the `Client`/`Request` Mailbox helpers from `src/mailbox/helpers.rs`
//! against a `wiremock` HTTP server using the shared fixtures in
//! `tests/common/mod.rs`. Every canned payload is the RFC example body where the
//! RFC has one (the section is cited on the fixture); otherwise it is the minimal
//! RFC-correct shape, also cited.
//!
//! Coverage targets: `mailbox/helpers.rs` (every `Client::mailbox_*` helper plus
//! the `Request::{get,changes,query,query_changes,set}_mailbox` builders),
//! `mailbox/get.rs` (record + `MailboxRights` accessors), `mailbox/set.rs`
//! (patch keys, `Role` handling, `onDestroyRemoveEmails`), `mailbox/query.rs`
//! (filters, comparators, tree arguments) and `mailbox/mod.rs` (`Role`
//! (de)serialisation, `Property`).

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use common::{
    connected_client, jmap_response, mount_jmap, single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::query::{Comparator, Filter};
use jmap_client::core::set::SetErrorType;
use jmap_client::mailbox::{query as mailbox_query, Property, Role};
use jmap_client::principal::ACL;
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::MockServer;

/// The sort list `Client::mailbox_query` / `Request::query_mailbox` expect, so a
/// test that supplies no sort can name the type instead of leaving it ambiguous.
type MailboxSort = Vec<Comparator<mailbox_query::Comparator>>;

/// The property list `Client::mailbox_get` expects, for the "no properties" case.
type MailboxProps = Vec<Property>;

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

/// RFC 8621 §2.6 — the `Inbox` entry of the `Mailbox/get` example, verbatim.
fn rfc_inbox() -> Value {
    json!({
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
    })
}

/// A `Mailbox/get` result envelope (RFC 8620 §5.1: `state`, `list`, `notFound`
/// are all required members of the response).
fn mailbox_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "78540",
        "list": list,
        "notFound": not_found
    })
}

/// The `Mailbox/set` response for the RFC 8621 §2.6 rename example: the RFC
/// spells the updated entry `null`, which the client models as
/// `Option<Mailbox<Get>>` and surfaces as `Ok(None)`.
fn renamed_body() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "78542",
        "newState": "78549",
        "updated": {
            "MB674cc24095db49ce": null
        }
    })
}

/// A `Mailbox/set` response that created one Mailbox under creation id `c0`
/// (RFC 8620 §5.3: `created` maps creation id → the created object, which MUST
/// carry the server-assigned `id`).
fn created_mailbox_body() -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "78542",
        "newState": "78549",
        "created": {
            "c0": {
                "id": "MBdfe8ab4d4e0c4f0",
                "name": "Maybe important mail",
                "parentId": "MB674cc24095db49ce",
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

/// A `Mailbox/query` result envelope (RFC 8620 §5.5: `queryState`, `position`
/// and `ids` are required).
fn query_body(ids: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "queryState": "e35e9facf117-780615:0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ids,
        "total": ids.len()
    })
}

// ---------------------------------------------------------------------------
// 1. Mailbox/get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_get_returns_the_rfc_inbox_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/get", mailbox_get_body(vec![rfc_inbox()], vec![])).await;

    let mailbox = client
        .mailbox_get("MB23cfa8094c0f41e6", None::<MailboxProps>)
        .await
        .unwrap()
        .expect("the Inbox record must come back");

    assert_eq!(mailbox.id(), Some("MB23cfa8094c0f41e6"));
    assert_eq!(mailbox.name(), Some("Inbox"));
    assert_eq!(mailbox.parent_id(), None);
    assert_eq!(mailbox.role(), Role::Inbox);
    assert_eq!(mailbox.sort_order(), 10);
    assert_eq!(mailbox.total_emails(), 16307);
    assert_eq!(mailbox.unread_emails(), 13905);
    assert_eq!(mailbox.total_threads(), 5833);
    assert_eq!(mailbox.unread_threads(), 5128);
    assert!(mailbox.is_subscribed());
    // No `shareWith` member was sent, so the ACL map is absent.
    assert!(mailbox.acl().is_none());

    let rights = mailbox.my_rights().expect("myRights must be present");
    assert!(rights.may_read_items());
    assert!(rights.may_add_items());
    assert!(rights.may_remove_items());
    assert!(rights.may_set_seen());
    assert!(rights.may_set_keywords());
    assert!(rights.may_create_child());
    assert!(!rights.may_rename());
    assert!(!rights.may_delete());
    assert!(rights.may_submit());

    // --- what actually went over the wire (RFC 8620 §3.3/§5.1) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/get");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["ids"], json!(["MB23cfa8094c0f41e6"]));
    // `properties` was not set, so it must be absent (= "all properties").
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn mailbox_get_with_an_explicit_property_list_sends_every_name() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/get", mailbox_get_body(vec![], vec!["MB23cfa8094c0f41e6"])).await;

    let mailbox = client
        .mailbox_get(
            "MB23cfa8094c0f41e6",
            Some([
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
            ]),
        )
        .await
        .unwrap();
    assert!(mailbox.is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["properties"],
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
}

#[tokio::test]
async fn mailbox_get_of_an_unknown_id_returns_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.1: ids that do not exist go in `notFound`, not into an error.
    mount_one(
        &server,
        "Mailbox/get",
        mailbox_get_body(vec![], vec!["MB0000000000000000"]),
    )
    .await;

    assert!(client
        .mailbox_get("MB0000000000000000", Some([Property::Id]))
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["properties"],
        json!(["id"])
    );
}

#[tokio::test]
async fn mailbox_role_deserialises_case_insensitively_and_keeps_unknown_roles() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.1 registers the seven standard roles in lower case; the
    // client lower-cases on the way in and keeps anything else as `Role::Other`.
    mount_one(
        &server,
        "Mailbox/get",
        mailbox_get_body(
            vec![
                json!({ "id": "m1", "name": "A", "role": "INBOX" }),
                json!({ "id": "m2", "name": "B", "role": "Sent" }),
                json!({ "id": "m3", "name": "C", "role": "FoO" }),
                json!({ "id": "m4", "name": "D", "role": null }),
                json!({ "id": "m5", "name": "E" }),
            ],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request.get_mailbox();
    let mut response = request.send_get_mailbox().await.unwrap();
    let list = response.take_list();

    assert_eq!(list[0].role(), Role::Inbox);
    assert_eq!(list[1].role(), Role::Sent);
    assert_eq!(list[2].role(), Role::Other("foo".to_string()));
    assert_eq!(list[3].role(), Role::None);
    assert_eq!(list[4].role(), Role::None);
    // A mailbox fetched without `myRights` has none, and the accessor says so.
    assert!(list[0].my_rights().is_none());
}

#[tokio::test]
async fn mailbox_rights_project_to_an_acl_list_in_declaration_order() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/get", mailbox_get_body(vec![rfc_inbox()], vec![])).await;

    let mailbox = client
        .mailbox_get("MB23cfa8094c0f41e6", Some([Property::MyRights]))
        .await
        .unwrap()
        .unwrap();

    // `acl_list()` walks the nine rights in a fixed order and keeps the granted
    // ones; `mayRename`/`mayDelete` are false in the RFC record, so they drop out.
    assert_eq!(
        mailbox.my_rights().unwrap().acl_list(),
        vec![
            ACL::ReadItems,
            ACL::AddItems,
            ACL::RemoveItems,
            ACL::SetSeen,
            ACL::SetKeywords,
            ACL::CreateChild,
            ACL::Submit,
        ]
    );
}

#[tokio::test]
async fn mailbox_property_display_and_is_count() {
    assert_eq!(Property::Id.to_string(), "id");
    assert_eq!(Property::ParentId.to_string(), "parentId");
    assert_eq!(Property::SortOrder.to_string(), "sortOrder");
    assert_eq!(Property::MyRights.to_string(), "myRights");
    assert_eq!(Property::ShareWith.to_string(), "shareWith");

    // RFC 8621 §2.1: the four counter properties, and only those.
    assert!(Property::TotalEmails.is_count());
    assert!(Property::UnreadEmails.is_count());
    assert!(Property::TotalThreads.is_count());
    assert!(Property::UnreadThreads.is_count());
    assert!(!Property::Name.is_count());
    assert!(!Property::IsSubscribed.is_count());
}

// ---------------------------------------------------------------------------
// 2. Mailbox/set — create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_create_sends_creation_id_c0_and_returns_the_created_mailbox() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/set", created_mailbox_body()).await;

    let mailbox = client
        .mailbox_create(
            "Maybe important mail",
            Some("MB674cc24095db49ce"),
            Role::None,
        )
        .await
        .unwrap();

    assert_eq!(mailbox.id(), Some("MBdfe8ab4d4e0c4f0"));
    assert_eq!(mailbox.name(), Some("Maybe important mail"));
    assert_eq!(mailbox.parent_id(), Some("MB674cc24095db49ce"));

    // --- wire ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/set");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    // `SetRequest::create()` numbers creation ids from 0 → "c0".
    //
    // `Role::None` does **not** make the `role` member disappear: the setter
    // assigns `None`, and `role_not_set` only suppresses the `Some(Role::None)`
    // that `Mailbox::new()` seeds, so an explicitly-cleared role rides along as
    // JSON `null` (see the Findings log).
    assert_eq!(
        call[1]["create"],
        json!({
            "c0": {
                "name": "Maybe important mail",
                "parentId": "MB674cc24095db49ce",
                "role": null
            }
        })
    );
}

#[tokio::test]
async fn mailbox_create_with_a_role_serialises_the_canonical_lower_case_name() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/set", created_mailbox_body()).await;

    client
        .mailbox_create("Drafts", None::<&str>, Role::Drafts)
        .await
        .unwrap();

    // RFC 8621 §2.1 registers the standard roles in lower case and `Serialize`
    // emits exactly that. A `None` parent serialises as `parentId: null`, i.e.
    // "top level", not as an absent member.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["create"]["c0"],
        json!({ "name": "Drafts", "parentId": null, "role": "drafts" })
    );
}

// ---------------------------------------------------------------------------
// 3. Mailbox/set — update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_rename_sends_a_name_patch_for_the_existing_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.6: the rename half of the set example.
    mount_one(&server, "Mailbox/set", renamed_body()).await;

    let mailbox = client
        .mailbox_rename("MB674cc24095db49ce", "Maybe important mail")
        .await
        .unwrap();
    // The RFC's `updated` entry is `null`, so the helper reports "updated, no
    // data returned" as `Ok(None)`.
    assert!(mailbox.is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/set");
    assert_eq!(
        call[1]["update"],
        json!({ "MB674cc24095db49ce": { "name": "Maybe important mail" } })
    );
    assert!(call[1].get("create").is_none());
    assert!(call[1].get("destroy").is_none());
}

#[tokio::test]
async fn mailbox_move_replaces_the_parent_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/set", renamed_body()).await;

    client
        .mailbox_move("MB674cc24095db49ce", Some("MB23cfa8094c0f41e6"))
        .await
        .unwrap();

    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({ "MB674cc24095db49ce": { "parentId": "MB23cfa8094c0f41e6" } })
    );
}

#[tokio::test]
async fn mailbox_move_to_the_top_level_patches_parent_id_to_null() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/set", renamed_body()).await;

    client
        .mailbox_move("MB674cc24095db49ce", None::<&str>)
        .await
        .unwrap();

    // RFC 8621 §2.1: a top-level Mailbox has `parentId: null`. The client
    // clears the field, which serialises as an explicit JSON null — the patch
    // form that actually un-parents the Mailbox.
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({ "MB674cc24095db49ce": { "parentId": null } })
    );
}

#[tokio::test]
async fn mailbox_update_role_and_sort_order_and_subscription_each_patch_one_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    mount_one(&server, "Mailbox/set", renamed_body()).await;
    client
        .mailbox_update_role("MB674cc24095db49ce", Role::Important)
        .await
        .unwrap();
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({ "MB674cc24095db49ce": { "role": "important" } })
    );

    server.reset().await;
    mount_one(&server, "Mailbox/set", renamed_body()).await;
    client
        .mailbox_update_sort_order("MB674cc24095db49ce", 42)
        .await
        .unwrap();
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({ "MB674cc24095db49ce": { "sortOrder": 42 } })
    );

    server.reset().await;
    mount_one(&server, "Mailbox/set", renamed_body()).await;
    client
        .mailbox_subscribe("MB674cc24095db49ce", true)
        .await
        .unwrap();
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({ "MB674cc24095db49ce": { "isSubscribed": true } })
    );
}

#[tokio::test]
async fn mailbox_update_acl_sends_a_share_with_patch_keyed_by_principal() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/set", renamed_body()).await;

    client
        .mailbox_update_acl(
            "MB674cc24095db49ce",
            "P28923980293",
            [ACL::ReadItems, ACL::AddItems, ACL::Submit],
        )
        .await
        .unwrap();

    // RFC 8621 §2.1 shares are `shareWith: Id[ACL]`; the client patches a single
    // grantee with the `shareWith/<id>` dotted key (RFC 8620 §5.3 patch keys).
    let update = single_jmap_request(&server).await["methodCalls"][0][1]["update"].clone();
    assert_eq!(
        update["MB674cc24095db49ce"]["shareWith/P28923980293"],
        json!({ "mayReadItems": true, "mayAddItems": true, "maySubmit": true })
    );
    // The dotted patch key replaces the whole map member, so the plain
    // `shareWith` must not also be on the wire.
    assert!(update["MB674cc24095db49ce"].get("shareWith").is_none());
}

#[tokio::test]
async fn mailbox_set_can_create_and_update_and_destroy_in_one_call() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Mailbox/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78542",
            "newState": "78550",
            "created": { "c0": { "id": "MBnewmailbox000001", "name": "Projects" } },
            "updated": { "MB674cc24095db49ce": null },
            "destroyed": [ "MB23cfa8094c0f41e6" ]
        }),
    )
    .await;

    let mut request = client.build();
    {
        let set = request.set_mailbox();
        set.create().name("Projects");
        set.update("MB674cc24095db49ce").sort_order(7);
        set.destroy(["MB23cfa8094c0f41e6"]);
        set.if_in_state("78542");
    }
    let mut response = request.send_set_mailbox().await.unwrap();

    assert_eq!(response.old_state(), Some("78542"));
    assert_eq!(response.new_state(), "78550");
    assert!(response.has_created());
    assert!(response.has_updated());
    assert!(response.has_destroyed());
    assert_eq!(
        response.created("c0").unwrap().name(),
        Some("Projects")
    );
    assert_eq!(
        response.take_destroyed_ids().unwrap(),
        vec!["MB23cfa8094c0f41e6"]
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["ifInState"], "78542");
    assert_eq!(call[1]["create"]["c0"]["name"], "Projects");
    assert_eq!(call[1]["update"]["MB674cc24095db49ce"]["sortOrder"], 7);
    assert_eq!(call[1]["destroy"], json!(["MB23cfa8094c0f41e6"]));
}

// ---------------------------------------------------------------------------
// 4. Mailbox/set — destroy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_destroy_with_delete_emails_sets_on_destroy_remove_emails() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Mailbox/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78549",
            "newState": "78551",
            "destroyed": [ "MB674cc24095db49ce" ]
        }),
    )
    .await;

    client
        .mailbox_destroy("MB674cc24095db49ce", true)
        .await
        .unwrap();

    // RFC 8621 §2.5: `onDestroyRemoveEmails` is the Mailbox/set extension that
    // permits destroying a non-empty Mailbox.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/set");
    assert_eq!(call[1]["destroy"], json!(["MB674cc24095db49ce"]));
    assert_eq!(call[1]["onDestroyRemoveEmails"], true);
}

#[tokio::test]
async fn mailbox_destroy_refuses_a_mailbox_that_still_has_email() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.5: with `onDestroyRemoveEmails` false the server rejects a
    // non-empty Mailbox with the `mailboxHasEmail` SetError.
    mount_one(
        &server,
        "Mailbox/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78549",
            "newState": "78549",
            "notDestroyed": {
                "MB674cc24095db49ce": {
                    "type": "mailboxHasEmail",
                    "description": "Mailbox still contains emails"
                }
            }
        }),
    )
    .await;

    let err = client
        .mailbox_destroy("MB674cc24095db49ce", false)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::MailboxHasEmail);
    assert_eq!(set_err.description(), Some("Mailbox still contains emails"));
    // RFC 8621 §2.5: the default is `false`, so the guard is on the wire even
    // though the caller only passed the flag positionally.
    assert_eq!(set_err.properties(), None);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["onDestroyRemoveEmails"], false);
}

#[tokio::test]
async fn mailbox_destroy_reports_a_child_mailbox_as_a_set_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.5: `mailboxHasChild` — the parent cannot go before its kids.
    mount_one(
        &server,
        "Mailbox/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78549",
            "newState": "78549",
            "notDestroyed": {
                "MB674cc24095db49ce": { "type": "mailboxHasChild" }
            }
        }),
    )
    .await;

    let mut request = client.build();
    request.set_mailbox().destroy(["MB674cc24095db49ce"]);
    let mut response = request.send_set_mailbox().await.unwrap();

    // Check the aggregate before the per-record lookup (Finding 26).
    assert!(response.unwrap_create_errors().is_ok());
    let err = response.destroyed("MB674cc24095db49ce").unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::MailboxHasChild);
    assert_eq!(set_err.to_string(), "mailboxHasChild");
}

// ---------------------------------------------------------------------------
// 5. Mailbox/query
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_query_sends_filter_and_sort_and_returns_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Mailbox/query",
        query_body(vec!["MB23cfa8094c0f41e6"]),
    )
    .await;

    let response = client
        .mailbox_query(
            Some(mailbox_query::Filter::name("Inbox")),
            Some([mailbox_query::Comparator::sort_order()]),
        )
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.query_state(), "e35e9facf117-780615:0");
    assert_eq!(response.position(), 0);
    assert_eq!(response.total(), Some(1));
    assert!(response.can_calculate_changes());
    assert_eq!(response.ids(), ["MB23cfa8094c0f41e6"]);

    // RFC 8621 §2.3: the filter conditions and the two sortable properties.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/query");
    assert_eq!(call[1]["filter"], json!({ "name": "Inbox" }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": true, "property": "sortOrder" }])
    );
    // Mailbox query arguments are plain bools, so they are always present
    // (Finding 35) — and default to false.
    assert_eq!(call[1]["sortAsTree"], false);
    assert_eq!(call[1]["filterAsTree"], false);
}

#[tokio::test]
async fn mailbox_query_composes_and_or_filter_operators() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/query", query_body(vec![])).await;

    client
        .mailbox_query(
            Some(Filter::and([
                mailbox_query::Filter::has_any_role(true).into(),
                Filter::or([
                    mailbox_query::Filter::is_subscribed(true),
                    mailbox_query::Filter::role(Role::Inbox),
                ]),
            ])),
            None::<MailboxSort>,
        )
        .await
        .unwrap();

    // RFC 8620 §5.5 filter operators nest; RFC 8621 §2.3 supplies the
    // Mailbox conditions.
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "hasAnyRole": true },
                { "operator": "OR",
                  "conditions": [ { "isSubscribed": true },
                                 { "role": "inbox" } ] }
            ]
        })
    );
}

#[tokio::test]
async fn mailbox_query_with_the_tree_arguments_sorts_and_filters_as_a_tree() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Mailbox/query",
        query_body(vec!["MB23cfa8094c0f41e6", "MB674cc24095db49ce"]),
    )
    .await;

    let mut request = client.build();
    {
        let query = request.query_mailbox();
        query.filter(mailbox_query::Filter::parent_id(Some(
            "MB23cfa8094c0f41e6",
        )));
        query.arguments().sort_as_tree(true).filter_as_tree(true);
        query.limit(10).calculate_total(true);
    }
    let response = request.send_query_mailbox().await.unwrap();
    assert_eq!(response.ids().len(), 2);

    // RFC 8621 §2.3: sortAsTree keeps an ancestor before its descendant;
    // filterAsTree only includes a Mailbox whose ancestors also match.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["filter"], json!({ "parentId": "MB23cfa8094c0f41e6" }));
    assert_eq!(call[1]["sortAsTree"], true);
    assert_eq!(call[1]["filterAsTree"], true);
    assert_eq!(call[1]["limit"], 10);
    assert_eq!(call[1]["calculateTotal"], true);
}

#[tokio::test]
async fn mailbox_query_with_a_null_parent_filter_targets_top_level_mailboxes() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Mailbox/query", query_body(vec![])).await;

    let mut request = client.build();
    request
        .query_mailbox()
        .filter(mailbox_query::Filter::parent_id(None::<String>));
    request.send_query_mailbox().await.unwrap();

    // RFC 8621 §2.3: `parentId: "Id|null"` — null means "no parent".
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["filter"],
        json!({ "parentId": null })
    );
}

// ---------------------------------------------------------------------------
// 6. Mailbox/queryChanges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_query_changes_returns_added_removed_and_new_state() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.6 / RFC 8621 §2.4 response shape.
    mount_one(
        &server,
        "Mailbox/queryChanges",
        json!({
            "accountId": ACCOUNT_ID,
            "oldQueryState": "09aa9a075588-780599:0",
            "newQueryState": "e35e9facf117-780615:0",
            "total": 3,
            "removed": [ "MB0000000000000000" ],
            "added": [ { "id": "MB674cc24095db49ce", "index": 0 } ]
        }),
    )
    .await;

    let mut request = client.build();
    {
        let query = request.query_mailbox_changes("09aa9a075588-780599:0");
        query.filter(mailbox_query::Filter::is_subscribed(true));
        query.sort([mailbox_query::Comparator::name().descending()]);
        query.up_to_id("MB674cc24095db49ce");
        query.max_changes(25);
        query.calculate_total(true);
    }
    let response = request.send_query_mailbox_changes().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_query_state(), "09aa9a075588-780599:0");
    assert_eq!(response.new_query_state(), "e35e9facf117-780615:0");
    assert_eq!(response.total(), Some(3));
    assert_eq!(response.removed(), ["MB0000000000000000"]);
    assert_eq!(response.added().len(), 1);
    assert_eq!(response.added()[0].id(), "MB674cc24095db49ce");
    assert_eq!(response.added()[0].index(), 0);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/queryChanges");
    assert_eq!(call[1]["sinceQueryState"], "09aa9a075588-780599:0");
    assert_eq!(call[1]["upToId"], "MB674cc24095db49ce");
    assert_eq!(call[1]["maxChanges"], 25);
    assert_eq!(call[1]["calculateTotal"], true);
    assert_eq!(call[1]["filter"], json!({ "isSubscribed": true }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": false, "property": "name" }])
    );
}

// ---------------------------------------------------------------------------
// 7. Mailbox/changes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_changes_reports_updated_properties_and_id_lists() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §2.6, first response of the changes→get chain, verbatim.
    mount_one(
        &server,
        "Mailbox/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78541",
            "newState": "78542",
            "hasMoreChanges": false,
            "updatedProperties": [
                "totalEmails", "unreadEmails",
                "totalThreads", "unreadThreads"
            ],
            "created": [],
            "updated": ["MB23cfa8094c0f41e6"],
            "destroyed": []
        }),
    )
    .await;

    let response = client.mailbox_changes("78541", 128).await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "78541");
    assert_eq!(response.new_state(), "78542");
    assert!(!response.has_more_changes());
    assert!(response.created().is_empty());
    assert_eq!(response.updated(), ["MB23cfa8094c0f41e6"]);
    assert!(response.destroyed().is_empty());
    assert_eq!(response.total_changes(), 1);

    // `updatedProperties` is the Mailbox *extension* (Finding 37), reached
    // through `arguments()`, not a core changes member.
    assert_eq!(
        response.arguments().updated_properties(),
        Some(&[
            Property::TotalEmails,
            Property::UnreadEmails,
            Property::TotalThreads,
            Property::UnreadThreads
        ][..])
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Mailbox/changes");
    assert_eq!(call[1]["sinceState"], "78541");
    assert_eq!(call[1]["maxChanges"], 128);
}

#[tokio::test]
async fn mailbox_changes_without_updated_properties_reports_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Mailbox/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "78542",
            "newState": "78543",
            "hasMoreChanges": true,
            "updatedProperties": null,
            "created": [ "MB674cc24095db49ce" ],
            "updated": [],
            "destroyed": []
        }),
    )
    .await;

    let response = client.mailbox_changes("78542", 10).await.unwrap();
    assert!(response.has_more_changes());
    assert_eq!(response.created(), ["MB674cc24095db49ce"]);
    assert!(response.arguments().updated_properties().is_none());
}

// ---------------------------------------------------------------------------
// 8. Error paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mailbox_method_level_error_maps_to_error_method() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §3.6.2: an error Invocation over HTTP 200.
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "invalidArguments")], SESSION_STATE),
    )
    .await;

    let err = client
        .mailbox_get("MB23cfa8094c0f41e6", None::<MailboxProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Mailbox/get"
    );
}

#[tokio::test]
async fn mailbox_query_error_does_not_stop_a_paired_get_from_unwrapping() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![
                method_error("s0", "unsupportedFilter"),
                inv(
                    "Mailbox/get",
                    "s1",
                    mailbox_get_body(vec![rfc_inbox()], vec![]),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request.query_mailbox().filter(mailbox_query::Filter::role(Role::Inbox));
    request.get_mailbox().ids(["MB23cfa8094c0f41e6"]);
    let mut response = request.send().await.unwrap();

    assert!(response.method_response_by_id("s0").unwrap().is_error());
    let get = response
        .method_response_by_pos(1)
        .unwrap_get_mailbox()
        .unwrap();
    assert_eq!(get.list().len(), 1);
    assert_eq!(get.list()[0].name(), Some("Inbox"));
}
