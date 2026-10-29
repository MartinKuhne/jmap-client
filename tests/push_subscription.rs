//! P6 — push-domain integration tests: **PushSubscription**.
//!
//! Drives the `Client`/`Request` PushSubscription helpers from
//! `src/push_subscription/helpers.rs` against a `wiremock` HTTP server using
//! the shared fixtures in `tests/common/mod.rs`. Canned payloads are the
//! example bodies of RFC 8620 §7.2.3 wherever the RFC has one, with the
//! section cited on the fixture.
//!
//! Coverage targets: `push_subscription/helpers.rs` (every
//! `Client::push_subscription_*` helper plus the
//! `Request::{get,set}_push_subscription` builders), `push_subscription/get.rs`
//! (record accessors and the URL-safe base64 `Keys` decode),
//! `push_subscription/set.rs` (patch keys, `Keys::new` encode) and
//! `push_subscription/mod.rs` (`Property`).
//!
//! Push subscriptions are the one core type that is *not* tied to an account
//! (RFC 8620 §7.2.1/§7.2.2), so these tests also pin down that no
//! `accountId` rides the request — which is why there is no `Client`-level
//! `push_subscription_get` helper and the get leg goes through `Request`.

mod common;

use common::{
    connected_client, jmap_response, mount_jmap, single_jmap_request, SESSION_STATE,
};
use jmap_client::core::set::{SetErrorType, SetObject};
use jmap_client::push_subscription::{Keys, Property, PushSubscription};
use jmap_client::{DataType, Error, Get};
use serde_json::{json, Value};
use wiremock::MockServer;

/// The RFC's own `deviceClientId` from the §7.2.3 example.
const DEVICE_CLIENT_ID: &str = "a889-ffea-910";

/// The RFC's push endpoint from the §7.2.3 example.
const PUSH_URL: &str = "https://example.com/push/?device=X8980fc&client=12c6d086";

/// The subscription id the RFC's create returns.
const SUBSCRIPTION_ID: &str = "P43dcfa4-1dd4-41ef-9156-2c89b3b19c60";

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

/// A `PushSubscription/get` result envelope.
///
/// RFC 8620 §7.2.1 says the method does *not* return a `state` argument, but
/// the crate's generic `GetResponse.state` is a required `String`, so the
/// fixture must supply one or the response will not deserialise (recorded as a
/// Finding).
fn push_get_body(list: Vec<Value>, not_found: Vec<&str>) -> Value {
    json!({
        "state": "0",
        "list": list,
        "notFound": not_found
    })
}

/// RFC 8620 §7.2.3 — the two subscriptions the opening `PushSubscription/get`
/// of the example returns.
///
/// One deviation: the RFC's first subscription has `"types": [ "Todo" ]`, but
/// this crate's `DataType` is a *closed* enum with no catch-all variant, so an
/// application-defined type name fails to deserialise. `Mailbox` stands in for
/// `Todo` here (recorded as a Finding).
fn rfc_example_subscriptions() -> Vec<Value> {
    vec![
        json!({
            "id": "e50b2c1d-9553-41a3-b0a7-a7d26b599ee1",
            "deviceClientId": "b37ff8001ca0",
            "verificationCode": "b210ef734fe5f439c1ca386421359f7b",
            "expires": "2018-07-31T00:13:21Z",
            "types": [ "Mailbox" ]
        }),
        json!({
            "id": "f2d0aab5-e976-4e8b-ad4b-b380a5b987e4",
            "deviceClientId": "X8980fc",
            "verificationCode": "f3d4618a9ae15c8b7f5582533786d531",
            "expires": "2018-07-12T05:55:00Z",
            "types": [ "Mailbox", "Email", "EmailDelivery" ]
        }),
    ]
}

/// A `PushSubscription/set` response that created `c0` as `object`
/// (RFC 8620 §5.3).
fn created(object: Value) -> Value {
    json!({ "created": { "c0": object } })
}

/// A `PushSubscription/set` response that updated [`SUBSCRIPTION_ID`] to
/// `object` (RFC 8620 §5.3).
fn updated(object: Value) -> Value {
    json!({ "updated": { SUBSCRIPTION_ID: object } })
}

/// The method-call arguments of the only JMAP request made so far.
async fn only_call(server: &MockServer) -> Value {
    single_jmap_request(server).await["methodCalls"][0][1].clone()
}

