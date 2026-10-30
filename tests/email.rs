//! P4 — mail-domain integration tests: **Emails**.
//!
//! Drives the `Client`/`Request` Email helpers from `src/email/helpers.rs`
//! against a `wiremock` HTTP server using the shared fixtures in
//! `tests/common/mod.rs`. Canned payloads are the RFC 8621 example bodies
//! wherever one exists (the section is cited on the fixture); otherwise the
//! minimal RFC-correct shape, also cited.
//!
//! Coverage targets: `email/helpers.rs` (every `Client::email_*` helper plus the
//! `Request::{get,changes,query,query_changes,set,copy,import,parse}_email`
//! builders), `email/get.rs` (record, body-part, body-value, address and header
//! accessors), `email/set.rs` (patch keys, `EmailBodyPart` builder, address
//! `From` impls), `email/query.rs` (filters and comparators), `email/import.rs`,
//! `email/parse.rs` and `email/mod.rs` (`Property`/`Header`/`HeaderForm`/
//! `HeaderValue`).

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use common::{
    connected_client, jmap_request_bodies, jmap_response, mount_jmap, single_jmap_request,
    ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::query::{Comparator as SortComparator, Filter as QueryFilter};
use jmap_client::core::request::ResultReference;
use jmap_client::core::set::SetErrorType;
use jmap_client::email::query as email_query;
use jmap_client::email::{BodyProperty, EmailBodyPart, Header, HeaderValue, Property};
use jmap_client::{Error, Get, Method, Set};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Type aliases
//
// The `Client::email_*` helpers take `Option<impl IntoIterator<…>>`, so a test
// that supplies nothing still has to name the type.
// ---------------------------------------------------------------------------

/// The sort list `Client::email_query` / `Request::query_email` expect.
type EmailSort = Vec<SortComparator<email_query::Comparator>>;

/// The property list `Client::email_get` / `Client::email_parse` expect.
type EmailProps = Vec<Property>;

/// The body-property list `Client::email_parse` expects.
type EmailBodyProps = Vec<BodyProperty>;

/// The keyword list the `email_set_*` helpers expect.
type Keywords = Vec<String>;

/// A boolean filter node over the Email filter vocabulary.
type EmailFilter = QueryFilter<email_query::Filter>;

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

/// `EmailValue::{text_body, html_body, attachment}` want an
/// `Into<EmailBodyPart<Get>>`, but the builder produces `EmailBodyPart<Set>`.
/// A named conversion keeps the call sites free of inference ambiguity.
fn into_get_part(part: EmailBodyPart<Set>) -> EmailBodyPart<Get> {
    part.into()
}

/// RFC 8621 §4.2.1 — the `Email/get` example record, verbatim.
///
/// The `header:List-POST:asURLs` member is the flattened-header form: the key
/// is a `header:<Name>:as<Form>[:all]` string and the value is the parsed form
/// the client keeps in `Email::headers`.
fn rfc_email_get_example_record() -> Value {
    json!({
        "id": "f123u457",
        "threadId": "ef1314a",
        "mailboxIds": { "f123": true },
        "from": [{ "name": "Joe Bloggs", "email": "joe@example.com" }],
        "subject": "Dinner on Thursday?",
        "receivedAt": "2013-10-13T14:12:00Z",
        "header:List-POST:asURLs": [
            "mailto:partytime@lists.example.com"
        ],
        "htmlBody": [{
            "partId": "1",
            "blobId": "B841623871",
            "size": 283331,
            "type": "text/html"
        }, {
            "partId": "2",
            "blobId": "B319437193",
            "size": 10343,
            "type": "text/plain"
        }],
        "bodyValues": {
            "1": {
                "isEncodingProblem": false,
                "isTruncated": true,
                "value": "<html><body><p>Hello ..."
            },
            "2": {
                "isEncodingProblem": false,
                "isTruncated": false,
                "value": "-- Sent by your friendly mailing list ..."
            }
        }
    })
}

/// A `Mailbox/set`-style `updated` envelope for the Email/set fixtures
/// (RFC 8620 §5.3: `updated` maps id → the updated object or `null`).
fn email_updated_body(id: &str) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "780839",
        "newState": "780840",
        "updated": { id: null }
    })
}

/// A `Email/get` result envelope (RFC 8620 §5.1).
fn email_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "state": "41234123231",
        "list": list,
        "notFound": not_found
    })
}

/// A `Email/query` result envelope (RFC 8620 §5.5).
fn email_query_body(ids: Vec<&str>) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "queryState": "09aa9a075588-780599:0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ids,
        "total": ids.len()
    })
}

/// Answers `POST /upload/{ACCOUNT_ID}` with the RFC 8620 §5.1 upload response.
async fn mount_upload(server: &MockServer, blob_id: &str, type_: &str, size: usize) {
    Mock::given(method("POST"))
        .and(path(format!("/upload/{ACCOUNT_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "accountId": ACCOUNT_ID,
            "blobId": blob_id,
            "type": type_,
            "size": size
        })))
        .mount(server)
        .await
}

// ---------------------------------------------------------------------------
// 1. Email/get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_get_returns_the_rfc_8621_example_record() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/get",
        email_get_body(vec![rfc_email_get_example_record()], vec!["f123u456"]),
    )
    .await;

    let email = client
        .email_get("f123u457", None::<EmailProps>)
        .await
        .unwrap()
        .expect("the example record must come back");

    assert_eq!(email.id(), Some("f123u457"));
    assert_eq!(email.thread_id(), Some("ef1314a"));
    assert_eq!(email.mailbox_ids(), ["f123"]);
    assert_eq!(email.subject(), Some("Dinner on Thursday?"));
    // 2013-10-13T14:12:00Z
    assert_eq!(email.received_at(), Some(1_381_673_520));
    // Members the record does not carry read as absent/zero, not as errors.
    assert!(email.keywords().is_empty());
    assert_eq!(email.size(), 0);
    assert!(!email.has_attachment());
    assert!(email.message_id().is_none());
    assert!(email.in_reply_to().is_none());
    assert!(email.references().is_none());
    assert!(email.sender().is_none());
    assert!(email.cc().is_none());
    assert!(email.bcc().is_none());
    assert!(email.reply_to().is_none());
    assert!(email.preview().is_none());
    assert!(email.text_body().is_none());
    assert!(email.body_structure().is_none());

    let from = email.from().expect("from must be present");
    assert_eq!(from.len(), 1);
    assert_eq!(from[0].name(), Some("Joe Bloggs"));
    assert_eq!(from[0].email(), "joe@example.com");

    // The flattened header round-trips under the exact key that was sent.
    let list_post = email
        .header(&Header::as_urls("List-POST", false))
        .expect("header:List-POST:asURLs must be reachable");
    // `HeaderValue` is an untagged enum without `PartialEq`, so match the variant.
    assert!(
        matches!(
            list_post,
            HeaderValue::AsTextAll(urls)
                if urls == &vec!["mailto:partytime@lists.example.com".to_string()]
        ),
        "unexpected List-POST header value: {list_post:?}"
    );
    assert!(email.has_header(&Header::as_urls("List-POST", false)));
    assert!(!email.has_header(&Header::as_text("List-POST", false)));

    let html = email.html_body().expect("htmlBody must be present");
    assert_eq!(html.len(), 2);
    assert_eq!(html[0].part_id(), Some("1"));
    assert_eq!(html[0].blob_id(), Some("B841623871"));
    assert_eq!(html[0].size(), 283_331);
    assert_eq!(html[0].content_type(), Some("text/html"));
    assert_eq!(html[1].part_id(), Some("2"));
    assert_eq!(html[1].content_type(), Some("text/plain"));
    // Properties this part did not carry.
    assert!(html[0].name().is_none());
    assert!(html[0].charset().is_none());
    assert!(html[0].headers().is_none());
    assert!(html[0].sub_parts().is_none());
    assert!(html[0].content_disposition().is_none());
    assert!(html[0].content_id().is_none());
    assert!(html[0].content_language().is_none());
    assert!(html[0].content_location().is_none());

    let truncated = email.body_value("1").expect("bodyValues[1]");
    assert_eq!(truncated.value(), "<html><body><p>Hello ...");
    assert!(truncated.is_truncated());
    assert!(!truncated.is_encoding_problem());
    let whole = email.body_value("2").expect("bodyValues[2]");
    assert!(!whole.is_truncated());
    assert!(email.body_value("nope").is_none());

    // --- wire (RFC 8621 §4.2.1 request) ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/get");
    assert_eq!(call[2], "s0");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(call[1]["ids"], json!(["f123u457"]));
    assert!(call[1].get("properties").is_none());
}

