//! P6 — blob-domain integration tests: **upload / download / Blob/copy**.
//!
//! Drives the `Client` blob methods from `src/blob/{upload,download,copy}.rs`
//! and the `Request::{copy_blob,send_copy_blob}` builders against a `wiremock`
//! HTTP server using the shared fixtures in `tests/common/mod.rs`.
//!
//! Unlike the method-call domains, the upload and download legs are *plain
//! HTTP*: RFC 8620 §6.1 and §6.2 put them on the Session's `uploadUrl` and
//! `downloadUrl` templates rather than behind `POST /jmap`, so these tests
//! mount those routes directly and assert the templating, the headers and the
//! raw octets — not a `methodCalls` array. `Blob/copy` (§6.3) *is* a normal
//! method call, so it is asserted the usual way.
//!
//! Coverage targets: `blob/upload.rs` (URL templating, the Content-Type
//! default, `UploadResponse` accessors and `take_blob_id`), `blob/download.rs`
//! (the four template parameters, the `name`/`type` values the client
//! substitutes, and the Content-Type header it strips for the GET),
//! `blob/copy.rs` (request members, the `copied`/`notCopied` maps and their
//! accessors) and `blob/helpers.rs`.

// Written against the async API: enabling `blocking` turns every `maybe_async` call synchronous, so this file is compiled out.
#![cfg(all(feature = "async", not(feature = "blocking")))]
mod common;

use common::{
    connected_client, jmap_response, mount_jmap, single_jmap_request, ACCOUNT_ID, SESSION_STATE,
};
use jmap_client::core::set::SetErrorType;
use jmap_client::Error;
use serde_json::{json, Value};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A blob id used across the fixtures.
const BLOB_ID: &str = "S0a1b2c3d4e5f6789";

/// Another account, used by the copy tests (RFC 8620 §6.3 copies *between*
/// accounts).
const OTHER_ACCOUNT_ID: &str = "u10987654";

// ---------------------------------------------------------------------------
// Fixtures
//
// A JMAP method response is an *Invocation* — `[name, arguments, callId]`
// (RFC 8620 §3.2) — so every method-call body below is wrapped with [`inv`].
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

/// Answers `POST /upload/{account}` with the RFC 8620 §6.1 upload response.
async fn mount_upload(server: &MockServer, account: &str, blob_id: &str, type_: &str, size: usize) {
    Mock::given(method("POST"))
        .and(path(format!("/upload/{account}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "accountId": account,
            "blobId": blob_id,
            "type": type_,
            "size": size
        })))
        .mount(server)
        .await
}

/// Answers the templated download URL for `blob_id` with `bytes`.
async fn mount_download(server: &MockServer, blob_id: &str, bytes: &[u8], content_type: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/download/{ACCOUNT_ID}/{blob_id}/none")))
        .and(query_param("accept", "application/octet-stream"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), content_type),
        )
        .mount(server)
        .await
}

/// Every request the server recorded, owned, so it can be filtered after the
/// await without fighting the borrow checker.
async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap()
}

/// The method-call arguments of the only JMAP request made so far.
async fn only_call(server: &MockServer) -> Value {
    single_jmap_request(server).await["methodCalls"][0][1].clone()
}

// ---------------------------------------------------------------------------
// 1. Upload (RFC 8620 §6.1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upload_returns_the_blob_metadata_and_puts_the_octets_on_the_wire() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    let payload = b"require \"fileinto\";\r\nfileinto \"INBOX.target\";\r\n";
    mount_upload(&server, ACCOUNT_ID, BLOB_ID, "application/sieve", payload.len()).await;

    let mut response = client
        .upload(None, payload.to_vec(), Some("application/sieve"))
        .await
        .unwrap();

    assert_eq!(response.account_id(), ACCOUNT_ID);
    assert_eq!(response.blob_id(), BLOB_ID);
    assert_eq!(response.content_type(), "application/sieve");
    assert_eq!(response.size(), payload.len());
    assert_eq!(response.take_blob_id(), BLOB_ID);
    // `take_blob_id` empties the field.
    assert_eq!(response.blob_id(), "");

    // The upload went to the templated URL with the raw body and the
    // Content-Type the caller asked for.
    let uploads: Vec<wiremock::Request> = received(&server)
        .await
        .into_iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path().starts_with("/upload/"))
        .collect();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].body, payload);
    assert_eq!(
        uploads[0].headers["Content-Type"].to_str().unwrap(),
        "application/sieve"
    );
    assert_eq!(uploads[0].url.path(), format!("/upload/{ACCOUNT_ID}"));
}