// ---------------------------------------------------------------------------
// 1. PushSubscription/get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_subscription_get_returns_the_rfc_example_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/get",
        push_get_body(rfc_example_subscriptions(), vec![]),
    )
    .await;

    let mut request = client.build();
    request.get_push_subscription().ids(["e50b2c1d-9553-41a3-b0a7-a7d26b599ee1"]);
    let mut response = request.send_get_push_subscription().await.unwrap();

    assert_eq!(response.list().len(), 2);
    assert!(response.not_found().is_empty());
    // RFC 8620 §7.2.1: no accountId is returned for push subscriptions, and
    // the crate models it as an Option, so the RFC's omission parses cleanly.
    assert_eq!(response.account_id(), None);

    let first = &response.list()[0];
    assert_eq!(first.id(), Some("e50b2c1d-9553-41a3-b0a7-a7d26b599ee1"));
    assert_eq!(first.device_client_id(), Some("b37ff8001ca0"));
    assert_eq!(
        first.verification_code(),
        Some("b210ef734fe5f439c1ca386421359f7b")
    );
    assert_eq!(first.expires(), Some(1_532_996_001));
    assert_eq!(first.types(), Some(&[DataType::Mailbox][..]));

    let second = &response.list()[1];
    assert_eq!(second.device_client_id(), Some("X8980fc"));
    assert_eq!(second.expires(), Some(1_531_374_900));
    assert_eq!(
        second.types().unwrap(),
        [DataType::Mailbox, DataType::Email, DataType::EmailDelivery]
    );

    // The method name went on the wire and no account was addressed.
    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "PushSubscription/get");
    assert!(body["using"]
        .as_array()
        .unwrap()
        .contains(&json!("urn:ietf:params:jmap:core")));
    assert!(
        body["methodCalls"][0][1].get("accountId").is_none(),
        "RFC 8620 §7.2.1: the request must not carry an accountId: {body}"
    );
    assert_eq!(response.take_state(), "0");
}

#[tokio::test]
async fn push_subscription_get_decodes_the_url_safe_base64_keys() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §7.2: `keys.p256dh` / `keys.auth` are URL-safe base64
    // (RFC 4648). 0xFB 0xFF 0xFE encodes to `-__-` URL-safe (`++//` in the
    // standard alphabet), so this fixture pins the alphabet, not just the shape.
    mount_one(
        &server,
        "PushSubscription/get",
        push_get_body(
            vec![json!({
                "id": SUBSCRIPTION_ID,
                "deviceClientId": DEVICE_CLIENT_ID,
                "url": PUSH_URL,
                "keys": { "p256dh": "-__-", "auth": "-__-" }
            })],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request
        .get_push_subscription()
        .properties([Property::Id, Property::Keys]);
    let response = request.send_get_push_subscription().await.unwrap();

    let keys = response.list()[0].keys().expect("keys must be present");
    assert_eq!(keys.p256dh().unwrap(), vec![0xFB, 0xFF, 0xFE]);
    assert_eq!(keys.auth().unwrap(), vec![0xFB, 0xFF, 0xFE]);

    assert_eq!(
        response.list()[0].url(),
        Some(PUSH_URL),
        "the crate exposes `url` even though RFC 8620 §7.2.1 forbids a server \
         from returning it — the client is permissive on read"
    );
}

#[tokio::test]
async fn a_subscription_without_keys_reads_as_none() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/get",
        push_get_body(
            vec![json!({
                "id": SUBSCRIPTION_ID,
                "deviceClientId": DEVICE_CLIENT_ID,
                "verificationCode": "abc"
            })],
            vec![],
        ),
    )
    .await;

    let mut request = client.build();
    request.get_push_subscription().ids([SUBSCRIPTION_ID]);
    let response = request.send_get_push_subscription().await.unwrap();

    let sub = &response.list()[0];
    assert!(sub.keys().is_none());
    assert!(sub.types().is_none(), "`types` absent means push everything");
    assert!(sub.expires().is_none());
}

// ---------------------------------------------------------------------------
// 2. PushSubscription/set — create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_subscription_create_sends_the_rfc_example_create() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §7.2.3 — the server creates the subscription but caps the
    // expiry at 7 days.
    mount_one(
        &server,
        "PushSubscription/set",
        created(json!({
            "id": SUBSCRIPTION_ID,
            "keys": null,
            "expires": "2018-07-13T02:14:29Z"
        })),
    )
    .await;

    let sub = client
        .push_subscription_create(DEVICE_CLIENT_ID, PUSH_URL, None)
        .await
        .unwrap();

    assert_eq!(sub.id(), Some(SUBSCRIPTION_ID));
    assert_eq!(sub.expires(), Some(1_531_448_069));
    assert!(sub.keys().is_none());

    let args = only_call(&server).await;
    assert_eq!(
        args["create"]["c0"],
        json!({ "deviceClientId": DEVICE_CLIENT_ID, "url": PUSH_URL })
    );
    assert!(
        args.get("accountId").is_none(),
        "RFC 8620 §7.2.2: the set request must not carry an accountId: {args}"
    );
    assert!(
        args.get("ifInState").is_none(),
        "RFC 8620 §7.2.2: no ifInState for push subscriptions: {args}"
    );
}