#[tokio::test]
async fn email_get_with_the_rfc_property_and_body_property_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/get", email_get_body(vec![], vec!["f123u456"])).await;

    let fetched = client
        .email_get(
            "f123u456",
            Some([
                Property::ThreadId,
                Property::MailboxIds,
                Property::From,
                Property::Subject,
                Property::ReceivedAt,
                Property::Header(Header::as_urls("List-POST", false)),
                Property::HtmlBody,
                Property::BodyValues,
            ]),
        )
        .await
        .unwrap();
    assert!(fetched.is_none());

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["properties"],
        json!([
            "threadId",
            "mailboxIds",
            "from",
            "subject",
            "receivedAt",
            "header:List-POST:asURLs",
            "htmlBody",
            "bodyValues"
        ])
    );
}

#[tokio::test]
async fn email_get_arguments_serialise_body_properties_and_value_limits() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/get", email_get_body(vec![], vec![])).await;

    let mut request = client.build();
    {
        let get = request.get_email().ids(["f123u457"]);
        get.properties([
            Property::BodyStructure,
            Property::BodyValues,
            Property::Header(Header::as_text("X-Priority", true)),
        ]);
        get.arguments()
            .body_properties([
                BodyProperty::PartId,
                BodyProperty::BlobId,
                BodyProperty::Size,
                BodyProperty::Type,
                BodyProperty::Headers,
                BodyProperty::Name,
                BodyProperty::Charset,
                BodyProperty::Disposition,
                BodyProperty::Cid,
                BodyProperty::Language,
                BodyProperty::Location,
                BodyProperty::SubParts,
                BodyProperty::Header(Header::as_text("Content-Language", false)),
            ])
            .fetch_text_body_values(true)
            .fetch_html_body_values(true)
            .fetch_all_body_values(false)
            .max_body_value_bytes(256);
    }
    request.send_get_email().await.unwrap();

    // RFC 8621 §4.2: the Email/get extension arguments.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["bodyProperties"],
        json!([
            "partId",
            "blobId",
            "size",
            "type",
            "headers",
            "name",
            "charset",
            "disposition",
            "cid",
            "language",
            "location",
            "subParts",
            "header:Content-Language:asText"
        ])
    );
    assert_eq!(call[1]["fetchTextBodyValues"], true);
    assert_eq!(call[1]["fetchHTMLBodyValues"], true);
    assert_eq!(call[1]["fetchAllBodyValues"], false);
    assert_eq!(call[1]["maxBodyValueBytes"], 256);
    assert_eq!(call[1]["properties"][2], "header:X-Priority:asText:all");
}

#[tokio::test]
async fn email_get_aliased_header_properties_land_on_the_typed_fields() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.1.1: `messageId`/`inReplyTo`/`references`/`from`/`to`/
    // `subject`/`sentAt` are the *aliases* for the corresponding
    // `header:…:as<Form>` property names, and the client accepts both spellings.
    mount_one(
        &server,
        "Email/get",
        email_get_body(
            vec![json!({
                "id": "e1",
                "header:Message-ID:asMessageIds": ["<bbce0ae9@sloti7d1t02>"],
                "header:In-Reply-To:asMessageIds": ["<a1@x>"],
                "header:References:asMessageIds": ["<a1@x>", "<a2@x>"],
                "header:From:asAddresses": [{ "name": "Joe Bloggs", "email": "joe@example.com" }],
                "header:To:asAddresses": [{ "name": null, "email": "john@example.com" }],
                "header:Subject:asText": "World domination",
                "header:Date:asDate": "2018-07-10T01:05:08Z"
            })],
            vec![],
        ),
    )
    .await;

    let email = client
        .email_get(
            "e1",
            Some([
                Property::Header(Header::as_message_ids("Message-ID", false)),
                Property::Header(Header::as_message_ids("In-Reply-To", false)),
                Property::Header(Header::as_message_ids("References", false)),
                Property::Header(Header::as_addresses("From", false)),
                Property::Header(Header::as_addresses("To", false)),
                Property::Header(Header::as_text("Subject", false)),
                Property::Header(Header::as_date("Date", false)),
            ]),
        )
        .await
        .unwrap()
        .unwrap();

    // Accessors borrow `&[String]`, so compare owned copies.
    assert_eq!(
        email.message_id().map(<[String]>::to_vec),
        Some(vec!["<bbce0ae9@sloti7d1t02>".to_string()])
    );
    assert_eq!(
        email.in_reply_to().map(<[String]>::to_vec),
        Some(vec!["<a1@x>".to_string()])
    );
    assert_eq!(
        email.references().map(<[String]>::to_vec),
        Some(vec!["<a1@x>".to_string(), "<a2@x>".to_string()])
    );
    assert_eq!(email.from().unwrap()[0].email(), "joe@example.com");
    assert_eq!(email.to().unwrap()[0].name(), None);
    assert_eq!(email.to().unwrap()[0].email(), "john@example.com");
    assert_eq!(email.subject(), Some("World domination"));
    // 2018-07-10T01:05:08Z
    assert_eq!(email.sent_at(), Some(1_531_184_708));

    // The property names go out exactly as the caller spelled them.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["properties"][0], "header:Message-ID:asMessageIds");
    assert_eq!(call[1]["properties"][6], "header:Date:asDate");
}