#[tokio::test]
async fn upload_defaults_the_content_type_to_octet_stream() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_upload(&server, ACCOUNT_ID, BLOB_ID, "application/octet-stream", 4).await;

    let response = client.upload(None, vec![1, 2, 3, 4], None).await.unwrap();

    assert_eq!(response.blob_id(), BLOB_ID);
    let uploads: Vec<wiremock::Request> = received(&server)
        .await
        .into_iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path().starts_with("/upload/"))
        .collect();
    assert_eq!(
        uploads[0].headers["Content-Type"].to_str().unwrap(),
        "application/octet-stream",
        "RFC 8620 §6.1: the type the server reports is the type that was sent"
    );
}

#[tokio::test]
async fn upload_targets_an_explicit_account_when_one_is_given() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_upload(&server, OTHER_ACCOUNT_ID, BLOB_ID, "text/plain", 3).await;

    let response = client
        .upload(Some(OTHER_ACCOUNT_ID), b"abc".to_vec(), Some("text/plain"))
        .await
        .unwrap();

    assert_eq!(response.account_id(), OTHER_ACCOUNT_ID);
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .any(|r| r.url.path() == format!("/upload/{OTHER_ACCOUNT_ID}")));
}

#[tokio::test]
async fn an_upload_problem_document_surfaces_as_error_problem() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §6.1: "the server SHOULD return a JSON problem details object
    // as the response body, as per [RFC7807]" — and the client byte-compares
    // the content type, so no charset suffix (Findings #7).
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
        .upload(None, vec![0u8; 10], Some("application/octet-stream"))
        .await
        .unwrap_err();

    let Error::Problem(problem) = err else {
        panic!("expected a Problem error, got {err:?}");
    };
    assert_eq!(problem.title(), Some("TOO_LARGE"));
}

#[tokio::test]
async fn an_upload_error_without_a_problem_content_type_degrades_to_server_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    Mock::given(method("POST"))
        .and(path(format!("/upload/{ACCOUNT_ID}")))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let err = client
        .upload(None, vec![0u8; 10], None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Server(_)),
        "a non-problem error body must degrade to Error::Server, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. Download (RFC 8620 §6.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn download_returns_the_raw_bytes() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    let payload = b"require [\"fileinto\"];\r\nfileinto \"INBOX.target\";\r\n";
    mount_download(&server, BLOB_ID, payload, "application/sieve").await;

    let bytes = client.download(BLOB_ID).await.unwrap();

    assert_eq!(bytes, payload);

    // The client fills the `name` template parameter with "none" and the
    // `type` parameter with "application/octet-stream" (it has no metadata
    // for a bare blobId), so that is the URL that must have been requested.
    let downloads: Vec<wiremock::Request> = received(&server)
        .await
        .into_iter()
        .filter(|r| r.method.as_str() == "GET" && r.url.path().starts_with("/download/"))
        .collect();
    assert_eq!(downloads.len(), 1);
    assert_eq!(
        downloads[0].url.path(),
        format!("/download/{ACCOUNT_ID}/{BLOB_ID}/none")
    );
    assert_eq!(
        downloads[0]
            .url
            .query_pairs()
            .find(|(k, _)| k == "accept")
            .map(|(_, v)| v.into_owned()),
        Some("application/octet-stream".to_string())
    );
}

#[tokio::test]
async fn a_download_problem_document_surfaces_as_error_problem() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    Mock::given(method("GET"))
        .and(path(format!("/download/{ACCOUNT_ID}/missing/none")))
        .and(query_param("accept", "application/octet-stream"))
        .respond_with(ResponseTemplate::new(404).set_body_raw(
            json!({
                "type": "https://www.rfc-editor.org/rfc/rfc8620#section-6.2",
                "title": "NOT_FOUND",
                "detail": "No such blob"
            })
            .to_string(),
            "application/problem+json",
        ))
        .mount(&server)
        .await;

    let err = client.download("missing").await.unwrap_err();
    let Error::Problem(problem) = err else {
        panic!("expected a Problem error, got {err:?}");
    };
    assert_eq!(problem.title(), Some("NOT_FOUND"));
}