#[tokio::test]
async fn push_subscription_create_encodes_keys_with_the_url_safe_alphabet() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        created(json!({ "id": SUBSCRIPTION_ID })),
    )
    .await;

    client
        .push_subscription_create(
            DEVICE_CLIENT_ID,
            PUSH_URL,
            Some(Keys::new(&[0xFB, 0xFF, 0xFE], &[0xFB, 0xFF, 0xFE])),
        )
        .await
        .unwrap();

    let wire_keys = only_call(&server).await["create"]["c0"]["keys"].clone();
    assert_eq!(
        wire_keys,
        json!({ "p256dh": "-__-", "auth": "-__-" }),
        "RFC 8291 requires the URL-safe base64 alphabet, not the standard one"
    );

    // And the same bytes come back out through the read-side accessors.
    let round_tripped: PushSubscription<Get> = serde_json::from_value(json!({
        "id": SUBSCRIPTION_ID,
        "keys": wire_keys
    }))
    .unwrap();
    assert_eq!(
        round_tripped.keys().unwrap().p256dh().unwrap(),
        vec![0xFB, 0xFF, 0xFE]
    );
    assert_eq!(
        round_tripped.keys().unwrap().auth().unwrap(),
        vec![0xFB, 0xFF, 0xFE]
    );
}

#[tokio::test]
async fn push_subscription_create_reports_a_rejected_create() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        json!({
            "notCreated": {
                "c0": {
                    "type": "invalidProperties",
                    "description": "The push URL must use https",
                    "properties": ["url"]
                }
            }
        }),
    )
    .await;

    let err = client
        .push_subscription_create(DEVICE_CLIENT_ID, "http://insecure.example/", None)
        .await
        .unwrap_err();

    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::InvalidProperties);
    assert_eq!(err.properties().unwrap(), ["url"]);
}

// ---------------------------------------------------------------------------
// 3. PushSubscription/set — verify / update types / destroy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_subscription_verify_patches_the_verification_code() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §7.2.3 — the client echoes back the code the server pushed in
    // its PushVerification object.
    mount_one(
        &server,
        "PushSubscription/set",
        updated(json!({ "verificationCode": "da1f097b11ca17f06424e30bf02bfa67" })),
    )
    .await;

    let sub = client
        .push_subscription_verify(SUBSCRIPTION_ID, "da1f097b11ca17f06424e30bf02bfa67")
        .await
        .unwrap()
        .expect("the updated record must come back");

    assert_eq!(
        sub.verification_code(),
        Some("da1f097b11ca17f06424e30bf02bfa67")
    );
    assert_eq!(
        only_call(&server).await["update"][SUBSCRIPTION_ID],
        json!({ "verificationCode": "da1f097b11ca17f06424e30bf02bfa67" })
    );
}

#[tokio::test]
async fn push_subscription_verify_rejects_a_wrong_code() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §7.2.2: "Attempts to update the subscription with an invalid
    // verification code MUST be rejected with an invalidProperties SetError."
    mount_one(
        &server,
        "PushSubscription/set",
        json!({
            "notUpdated": {
                SUBSCRIPTION_ID: {
                    "type": "invalidProperties",
                    "description": "Wrong verification code"
                }
            }
        }),
    )
    .await;

    let err = client
        .push_subscription_verify(SUBSCRIPTION_ID, "0000000000000000")
        .await
        .unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::InvalidProperties);
    assert_eq!(err.description(), Some("Wrong verification code"));
}

#[tokio::test]
async fn push_subscription_update_types_sends_the_type_list() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        updated(json!({ "types": ["Mailbox", "Email"] })),
    )
    .await;

    let sub = client
        .push_subscription_update_types(
            SUBSCRIPTION_ID,
            Some(vec![DataType::Mailbox, DataType::Email]),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(sub.types().unwrap(), [DataType::Mailbox, DataType::Email]);
    assert_eq!(
        only_call(&server).await["update"][SUBSCRIPTION_ID],
        json!({ "types": ["Mailbox", "Email"] })
    );
}

#[tokio::test]
async fn push_subscription_update_types_to_none_asks_for_every_type() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        updated(json!({ "types": null })),
    )
    .await;

    client
        .push_subscription_update_types(SUBSCRIPTION_ID, None::<Vec<DataType>>)
        .await
        .unwrap();

    // RFC 8620 §7.2: `types: null` means "push all types", so the null must
    // actually ride the wire rather than being skipped.
    assert_eq!(
        only_call(&server).await["update"][SUBSCRIPTION_ID],
        json!({ "types": null })
    );
}

#[tokio::test]
async fn push_subscription_destroy_removes_the_subscription() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        json!({ "destroyed": [SUBSCRIPTION_ID] }),
    )
    .await;

    client
        .push_subscription_destroy(SUBSCRIPTION_ID)
        .await
        .unwrap();

    let args = only_call(&server).await;
    assert_eq!(args["destroy"], json!([SUBSCRIPTION_ID]));
    assert!(args.get("accountId").is_none(), "{args}");
}