#[tokio::test]
async fn header_aliases_are_case_sensitive_so_other_spellings_land_in_the_header_map() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // The `messageId` alias is spelled `header:Message-ID:asMessageIds` (all
    // caps `ID`). A server that answers `header:Message-Id:asMessageIds` is
    // still valid JMAP, but the value cannot reach the typed field: it falls
    // through to the flattened `Email::headers` map instead.
    mount_one(
        &server,
        "Email/get",
        email_get_body(
            vec![json!({
                "id": "e2",
                "header:Message-Id:asMessageIds": ["<mixed-case@example.org>"],
            })],
            vec![],
        ),
    )
    .await;

    let email = client
        .email_get("e2", Some([Property::MessageId]))
        .await
        .unwrap()
        .unwrap();

    assert!(
        email.message_id().is_none(),
        "the `Message-Id` spelling must not populate `messageId`"
    );
    assert_eq!(
        email
            .header(&Header::as_message_ids("Message-Id", false))
            .map(|v| match v {
                HeaderValue::AsTextAll(ids) => ids.clone(),
                other => panic!("unexpected header value: {other:?}"),
            }),
        Some(vec!["<mixed-case@example.org>".to_string()])
    );
}

#[tokio::test]
async fn email_get_grouped_addresses_land_in_the_flattened_header_map() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.1.1 address-group example: `Friends: jane@example.com; …`.
    mount_one(
        &server,
        "Email/get",
        email_get_body(
            vec![json!({
                "id": "e2",
                "header:To:asGroupedAddresses": [
                    { "name": "Friends",
                      "addresses": [{ "name": null, "email": "jane@example.com" }] },
                    { "name": null,
                      "addresses": [
                          { "name": "John Smith", "email": "john@example.com" },
                          { "name": null, "email": "joe@example.com" }
                      ] }
                ]
            })],
            vec![],
        ),
    )
    .await;

    let email = client
        .email_get(
            "e2",
            Some([Property::Header(Header::as_grouped_addresses(
                "To", false,
            ))]),
        )
        .await
        .unwrap()
        .unwrap();

    // `to` only aliases `header:To:asAddresses`, so the grouped form is reachable
    // through the flattened header map rather than the typed field.
    assert!(email.to().is_none());
    let grouped = email
        .header(&Header::as_grouped_addresses("To", false))
        .expect("grouped To must be reachable");
    match grouped {
        HeaderValue::AsGroupedAddresses(groups) => {
            assert_eq!(groups.len(), 2);
            assert_eq!(groups[0].name(), Some("Friends"));
            assert_eq!(groups[0].addresses().len(), 1);
            assert_eq!(groups[0].addresses()[0].email(), "jane@example.com");
            assert_eq!(groups[1].name(), None);
            assert_eq!(groups[1].addresses().len(), 2);
            assert_eq!(groups[1].addresses()[0].name(), Some("John Smith"));
        }
        other => panic!("expected AsGroupedAddresses, got {other:?}"),
    }
}

#[tokio::test]
async fn email_get_of_an_unknown_id_returns_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/get", email_get_body(vec![], vec!["missing"]))
        .await;

    assert!(client
        .email_get("missing", Some([Property::Id]))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn an_unknown_email_member_is_a_parse_error_not_an_ignored_field() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // `Email` flattens unrecognised keys into `AHashMap<Header, _>`, so a member
    // that is not a known property *and* not a `header:…` key makes the whole
    // response undeserialisable (see the Findings log).
    mount_one(
        &server,
        "Email/get",
        email_get_body(vec![json!({ "id": "e1", "bogus": 1 })], vec![]),
    )
    .await;

    let err = client
        .email_get("e1", None::<EmailProps>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Parse(_)),
        "expected Error::Parse, got {err:?}"
    );
}

#[tokio::test]
async fn an_unknown_property_name_is_property_other_and_round_trips_verbatim() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/get", email_get_body(vec![], vec![])).await;

    let property: Property = serde_json::from_str("\"xVendorThing\"").unwrap();
    assert_eq!(property, Property::Other("xVendorThing".to_string()));
    assert_eq!(property.to_string(), "xVendorThing");

    client
        .email_get("e1", Some([Property::Other("xVendorThing".to_string())]))
        .await
        .unwrap();
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["properties"],
        json!(["xVendorThing"])
    );
}

// ---------------------------------------------------------------------------
// 2. Email/set — patches
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_set_mailbox_sends_a_mailbox_ids_dotted_patch_key() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/set", email_updated_body("Mf40b5f831efa7233b9eb1c7f")).await;

    let updated = client
        .email_set_mailbox("Mf40b5f831efa7233b9eb1c7f", "2ea1ca41b38e", true)
        .await
        .unwrap();
    // RFC 8620 §5.3 lets `updated` carry `null`, which the helper reports as
    // "updated, no data returned".
    assert!(updated.is_none());

    // RFC 8621 §4.6: mutable Email properties are patched with dotted keys.
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][1]["update"],
        json!({
            "Mf40b5f831efa7233b9eb1c7f": { "mailboxIds/2ea1ca41b38e": true }
        })
    );
}

#[tokio::test]
async fn email_set_mailboxes_replaces_the_whole_mailbox_ids_map() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/set", email_updated_body("e1")).await;

    client
        .email_set_mailboxes("e1", ["2ea1ca41b38e", "75a4c956"])
        .await
        .unwrap();

    let update = single_jmap_request(&server).await["methodCalls"][0][1]["update"].clone();
    assert_eq!(
        update["e1"]["mailboxIds"],
        json!({ "2ea1ca41b38e": true, "75a4c956": true })
    );
    // A whole-map replacement must not also emit a dotted patch key.
    assert!(update["e1"]
        .as_object()
        .unwrap()
        .keys()
        .all(|k| !k.contains('/')));
}

#[tokio::test]
async fn email_set_keyword_uses_the_keywords_dotted_patch_key() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;

    // RFC 8621 §10.4.1/§10.4.2/§10.4.3 register `$draft`, `$seen` and
    // `$flagged`.
    // The crate exposes no dedicated seen/flagged/draft helpers — the keyword is
    // passed through verbatim, which is what the RFC requires.
    for (keyword, set) in [("$seen", true), ("$flagged", false), ("$draft", true)] {
        mount_one(&server, "Email/set", email_updated_body("e1")).await;
        client.email_set_keyword("e1", keyword, set).await.unwrap();

        let bodies = jmap_request_bodies(&server).await;
        assert_eq!(bodies.len(), 1, "exactly one JMAP call per iteration");
        let last = bodies.last().unwrap();
        assert_eq!(
            last["methodCalls"][0][1]["update"],
            json!({ "e1": { format!("keywords/{keyword}"): set } })
        );
        // Clear the recording so the next iteration's `bodies.last()` and the
        // length check above stay unambiguous.
        server.reset().await;
    }
}

#[tokio::test]
async fn email_set_keywords_replaces_the_keyword_map() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/set", email_updated_body("e1")).await;

    client
        .email_set_keywords("e1", ["$seen", "$draft", "$flagged"])
        .await
        .unwrap();

    let update = single_jmap_request(&server).await["methodCalls"][0][1]["update"].clone();
    assert_eq!(
        update["e1"]["keywords"],
        json!({ "$seen": true, "$draft": true, "$flagged": true })
    );
    assert!(update["e1"].get("mailboxIds").is_none());
}