// ---------------------------------------------------------------------------
// 3. Blob/copy (RFC 8620 §6.3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blob_copy_returns_the_new_blob_id() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Blob/copy",
        json!({
            "fromAccountId": OTHER_ACCOUNT_ID,
            "accountId": ACCOUNT_ID,
            "copied": { BLOB_ID: "S9z8y7x6w5v4u3210" }
        }),
    )
    .await;

    let new_id = client.blob_copy(OTHER_ACCOUNT_ID, BLOB_ID).await.unwrap();

    assert_eq!(new_id, "S9z8y7x6w5v4u3210");

    let body = single_jmap_request(&server).await;
    assert_eq!(body["methodCalls"][0][0], "Blob/copy");
    assert!(body["using"]
        .as_array()
        .unwrap()
        .contains(&json!("urn:ietf:params:jmap:blob")));

    let args = &body["methodCalls"][0][1];
    assert_eq!(args["fromAccountId"], OTHER_ACCOUNT_ID);
    assert_eq!(args["accountId"], ACCOUNT_ID);
    assert_eq!(args["blobIds"], json!([BLOB_ID]));
}

#[tokio::test]
async fn blob_copy_reports_a_not_found_blob() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    // RFC 8620 §6.3: "the `notFound` SetError may be returned if the blobId
    // to be copied cannot be found."
    mount_one(
        &server,
        "Blob/copy",
        json!({
            "fromAccountId": OTHER_ACCOUNT_ID,
            "accountId": ACCOUNT_ID,
            "notCopied": {
                BLOB_ID: {
                    "type": "notFound",
                    "description": "The blob does not exist"
                }
            }
        }),
    )
    .await;

    let err = client.blob_copy(OTHER_ACCOUNT_ID, BLOB_ID).await.unwrap_err();
    let Error::Set(err) = err else {
        panic!("expected a Set error, got {err:?}");
    };
    assert_eq!(err.error(), &SetErrorType::NotFound);
    assert_eq!(err.description(), Some("The blob does not exist"));
}

#[tokio::test]
async fn blob_copy_of_an_unreported_id_is_an_internal_error() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Blob/copy",
        json!({ "fromAccountId": OTHER_ACCOUNT_ID, "accountId": ACCOUNT_ID }),
    )
    .await;

    let err = client.blob_copy(OTHER_ACCOUNT_ID, BLOB_ID).await.unwrap_err();
    assert!(
        matches!(err, Error::Internal(_)),
        "an id in neither `copied` nor `notCopied` must be Internal, got {err:?}"
    );
}

#[tokio::test]
async fn copy_blob_via_the_request_builder_copies_several_blobs() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_one(
        &server,
        "Blob/copy",
        json!({
            "fromAccountId": OTHER_ACCOUNT_ID,
            "accountId": ACCOUNT_ID,
            "copied": {
                "BlobA": "CopyA",
                "BlobB": "CopyB"
            },
            "notCopied": {
                "BlobC": { "type": "notFound" }
            }
        }),
    )
    .await;

    let mut request = client.build();
    request
        .copy_blob(OTHER_ACCOUNT_ID)
        .blob_id("BlobA")
        .blob_id("BlobB")
        .blob_id("BlobC");
    let mut response = request.send_copy_blob().await.unwrap();

    assert_eq!(response.from_account_id(), OTHER_ACCOUNT_ID);
    assert_eq!(response.account_id(), ACCOUNT_ID);

    let copied_keys: Vec<String> = response.copied_ids().unwrap().cloned().collect();
    let mut copied: Vec<(String, String)> = copied_keys
        .into_iter()
        .map(|key| {
            let new_id = response.copied(&key).unwrap();
            (key, new_id)
        })
        .collect();
    copied.sort();
    assert_eq!(
        copied,
        vec![
            ("BlobA".to_string(), "CopyA".to_string()),
            ("BlobB".to_string(), "CopyB".to_string())
        ]
    );

    assert_eq!(response.not_copied_ids().unwrap().collect::<Vec<_>>(), ["BlobC"]);
    assert_eq!(
        response.not_copied_reason("BlobC").unwrap().error(),
        &SetErrorType::NotFound
    );

    let args = only_call(&server).await;
    assert_eq!(args["blobIds"], json!(["BlobA", "BlobB", "BlobC"]));
}

#[tokio::test]
async fn a_method_level_error_short_circuits_the_copy() {
    let server = MockServer::start().await;
    let client = connected_client(&server).await;
    mount_jmap(
        &server,
        jmap_response(
            vec![method_error("s0", "fromAccountNotFound")],
            SESSION_STATE,
        ),
    )
    .await;

    let err = client.blob_copy(OTHER_ACCOUNT_ID, BLOB_ID).await.unwrap_err();
    assert!(
        matches!(err, Error::Method(_)),
        "an `error` Invocation must surface as Error::Method, got {err:?}"
    );
}