#[tokio::test]
async fn push_subscription_destroy_reports_not_destroyed() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "PushSubscription/set",
        json!({
            "notDestroyed": {
                SUBSCRIPTION_ID: { "type": "notFound" }
            }
        }),
    )
    .await;

    let err = client
        .push_subscription_destroy(SUBSCRIPTION_ID)
        .await
        .unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::NotFound);
}

// ---------------------------------------------------------------------------
// 4. Method-level errors
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_method_level_error_short_circuits_the_request() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "cannotCalculateChanges")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request.get_push_subscription().ids([SUBSCRIPTION_ID]);
    let err = request.send_get_push_subscription().await.unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "an `error` Invocation must surface as Error::Method, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. Wire names
// ---------------------------------------------------------------------------

#[test]
fn push_subscription_property_wire_names() {
    for (property, wire) in [
        (Property::Id, "id"),
        (Property::DeviceClientId, "deviceClientId"),
        (Property::Url, "url"),
        (Property::Keys, "keys"),
        (Property::VerificationCode, "verificationCode"),
        (Property::Expires, "expires"),
        (Property::Types, "types"),
    ] {
        assert_eq!(serde_json::to_value(property).unwrap(), json!(wire));
        assert_eq!(property.to_string(), wire);
        assert_eq!(
            serde_json::from_value::<Property>(json!(wire)).unwrap(),
            property
        );
    }
}

#[test]
fn a_push_subscription_carries_only_the_seven_rfc_members() {
    // RFC 8620 §7.2 lists exactly these seven properties; nothing else may
    // leak into the create/update patch.
    let mut sub: PushSubscription<jmap_client::Set> = PushSubscription::new(None);
    sub.device_client_id("d")
        .url("https://x")
        .verification_code("v")
        .expires(1_531_448_069)
        .keys(Keys::new(&[1], &[2]))
        .types(Some(vec![DataType::Email]));
    let mut keys: Vec<String> = serde_json::to_value(&sub)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "deviceClientId",
            "expires",
            "keys",
            "types",
            "url",
            "verificationCode"
        ]
    );
}

// ---------------------------------------------------------------------------
// The `Request::send_set_push_subscription` convenience wrapper
//
// `Request::send_set_push_subscription` is a thin `self.send_single().await`
// (src/push_subscription/helpers.rs:116-118). The `Client::push_subscription_*`
// helpers call `send_single` directly, so nothing else in the crate reaches the
// wrapper by name. The test builds the patch through the typed builder, sends
// it through the wrapper, asserts the parsed result, and spies on the wire so
// it cannot pass on an empty `methodCalls` array (Finding #31).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_set_push_subscription_sends_the_call_and_parses_the_response() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §7.2.3 — verifying a subscription patches `verificationCode`.
    mount_one(
        &server,
        "PushSubscription/set",
        updated(json!({ "verificationCode": "12345678" })),
    )
    .await;

    let mut request = client.build();
    request
        .set_push_subscription()
        .update(SUBSCRIPTION_ID)
        .verification_code("12345678");
    let mut response = request.send_set_push_subscription().await.unwrap();

    // RFC 8620 §7.3: like PushSubscription/get, the set method carries no
    // accountId, and the crate models it as an Option, so the omission
    // parses cleanly as None.
    assert_eq!(response.account_id(), None);
    let subscription = response
        .updated(SUBSCRIPTION_ID)
        .unwrap()
        .expect("the verified subscription");
    assert_eq!(subscription.verification_code(), Some("12345678"));

    let body = single_jmap_request(&server).await;
    assert_eq!(
        body["methodCalls"][0][0],
        "PushSubscription/set",
        "the wrapper must put the method name on the wire"
    );
    assert_eq!(body["methodCalls"][0][2], "s0");
    assert_eq!(
        body["methodCalls"][0][1]["update"][SUBSCRIPTION_ID],
        json!({ "verificationCode": "12345678" })
    );
}

#[tokio::test]
async fn send_set_push_subscription_surfaces_a_method_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(vec![method_error("s0", "forbidden")], SESSION_STATE),
    )
    .await;

    let mut request = client.build();
    request
        .set_push_subscription()
        .update(SUBSCRIPTION_ID)
        .verification_code("00000000");
    let err = request.send_set_push_subscription().await.unwrap_err();

    assert!(
        matches!(err, Error::Method(_)),
        "the wrapper must propagate an `error` Invocation as Error::Method, got {err:?}"
    );
    assert_eq!(
        single_jmap_request(&server).await["methodCalls"][0][0],
        "PushSubscription/set"
    );
}