#[tokio::test]
async fn email_destroy_sends_a_destroy_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780840",
            "newState": "780841",
            "destroyed": [ "Mf40b5f831efa7233b9eb1c7f" ]
        }),
    )
    .await;

    client
        .email_destroy("Mf40b5f831efa7233b9eb1c7f")
        .await
        .unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/set");
    assert_eq!(call[1]["destroy"], json!(["Mf40b5f831efa7233b9eb1c7f"]));
    assert!(call[1].get("update").is_none());
}

#[tokio::test]
async fn email_set_update_error_surfaces_as_error_set() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §5.3: a per-record failure is `notUpdated`, not a method error.
    mount_one(
        &server,
        "Email/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780840",
            "newState": "780840",
            "notUpdated": {
                "gone": {
                    "type": "notFound",
                    "description": "Email not found"
                }
            }
        }),
    )
    .await;

    let err = client
        .email_set_keyword("gone", "$seen", true)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::NotFound);
    assert_eq!(set_err.description(), Some("Email not found"));
    assert_eq!(set_err.to_string(), "notFound: Email not found");
}

#[tokio::test]
async fn email_destroy_reports_a_not_destroyed_set_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780840",
            "newState": "780840",
            "notDestroyed": { "e1": { "type": "forbidden" } }
        }),
    )
    .await;

    let err = client.email_destroy("e1").await.unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::Forbidden);
}

// ---------------------------------------------------------------------------
// 3. Email/set — creating a draft (RFC 8621 §4.6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_create_a_draft_sends_the_full_rfc_8621_create_object() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.6, the "created" half of the draft example.
    mount_one(
        &server,
        "Email/set",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780839",
            "newState": "780840",
            "created": {
                "c0": {
                    "id": "Mf40b5f831efa7233b9eb1c7f",
                    "blobId": "Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7",
                    "threadId": "Td957e72e89f516dc",
                    "size": 359
                }
            }
        }),
    )
    .await;

    let mut request = client.build();
    {
        let set = request.set_email();
        let draft = set.create();
        draft
            .mailbox_ids(["2ea1ca41b38e"])
            .keywords(["$seen", "$draft"])
            .from([("Joe Bloggs", "joe@example.com")])
            .to([("John", "john@example.com")])
            .subject("World domination")
            .received_at(1_531_184_708)
            .sent_at(1_531_213_508)
            .message_id(["<bbce0ae9-58be-4b24-ac82-deb840d58016@sloti7d1t02>"])
            .in_reply_to(["<a1@example.com>"])
            .references(["<a1@example.com>"])
            .reply_to(["reply@example.com"])
            .cc(["cc@example.com"])
            .bcc(["bcc@example.com"])
            .sender(["sender@example.com"])
            .header(
                Header::as_text("X-Priority", false),
                HeaderValue::AsText("1".to_string()),
            )
            .body_structure(
                EmailBodyPart::new()
                    .content_type("multipart/alternative")
                    .sub_part(
                        EmailBodyPart::new()
                            .part_id("a49d")
                            .content_type("text/html")
                            .into(),
                    )
                    .sub_part(
                        EmailBodyPart::new()
                            .part_id("bd48")
                            .content_type("text/plain")
                            .into(),
                    )
                    .into(),
            )
            .body_value("bd48".to_string(), "I have the most brilliant plan.")
            .text_body(into_get_part(
                EmailBodyPart::new().part_id("bd48"),
            ))
            .html_body(into_get_part(
                EmailBodyPart::new().part_id("a49d"),
            ))
            .attachment(into_get_part(
                EmailBodyPart::new().part_id("att1").name("plan.txt"),
            ));
    }
    let mut response = request.send_set_email().await.unwrap();
    let created = response.created("c0").unwrap();
    assert_eq!(created.id(), Some("Mf40b5f831efa7233b9eb1c7f"));
    assert_eq!(created.blob_id(), Some("Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7"));
    assert_eq!(created.size(), 359);

    // --- wire ---
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/set");
    let draft_body = &call[1]["create"]["c0"];
    assert_eq!(draft_body["subject"], "World domination");
    assert_eq!(draft_body["mailboxIds"], json!({ "2ea1ca41b38e": true }));
    assert_eq!(draft_body["keywords"], json!({ "$seen": true, "$draft": true }));
    assert_eq!(
        draft_body["from"],
        json!([{ "name": "Joe Bloggs", "email": "joe@example.com" }])
    );
    assert_eq!(
        draft_body["to"],
        json!([{ "name": "John", "email": "john@example.com" }])
    );
    assert_eq!(
        draft_body["replyTo"],
        json!([{ "name": null, "email": "reply@example.com" }])
    );
    assert_eq!(
        draft_body["messageId"],
        json!(["<bbce0ae9-58be-4b24-ac82-deb840d58016@sloti7d1t02>"])
    );
    assert_eq!(draft_body["receivedAt"], "2018-07-10T01:05:08Z");
    assert_eq!(draft_body["sentAt"], "2018-07-10T09:05:08Z");
    assert_eq!(draft_body["bodyStructure"]["type"], "multipart/alternative");
    assert_eq!(draft_body["bodyStructure"]["subParts"][0]["partId"], "a49d");
    assert_eq!(draft_body["bodyStructure"]["subParts"][1]["type"], "text/plain");
    assert_eq!(
        draft_body["bodyValues"]["bd48"]["value"],
        "I have the most brilliant plan."
    );
    assert_eq!(draft_body["attachments"][0]["name"], "plan.txt");
    assert_eq!(draft_body["header:X-Priority:asText"], "1");
}

// ---------------------------------------------------------------------------
// 4. Email/query + Email/queryChanges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_query_sends_the_rfc_inbox_query_and_returns_ids() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/query",
        email_query_body(vec!["Ma783e5cdf5f2deffbc97930a"]),
    )
    .await;

    let response = client
        .email_query(
            Some(email_query::Filter::in_mailbox("fb666a55")),
            Some([email_query::Comparator::received_at().descending()]),
        )
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.query_state(), "09aa9a075588-780599:0");
    assert_eq!(response.position(), 0);
    assert_eq!(response.total(), Some(1));
    assert_eq!(response.ids(), ["Ma783e5cdf5f2deffbc97930a"]);

    // RFC 8621 §4.4 filter + §4.4.2 sort.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/query");
    assert_eq!(call[1]["filter"], json!({ "inMailbox": "fb666a55" }));
    assert_eq!(
        call[1]["sort"],
        json!([{ "isAscending": false, "property": "receivedAt" }])
    );
    // `collapseThreads` is an Option and was not set.
    assert!(call[1].get("collapseThreads").is_none());
}

#[tokio::test]
async fn email_query_with_collapse_threads_and_a_window() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/query", email_query_body(vec![])).await;

    let mut request = client.build();
    {
        let query = request.query_email();
        query.filter(email_query::Filter::in_mailbox("fb666a55"));
        query
            .sort([email_query::Comparator::received_at().descending()])
            .position(0)
            .limit(30)
            .calculate_total(true);
        query.arguments().collapse_threads(true);
    }
    request.send_query_email().await.unwrap();

    // RFC 8621 §4.4/§4.5: `collapseThreads` is the Email/query extension.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["collapseThreads"], true);
    assert_eq!(call[1]["position"], 0);
    assert_eq!(call[1]["limit"], 30);
    assert_eq!(call[1]["calculateTotal"], true);
}

#[tokio::test]
async fn email_query_composes_keyword_and_text_filters() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/query", email_query_body(vec![])).await;

    // `Filter::not` is generic over its condition container, so pin the element
    // type explicitly or `V` stays unconstrained.
    let not_seen_or_attached: [EmailFilter; 2] = [
        email_query::Filter::has_keyword("$seen").into(),
        email_query::Filter::has_attachment(true).into(),
    ];

    client
        .email_query(
            Some(QueryFilter::and([
                email_query::Filter::in_mailbox("fb666a55").into(),
                QueryFilter::not(not_seen_or_attached),
            ])),
            Some([
                email_query::Comparator::subject(),
                email_query::Comparator::has_keyword("$flagged"),
            ]),
        )
        .await
        .unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(
        call[1]["filter"],
        json!({
            "operator": "AND",
            "conditions": [
                { "inMailbox": "fb666a55" },
                { "operator": "NOT",
                  "conditions": [ { "hasKeyword": "$seen" },
                                 { "hasAttachment": true } ] }
            ]
        })
    );
    assert_eq!(
        call[1]["sort"],
        json!([
            { "isAscending": true, "property": "subject" },
            { "isAscending": true, "property": "hasKeyword", "keyword": "$flagged" }
        ])
    );
}

#[tokio::test]
async fn email_query_changes_returns_the_rfc_added_and_removed_sets() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.5 response, verbatim shape.
    mount_one(
        &server,
        "Email/queryChanges",
        json!({
            "accountId": ACCOUNT_ID,
            "oldQueryState": "09aa9a075588-780599:0",
            "newQueryState": "e35e9facf117-780615:0",
            "added": [{ "id": "Me8de6c9f6de198239b982ea2", "index": 0 }],
            "removed": [ "M9bd17497e2a99cb345fc1d0a" ],
            "total": 115
        }),
    )
    .await;

    let response = client
        .email_query_changes(
            "09aa9a075588-780599:0",
            Some(email_query::Filter::in_mailbox("fb666a55")),
        )
        .await
        .unwrap();

    assert_eq!(response.old_query_state(), "09aa9a075588-780599:0");
    assert_eq!(response.new_query_state(), "e35e9facf117-780615:0");
    assert_eq!(response.total(), Some(115));
    assert_eq!(response.removed(), ["M9bd17497e2a99cb345fc1d0a"]);
    assert_eq!(response.added()[0].id(), "Me8de6c9f6de198239b982ea2");
    assert_eq!(response.added()[0].index(), 0);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/queryChanges");
    assert_eq!(call[1]["sinceQueryState"], "09aa9a075588-780599:0");
    assert_eq!(call[1]["filter"], json!({ "inMailbox": "fb666a55" }));
    // Finding 35: `calculateTotal` is a plain bool, so it is always on the wire.
    assert_eq!(call[1]["calculateTotal"], false);
}

#[tokio::test]
async fn email_query_changes_with_up_to_id_and_max_changes() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/queryChanges",
        json!({
            "accountId": ACCOUNT_ID,
            "oldQueryState": "09aa9a075588-780599:0",
            "newQueryState": "e35e9facf117-780615:0",
            "added": [],
            "removed": []
        }),
    )
    .await;

    let mut request = client.build();
    {
        let query = request.query_email_changes("09aa9a075588-780599:0");
        query.filter(email_query::Filter::in_mailbox("fb666a55"));
        query
            .sort([email_query::Comparator::received_at().descending()])
            .up_to_id("Mc2781d5e856a908d8a35a564")
            .max_changes(25)
            .calculate_total(true);
        query.arguments().collapse_threads(true);
    }
    request.send_query_email_changes().await.unwrap();

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["upToId"], "Mc2781d5e856a908d8a35a564");
    assert_eq!(call[1]["maxChanges"], 25);
    assert_eq!(call[1]["calculateTotal"], true);
    assert_eq!(call[1]["collapseThreads"], true);
}

// ---------------------------------------------------------------------------
// 5. Email/changes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_changes_returns_the_rfc_id_lists() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.5 walkthrough, the `Email/changes` half.
    mount_one(
        &server,
        "Email/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780605",
            "newState": "780800",
            "hasMoreChanges": false,
            "created": [ "Me8de6c9f6de198239b982ea2" ],
            "updated": [ "Ma783e5cdf5f2deffbc97930a" ],
            "destroyed": [ "M9bd17497e2a99cb345fc1d0a" ]
        }),
    )
    .await;

    let response = client.email_changes("780605", Some(50)).await.unwrap();
    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), "780605");
    assert_eq!(response.new_state(), "780800");
    assert!(!response.has_more_changes());
    assert_eq!(response.created(), ["Me8de6c9f6de198239b982ea2"]);
    assert_eq!(response.updated(), ["Ma783e5cdf5f2deffbc97930a"]);
    assert_eq!(response.destroyed(), ["M9bd17497e2a99cb345fc1d0a"]);
    assert_eq!(response.total_changes(), 3);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/changes");
    assert_eq!(call[1]["sinceState"], "780605");
    assert_eq!(call[1]["maxChanges"], 50);
}

#[tokio::test]
async fn email_changes_without_max_changes_omits_the_member() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/changes",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780800",
            "newState": "780801",
            "hasMoreChanges": true,
            "created": [],
            "updated": [],
            "destroyed": []
        }),
    )
    .await;

    let response = client.email_changes("780800", None).await.unwrap();
    assert!(response.has_more_changes());
    assert_eq!(response.total_changes(), 0);
    assert!(single_jmap_request(&server).await["methodCalls"][0][1]
        .get("maxChanges")
        .is_none());
}

// ---------------------------------------------------------------------------
// 6. Email/import (RFC 8621 §4.8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_import_uploads_the_raw_message_then_imports_the_blob() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    let raw = b"From: joe@example.com\r\nSubject: Imported\r\n\r\nBody\r\n".to_vec();
    mount_upload(
        &server,
        "Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7",
        "message/rfc822",
        raw.len(),
    )
    .await;
    // RFC 8621 §4.8: `created` maps the creation id to `{id, blobId, threadId,
    // size}`.
    mount_one(
        &server,
        "Email/import",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780839",
            "newState": "780840",
            "created": {
                "i0": {
                    "id": "Mf40b5f831efa7233b9eb1c7f",
                    "blobId": "Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7",
                    "threadId": "Td957e72e89f516dc",
                    "size": 359
                }
            }
        }),
    )
    .await;

    let email = client
        .email_import(
            raw.clone(),
            ["2ea1ca41b38e"],
            Some(["$seen"]),
            Some(1_531_184_708),
        )
        .await
        .unwrap();

    assert_eq!(email.id(), Some("Mf40b5f831efa7233b9eb1c7f"));
    assert_eq!(email.thread_id(), Some("Td957e72e89f516dc"));
    assert_eq!(
        email.blob_id(),
        Some("Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7")
    );
    assert_eq!(email.size(), 359);
    // RFC 8621 §4.8 puts only `{id, blobId, threadId, size}` in `created`, so
    // the mailbox placement is visible on the request, not on the returned record.
    assert!(email.mailbox_ids().is_empty());

    // The upload leg: raw bytes, `application/octet-stream`, on the session's
    // uploadUrl with the account id substituted.
    let upload = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == format!("/upload/{ACCOUNT_ID}"))
        .expect("the client must POST the blob before importing it");
    assert_eq!(upload.headers["Content-Type"], "application/octet-stream");
    assert_eq!(upload.body, raw);

    // The import leg: the blobId from the upload response is what gets imported,
    // under creation id `i0` (`EmailImport::create_id`).
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/import");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    assert_eq!(
        call[1]["emails"]["i0"]["blobId"],
        "Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7"
    );
    assert_eq!(call[1]["emails"]["i0"]["mailboxIds"], json!({ "2ea1ca41b38e": true }));
    assert_eq!(call[1]["emails"]["i0"]["keywords"], json!({ "$seen": true }));
    assert_eq!(call[1]["emails"]["i0"]["receivedAt"], "2018-07-10T01:05:08Z");
}

#[tokio::test]
async fn email_import_without_keywords_or_received_at_sends_only_the_required_members() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_upload(&server, "B1", "message/rfc822", 4).await;
    mount_one(
        &server,
        "Email/import",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": null,
            "newState": "780841",
            "created": {
                "i0": { "id": "M1", "blobId": "B1", "threadId": "T1", "size": 4 }
            }
        }),
    )
    .await;

    let email = client
        .email_import(b"raw!".to_vec(), ["mb1"], None::<Keywords>, None)
        .await
        .unwrap();
    assert_eq!(email.id(), Some("M1"));
    assert_eq!(email.size(), 4);

    let imported = single_jmap_request(&server).await["methodCalls"][0][1]["emails"]["i0"].clone();
    assert_eq!(imported["blobId"], "B1");
    assert_eq!(imported["mailboxIds"], json!({ "mb1": true }));
    // `keywords` is a required (but possibly empty) member; `receivedAt` is not.
    assert_eq!(imported["keywords"], json!({}));
    assert!(imported.get("receivedAt").is_none());
    assert!(
        single_jmap_request(&server).await["methodCalls"][0][1]
            .get("ifInState")
            .is_none(),
        "`email_import` never sets `ifInState`"
    );
}

#[tokio::test]
async fn email_import_reports_a_duplicate_as_already_exists() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_upload(&server, "B1", "message/rfc822", 4).await;
    // RFC 8621 §4.8: a duplicate import is rejected with `alreadyExists`.
    mount_one(
        &server,
        "Email/import",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780841",
            "newState": "780841",
            "notCreated": {
                "i0": { "type": "alreadyExists", "description": "Duplicate message" }
            }
        }),
    )
    .await;

    let err = client
        .email_import(b"raw!".to_vec(), ["mb1"], None::<Keywords>, None)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::AlreadyExists);
    assert_eq!(set_err.description(), Some("Duplicate message"));
}

#[tokio::test]
async fn email_import_reports_invalid_properties() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_upload(&server, "B1", "message/rfc822", 4).await;
    // RFC 8621 §4.8: an unknown mailbox id is an `invalidProperties` SetError.
    mount_one(
        &server,
        "Email/import",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780841",
            "newState": "780841",
            "notCreated": {
                "i0": { "type": "invalidProperties", "properties": ["mailboxIds"] }
            }
        }),
    )
    .await;

    let err = client
        .email_import(b"raw!".to_vec(), ["nope"], None::<Keywords>, None)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::InvalidProperties);
    assert_eq!(set_err.properties(), Some(&["mailboxIds".to_string()][..]));
}

// ---------------------------------------------------------------------------
// 7. Email/parse (RFC 8621 §4.9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_parse_returns_the_parsed_email() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/parse",
        json!({
            "accountId": ACCOUNT_ID,
            "parsed": { "B1": rfc_email_get_example_record() },
            "notParsable": null,
            "notFound": null
        }),
    )
    .await;

    let email = client
        .email_parse(
            "B1",
            Some([Property::Subject, Property::HtmlBody, Property::BodyValues]),
            Some([BodyProperty::PartId, BodyProperty::Type]),
            Some(256),
        )
        .await
        .unwrap();

    assert_eq!(email.subject(), Some("Dinner on Thursday?"));
    assert_eq!(email.html_body().unwrap().len(), 2);

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/parse");
    assert_eq!(call[1]["blobIds"], json!(["B1"]));
    assert_eq!(call[1]["properties"], json!(["subject", "htmlBody", "bodyValues"]));
    assert_eq!(call[1]["bodyProperties"], json!(["partId", "type"]));
    // `Client::email_parse` turns on `fetchAllBodyValues` whenever a byte cap is
    // given, so the cap actually bites on every part.
    assert_eq!(call[1]["fetchAllBodyValues"], true);
    assert_eq!(call[1]["maxBodyValueBytes"], 256);
}

#[tokio::test]
async fn email_parse_reports_a_blob_that_is_not_parsable() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/parse",
        json!({
            "accountId": ACCOUNT_ID,
            "parsed": null,
            "notParsable": [ "B1" ],
            "notFound": []
        }),
    )
    .await;

    let err = client
        .email_parse("B1", None::<EmailProps>, None::<EmailBodyProps>, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Internal(_)),
        "expected Error::Internal, got {err:?}"
    );
    assert!(err.to_string().contains("not parsable"));
}

#[tokio::test]
async fn email_parse_reports_a_missing_blob() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/parse",
        json!({
            "accountId": ACCOUNT_ID,
            "parsed": null,
            "notParsable": [],
            "notFound": [ "gone" ]
        }),
    )
    .await;

    let err = client
        .email_parse("gone", None::<EmailProps>, None::<EmailBodyProps>, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err:?}");
}

// ---------------------------------------------------------------------------
// 8. Email/copy (RFC 8621 §4.7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_copy_copies_a_message_into_another_account() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.7 response, with the trailing implicit `Email/set` response
    // omitted: see the paired test below for why.
    mount_one(
        &server,
        "Email/copy",
        json!({
            "fromAccountId": "ue150411c",
            "accountId": ACCOUNT_ID,
            "oldState": "7ee7e9263a6d",
            "newState": "5a0d2447ed26",
            "created": {
                "Md45b47b4877521042cec0938": {
                    "id": "M138f9954a5cd2423daeafa55",
                    "blobId": "G6b9fb047cba722c48c611e79233d057c6b0b74e8",
                    "threadId": "T2f242ea424a4079a",
                    "size": 11721
                }
            },
            "notCreated": null
        }),
    )
    .await;

    let copied = client
        .email_copy(
            "ue150411c",
            "Md45b47b4877521042cec0938",
            ["75a4c956"],
            Some(["$seen"]),
            Some(1_531_184_708),
        )
        .await
        .unwrap();

    assert_eq!(copied.id(), Some("M138f9954a5cd2423daeafa55"));
    assert_eq!(copied.thread_id(), Some("T2f242ea424a4079a"));
    assert_eq!(copied.size(), 11721);

    // RFC 8621 §4.7: the copy MUST set new `mailboxIds`, because the source
    // account's ids are meaningless in the destination.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/copy");
    assert_eq!(call[1]["fromAccountId"], "ue150411c");
    assert_eq!(call[1]["accountId"], ACCOUNT_ID);
    let created = &call[1]["create"]["Md45b47b4877521042cec0938"];
    assert_eq!(created["id"], "Md45b47b4877521042cec0938");
    assert_eq!(created["mailboxIds"], json!({ "75a4c956": true }));
    assert_eq!(created["keywords"], json!({ "$seen": true }));
    assert_eq!(created["receivedAt"], "2018-07-10T01:05:08Z");
    // The `Client` helper never destroys the original.
    assert_eq!(call[1]["onSuccessDestroyOriginal"], false);
}

#[tokio::test]
async fn email_copy_with_destroy_original_needs_the_request_builder() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/copy",
        json!({
            "fromAccountId": "ue150411c",
            "accountId": ACCOUNT_ID,
            "oldState": "7ee7e9263a6d",
            "newState": "5a0d2447ed26",
            "created": {
                "src1": { "id": "dst1", "blobId": "B2", "threadId": "T2", "size": 10 }
            }
        }),
    )
    .await;

    let mut request = client.build().account_id(ACCOUNT_ID);
    {
        let copy = request.copy_email("ue150411c");
        copy.create("src1").mailbox_ids(["75a4c956"]);
        copy.on_success_destroy_original(true);
        copy.destroy_from_if_in_state("7ee7e9263a6d");
        copy.if_in_state("5a0d2447ed25");
    }
    let mut response = request.send_copy_email().await.unwrap();
    assert_eq!(response.from_account_id(), "ue150411c");
    assert_eq!(response.new_state(), "5a0d2447ed26");
    assert_eq!(
        response.created("src1").unwrap().id(),
        Some("dst1")
    );

    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[1]["onSuccessDestroyOriginal"], true);
    assert_eq!(call[1]["destroyFromIfInState"], "7ee7e9263a6d");
    assert_eq!(call[1]["ifInState"], "5a0d2447ed25");
}

#[tokio::test]
async fn email_copy_reports_not_created() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/copy",
        json!({
            "fromAccountId": "ue150411c",
            "accountId": ACCOUNT_ID,
            "oldState": "7ee7e9263a6d",
            "newState": "7ee7e9263a6d",
            "notCreated": {
                "src1": { "type": "notFound", "description": "Source Email not found" }
            }
        }),
    )
    .await;

    let err = client
        .email_copy("ue150411c", "src1", ["75a4c956"], None::<Keywords>, None)
        .await
        .unwrap_err();
    let Error::Set(set_err) = err else {
        panic!("expected Error::Set, got {err:?}")
    };
    assert_eq!(set_err.type_, SetErrorType::NotFound);
}

#[tokio::test]
async fn email_copy_with_the_rfc_implicit_set_response_returns_the_copy_result() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8621 §4.7 makes `Email/copy` a standard `/copy`, so RFC 8620 §5.4
    // applies: with `onSuccessDestroyOriginal` the server MUST make the implicit
    // `Email/set` call "after emitting the Foo/copy response", and "the output
    // of this is added to the responses as normal, to be returned to the
    // client". That is TWO method responses for the one call, the copy result
    // FIRST and the implicit `Email/set` second, both under the same call id
    // (RFC 8620 §3.2). Reading the primary response is what makes the helper see
    // the copy body; reading the tail used to parse the `Email/set` body as a
    // `CopyResponse` and fail a successful operation with `Error::Parse`
    // (Findings #40/#49).
    mount_jmap(
        &server,
        jmap_response(
            vec![
                inv(
                    "Email/copy",
                    "s0",
                    json!({
                        "fromAccountId": "ue150411c",
                        "accountId": ACCOUNT_ID,
                        "oldState": "7ee7e9263a6d",
                        "newState": "5a0d2447ed26",
                        "created": {
                            "src1": { "id": "dst1", "blobId": "B2", "threadId": "T2", "size": 10 }
                        }
                    }),
                ),
                inv(
                    "Email/set",
                    "s0",
                    json!({
                        "accountId": "ue150411c",
                        "oldState": "780842",
                        "newState": "780871",
                        "destroyed": [ "src1" ]
                    }),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let copied = client
        .email_copy("ue150411c", "src1", ["75a4c956"], None::<Keywords>, None)
        .await
        .expect("the implicit Email/set response must not shadow the copy result");

    assert_eq!(copied.id(), Some("dst1"));
    assert_eq!(copied.blob_id(), Some("B2"));
    assert_eq!(copied.thread_id(), Some("T2"));
    assert_eq!(copied.size(), 10);

    // The request really was the RFC's destroy-the-original shape, so the two
    // responses are an answer to this call and not an unmatched fixture.
    let call = single_jmap_request(&server).await["methodCalls"][0].clone();
    assert_eq!(call[0], "Email/copy");
    assert_eq!(call[2], "s0", "the client's single call id");
}

// ---------------------------------------------------------------------------
// 9. Cross-cutting error paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn email_method_level_error_maps_to_error_method() {
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

    let err = client
        .email_query(None::<email_query::Filter>, None::<EmailSort>)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "expected Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Email/query"
    );
}

#[tokio::test]
async fn email_query_error_does_not_stop_a_paired_get_from_unwrapping() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![
                method_error("s0", "unsupportedFilter"),
                inv(
                    "Email/get",
                    "s1",
                    email_get_body(vec![rfc_email_get_example_record()], vec![]),
                ),
            ],
            SESSION_STATE,
        ),
    )
    .await;

    let mut request = client.build();
    request
        .query_email()
        .filter(email_query::Filter::text("nonsense"));
    request.get_email().ids(["f123u457"]);
    let mut response = request.send().await.unwrap();

    assert!(response.method_response_by_id("s0").unwrap().is_error());
    let get = response
        .method_response_by_pos(1)
        .unwrap_get_email()
        .unwrap();
    assert_eq!(get.list()[0].subject(), Some("Dinner on Thursday?"));
}

// ---------------------------------------------------------------------------
// 9. The `Request::send_import_email` / `send_parse_email` wrappers, and the
//    cold `EmailImport` builder edges (src/email/import.rs)
//
// Both wrappers are a thin `self.send_single().await`
// (src/email/helpers.rs:405-407, 418-420). The `Client::email_import*` and
// `Client::email_parse` helpers call `send_single` directly, so nothing else
// in the crate reaches them by name. `EmailImportRequest::if_in_state`,
// `EmailImport::mailbox_ids_ref` and the "id in neither map" arm of
// `EmailImportResponse::created` are only reachable through the
// `Request::import_email()` builder, which the client helpers never drive
// with those members set.
// ---------------------------------------------------------------------------

/// The blob the import fixtures reference (RFC 8621 §4.8 example shape).
const IMPORT_BLOB_ID: &str = "Gf40b5f831efa7233b9eb1c7f8f97d84eeeee64f7";

/// A `Email/import` response that created `i0` as `object` (RFC 8621 §4.8:
/// `created` carries only `{id, blobId, threadId, size}`, Finding #50).
fn import_created(object: Value) -> Value {
    json!({
        "accountId": ACCOUNT_ID,
        "oldState": "780839",
        "newState": "780840",
        "created": { "i0": object }
    })
}

/// The `created` record of the RFC §4.8 example.
fn imported_email() -> Value {
    json!({
        "id": "Mf40b5f831efa7233b9eb1c7f",
        "blobId": IMPORT_BLOB_ID,
        "threadId": "Td957e72e89f516dc",
        "size": 359
    })
}

#[tokio::test]
async fn send_import_email_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/import", import_created(imported_email())).await;

    let mut request = client.build();
    request
        .import_email()
        .email(IMPORT_BLOB_ID)
        .mailbox_ids(["2ea1ca41b38e"])
        .keywords(["$seen"])
        .received_at(1_531_184_708);
    let mut response = request.send_import_email().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.old_state(), Some("780839"));
    assert_eq!(response.new_state(), "780840");
    let email = response.created("i0").expect("the imported email");
    assert_eq!(email.id(), Some("Mf40b5f831efa7233b9eb1c7f"));
    assert_eq!(email.thread_id(), Some("Td957e72e89f516dc"));
    assert_eq!(email.size(), 359);

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "Email/import",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    let args = &body["methodCalls"][0][1];
    assert_eq!(args["accountId"], ACCOUNT_ID);
    assert_eq!(
        args["emails"]["i0"],
        json!({
            "blobId": IMPORT_BLOB_ID,
            "mailboxIds": { "2ea1ca41b38e": true },
            "keywords": { "$seen": true },
            "receivedAt": "2018-07-10T01:05:08Z"
        })
    );
}

#[tokio::test]
async fn send_import_email_with_if_in_state_puts_the_member_on_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/import", import_created(imported_email())).await;

    // RFC 8620 §5.3: `ifInState` makes the whole set conditional on the
    // account still being at the state the client last saw.
    let mut request = client.build();
    request
        .import_email()
        .if_in_state("780839")
        .email(IMPORT_BLOB_ID)
        .mailbox_ids(["2ea1ca41b38e"]);
    let response = request.send_import_email().await.unwrap();
    assert_eq!(response.new_state(), "780840");

    let args = &single_jmap_request(&server).await["methodCalls"][0][1];
    assert_eq!(args["ifInState"], "780839");
    assert!(
        args["emails"]["i0"].get("keywords").is_some(),
        "`keywords` has no skip guard, so it rides as an empty map: {args}"
    );
    assert!(args["emails"]["i0"].get("receivedAt").is_none(), "{args}");
}

#[tokio::test]
async fn send_import_email_with_a_mailbox_ids_reference_replaces_the_literals() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(&server, "Email/import", import_created(imported_email())).await;

    // RFC 8620 §3.6: `#mailboxIds` stands in for the whole `mailboxIds`
    // argument, resolved by the server from an earlier call in the same
    // request. The client serialises the reference under the `#` key and
    // drops the literal map.
    let mut request = client.build();
    request
        .import_email()
        .email(IMPORT_BLOB_ID)
        .mailbox_ids(["stale"])
        .mailbox_ids_ref(ResultReference::new(Method::QueryMailbox, 0, "/ids"));
    let response = request.send_import_email().await.unwrap();
    assert_eq!(response.new_state(), "780840");

    let args = &single_jmap_request(&server).await["methodCalls"][0][1];
    let imported = &args["emails"]["i0"];
    assert!(
        imported.get("mailboxIds").is_none(),
        "the reference must have cleared the literal map: {imported}"
    );
    assert_eq!(
        imported["#mailboxIds"],
        json!({
            "resultOf": "s0",
            "name": "Mailbox/query",
            "path": "/ids"
        })
    );
}

#[tokio::test]
async fn send_import_email_created_of_an_unreported_id_is_an_internal_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // The server reported `i0` only; asking for `i1` matches neither `created`
    // nor `notCreated`, which is the crate's `Error::Internal` arm rather
    // than a SetError.
    mount_one(&server, "Email/import", import_created(imported_email())).await;

    let mut request = client.build();
    request
        .import_email()
        .email(IMPORT_BLOB_ID)
        .mailbox_ids(["2ea1ca41b38e"]);
    let mut response = request.send_import_email().await.unwrap();

    let err = response.created("i1").unwrap_err();
    assert!(
        matches!(err, Error::Internal(_)),
        "an id in neither map must be Error::Internal, got {err:?}"
    );
    assert!(err.to_string().contains("Id i1 not found."), "{err}");
    // The reported id is still there, and the id iterators agree.
    assert_eq!(
        response.created_ids().unwrap().cloned().collect::<Vec<_>>(),
        ["i0"]
    );
    assert!(response.not_created_ids().is_none());
}

#[tokio::test]
async fn send_import_email_reports_a_not_created_set_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/import",
        json!({
            "accountId": ACCOUNT_ID,
            "oldState": "780839",
            "newState": "780839",
            "notCreated": {
                "i0": {
                    "type": "alreadyExists",
                    "description": "That message is already imported"
                }
            }
        }),
    )
    .await;

    let mut request = client.build();
    request
        .import_email()
        .email(IMPORT_BLOB_ID)
        .mailbox_ids(["2ea1ca41b38e"]);
    let mut response = request.send_import_email().await.unwrap();

    let err = response.created("i0").unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::AlreadyExists);
    assert_eq!(err.description(), Some("That message is already imported"));
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Email/import"
    );
}

#[tokio::test]
async fn send_parse_email_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Email/parse",
        json!({
            "accountId": ACCOUNT_ID,
            "parsed": { IMPORT_BLOB_ID: rfc_email_get_example_record() },
            "notParsable": null,
            "notFound": null
        }),
    )
    .await;

    let mut request = client.build();
    request
        .parse_email()
        .blob_ids([IMPORT_BLOB_ID])
        .properties([Property::Subject])
        .body_properties([BodyProperty::PartId]);
    let mut response = request.send_parse_email().await.unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    let email = response.parsed(IMPORT_BLOB_ID).expect("the parsed email");
    assert_eq!(email.subject(), Some("Dinner on Thursday?"));

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "Email/parse",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    let args = &body["methodCalls"][0][1];
    assert_eq!(args["blobIds"], json!([IMPORT_BLOB_ID]));
    assert_eq!(args["properties"], json!(["subject"]));
    assert_eq!(args["bodyProperties"], json!(["partId"]));
}

#[tokio::test]
async fn send_parse_email_surfaces_a_method_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "invalidArguments")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request.parse_email().blob_ids([IMPORT_BLOB_ID]);
    let err = request.send_parse_email().await.unwrap_err();

    assert!(
        matches!(err, Error::Method(_)),
        "the wrapper must propagate an `error` Invocation as Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "Email/parse"
    );
}
