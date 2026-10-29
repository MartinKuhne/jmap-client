//! Self-tests for the mock: they drive the server over a plain TCP socket with a
//! hand-written HTTP/1.1 client, so the mock's wire behaviour is verified
//! independently of `jmap-client`.
//!
//! These are the guarantees later integration phases build on:
//! * the session resource advertises every capability block the client models;
//! * `POST /jmap` validates `using`, keeps call order, resolves result
//!   references and echoes `createdIds`;
//! * failures use the right *kind* of error — a problem document for transport
//!   level failures, an in-band `["error", ...]` for method failures;
//! * blob upload/download follow RFC 8620 sections 5.1/5.2.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use mock_jmap::session::{ACCOUNT_ID, MAX_SIZE_UPLOAD};
use mock_jmap::MockJmapServer;
use serde_json::{json, Value};

const AUTH: &str = "Basic dGVzdEBleGFtcGxlLm9yZzpzZWNyZXQ=";
const CORE: &str = "urn:ietf:params:jmap:core";
const MAIL: &str = "urn:ietf:params:jmap:mail";

// ---------------------------------------------------------------------------
// tiny HTTP/1.1 client
// ---------------------------------------------------------------------------

fn address(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

fn build_request(method: &str, path: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    raw.push_str("\r\n");
    let mut out = raw.into_bytes();
    out.extend_from_slice(body);
    out
}

fn split_response(raw: &[u8]) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let pos = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("response has no header terminator");
    let head = std::str::from_utf8(&raw[..pos]).expect("response head is utf-8");
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u16>().ok())
        .expect("status code");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    (status, headers, raw[pos + 4..].to_vec())
}

/// Sends `raw` and returns the full response.
fn send(port: u16, raw: &[u8]) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let mut stream = TcpStream::connect(address(port)).expect("connect to mock");
    stream.write_all(raw).expect("write request");
    stream.flush().expect("flush request");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("read response");
    split_response(&buf)
}

fn get(port: u16, path: &str) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    send(
        port,
        &build_request("GET", path, &[("Authorization", AUTH.into())], b""),
    )
}

/// `POST /jmap` with `using` + one or more method calls; returns the parsed body.
fn jmap(port: u16, using: &[&str], calls: Vec<Value>) -> (u16, Value) {
    let body = json!({
        "using": using,
        "methodCalls": calls,
    });
    let (status, headers, raw) = send(
        port,
        &build_request(
            "POST",
            "/jmap",
            &[
                ("Authorization", AUTH.into()),
                ("Content-Type", "application/json".into()),
            ],
            body.to_string().as_bytes(),
        ),
    );
    let content_type = headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or_default();
    assert!(
        content_type == "application/json" || content_type == "application/problem+json",
        "a JMAP response is JSON or a problem document, got {headers:?}"
    );
    (
        status,
        serde_json::from_slice(&raw).expect("JMAP response is valid JSON"),
    )
}

/// The `[name, args, callId]` triple of the nth method response.
fn nth(body: &Value, index: usize) -> (String, Value, String) {
    let entry = &body["methodResponses"][index];
    (
        entry[0].as_str().unwrap().to_string(),
        entry[1].clone(),
        entry[2].as_str().unwrap().to_string(),
    )
}

// ---------------------------------------------------------------------------
// session resource
// ---------------------------------------------------------------------------

#[test]
fn session_advertises_every_capability_block() {
    let server = MockJmapServer::start();
    let (status, headers, body) = get(server.port(), "/.well-known/jmap");
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/json");

    let session: Value = serde_json::from_slice(&body).unwrap();
    let caps = session["capabilities"].as_object().unwrap();
    for expected in [
        CORE,
        MAIL,
        "urn:ietf:params:jmap:submission",
        "urn:ietf:params:jmap:vacationresponse",
        "urn:ietf:params:jmap:websocket",
        "urn:ietf:params:jmap:sieve",
        "urn:ietf:params:jmap:principals",
        "urn:ietf:params:jmap:blob",
        "urn:ietf:params:jmap:quota",
        "urn:ietf:params:jmap:mdn",
    ] {
        assert!(caps.contains_key(expected), "missing {expected}");
    }

    // Core limits are real numbers the client reads.
    assert_eq!(caps[CORE]["maxSizeUpload"], json!(MAX_SIZE_UPLOAD));
    assert!(caps[CORE]["collationAlgorithms"]
        .as_array()
        .unwrap()
        .contains(&json!("i;unicode-casemap")));

    // Submission extensions are an *array of strings*, which is what
    // `jmap_client` deserialises into `Vec<String>`.
    assert_eq!(
        caps["urn:ietf:params:jmap:submission"]["submissionExtensions"],
        json!(["SIZE", "DSN", "UTF8ONLY"])
    );

    // WebSocket points at the mock's own endpoint.
    assert_eq!(
        caps["urn:ietf:params:jmap:websocket"]["url"],
        json!(server.ws_url())
    );
    assert_eq!(
        caps["urn:ietf:params:jmap:websocket"]["supportsPush"],
        json!(true)
    );

    // Urls carry the RFC's `{placeholder}` templates untouched.
    assert_eq!(
        session["apiUrl"],
        json!(format!("{}/jmap", server.base_url()))
    );
    assert_eq!(
        session["uploadUrl"],
        json!(format!("{}/upload/{{accountId}}", server.base_url()))
    );
    assert!(session["downloadUrl"]
        .as_str()
        .unwrap()
        .contains("/download/{accountId}/{blobId}/{name}"));
    assert!(session["eventSourceUrl"]
        .as_str()
        .unwrap()
        .contains("/eventSource?types={types}"));

    // One account, personal, primary for every capability that needs one.
    assert_eq!(session["username"], json!("test@example.org"));
    assert_eq!(session["accounts"][ACCOUNT_ID]["isPersonal"], json!(true));
    assert_eq!(
        session["primaryAccounts"][MAIL],
        json!(ACCOUNT_ID),
        "the fixture account is the primary mail account"
    );
    assert_eq!(session["state"], json!("1"));
}

// ---------------------------------------------------------------------------
// request envelope failures (problem documents)
// ---------------------------------------------------------------------------

#[test]
fn unauthenticated_request_is_401_problem() {
    let server = MockJmapServer::start();
    let (status, headers, body) = send(
        server.port(),
        &build_request("GET", "/.well-known/jmap", &[], b""),
    );
    assert_eq!(status, 401);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("unauthorized"));
    assert_eq!(doc["status"], json!(401));
}

#[test]
fn unknown_capability_is_rejected() {
    let server = MockJmapServer::start();
    let (status, body) = jmap(
        server.port(),
        &[CORE, "urn:example:not-a-real-capability"],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );
    assert_eq!(status, 400);
    assert_eq!(
        body["type"],
        json!("urn:ietf:params:jmap:error:unknownCapability"),
        "body: {body}"
    );
}

#[test]
fn garbage_body_is_not_json() {
    let server = MockJmapServer::start();
    let (status, headers, body) = send(
        server.port(),
        &build_request(
            "POST",
            "/jmap",
            &[
                ("Authorization", AUTH.into()),
                ("Content-Type", "application/json".into()),
            ],
            b"{ this is not json",
        ),
    );
    assert_eq!(status, 400);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("urn:ietf:params:jmap:error:notJSON"));
}

#[test]
fn valid_json_that_is_not_a_request_is_not_request() {
    let server = MockJmapServer::start();
    let (status, _headers, raw) = send(
        server.port(),
        &build_request(
            "POST",
            "/jmap",
            &[
                ("Authorization", AUTH.into()),
                ("Content-Type", "application/json".into()),
            ],
            br#"{"hello": "world"}"#,
        ),
    );
    let doc: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(status, 400);
    assert_eq!(doc["type"], json!("urn:ietf:params:jmap:error:notRequest"));
}

#[test]
fn unknown_route_is_404() {
    let server = MockJmapServer::start();
    let (status, headers, _) = get(server.port(), "/nope");
    assert_eq!(status, 404);
    assert_eq!(headers["content-type"], "application/problem+json");
}

#[test]
fn p2_routes_are_live_now() {
    let server = MockJmapServer::start();

    // A plain GET on the WebSocket endpoint is not a handshake; the upgrade is
    // intercepted before routing, so landing here means "you asked for /ws but
    // did not upgrade".
    let (status, headers, body) = get(server.port(), "/ws");
    assert_eq!(status, 400);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("notWebSocketUpgrade"));

    // The EventSource route answers with a stream now, but it still requires
    // authentication, and that failure is an ordinary problem document.
    let (status, headers, body) = send(
        server.port(),
        &build_request("GET", "/eventSource?types=*", &[], b""),
    );
    assert_eq!(status, 401);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("unauthorized"));
}

// ---------------------------------------------------------------------------
// method calls
// ---------------------------------------------------------------------------

#[test]
fn mailbox_get_round_trip() {
    let server = MockJmapServer::start();
    let (status, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );
    assert_eq!(status, 200);
    let (name, args, call_id) = nth(&body, 0);
    assert_eq!(name, "Mailbox/get");
    assert_eq!(call_id, "c0");
    assert_eq!(args["accountId"], json!(ACCOUNT_ID));

    let names: Vec<&str> = args["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mb| mb["name"].as_str().unwrap())
        .collect();
    for expected in ["Inbox", "Projects", "Drafts", "Sent", "Archive"] {
        assert!(names.contains(&expected), "missing mailbox {expected}");
    }
    assert_eq!(args["notFound"], json!([]));
    assert!(args["state"].is_string());
}

#[test]
fn get_reports_unknown_ids_in_not_found() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {
            "accountId": ACCOUNT_ID,
            "ids": ["mb_inbox", "mb_does_not_exist"]
        }, "c0"])],
    );
    let (_, args, _) = nth(&body, 0);
    assert_eq!(args["list"].as_array().unwrap().len(), 1);
    assert_eq!(args["notFound"], json!(["mb_does_not_exist"]));
}

#[test]
fn unknown_method_is_an_in_band_error_not_a_problem() {
    let server = MockJmapServer::start();
    let (status, body) = jmap(
        server.port(),
        &[CORE],
        vec![json!(["NoSuch/type", {}, "c0"])],
    );
    assert_eq!(status, 200, "method errors travel inside a 200 response");
    let (name, args, call_id) = nth(&body, 0);
    assert_eq!(name, "error");
    assert_eq!(call_id, "c0");
    assert_eq!(args["type"], json!("unknownMethod"));
}

#[test]
fn unknown_account_is_an_in_band_error() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": "nope"}, "c0"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "error");
    assert_eq!(args["type"], json!("accountNotFound"));
}

#[test]
fn call_order_and_ids_are_preserved() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![
            json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "s0"]),
            json!(["Thread/get", {"accountId": ACCOUNT_ID}, "s1"]),
            json!(["Email/get", {"accountId": ACCOUNT_ID}, "s2"]),
        ],
    );
    let responses = body["methodResponses"].as_array().unwrap();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0][2], json!("s0"));
    assert_eq!(responses[1][2], json!("s1"));
    assert_eq!(responses[2][2], json!("s2"));
    assert_eq!(responses[1][0], json!("Thread/get"));
    assert_eq!(responses[2][0], json!("Email/get"));
}

#[test]
fn result_reference_resolves_query_ids_into_get() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![
            json!(["Email/query", {
                "accountId": ACCOUNT_ID,
                "filter": { "inMailbox": "mb_inbox" }
            }, "q1"]),
            json!(["Email/get", {
                "accountId": ACCOUNT_ID,
                "ids": { "resultOf": "q1", "name": "Email/query", "path": "/ids" }
            }, "g1"]),
        ],
    );
    let (query_name, query_args, _) = nth(&body, 0);
    assert_eq!(query_name, "Email/query");
    let ids = query_args["ids"].as_array().unwrap();
    assert!(!ids.is_empty(), "the inbox query should match fixtures");

    let (get_name, get_args, _) = nth(&body, 1);
    assert_eq!(get_name, "Email/get");
    assert_eq!(
        get_args["list"].as_array().unwrap().len(),
        ids.len(),
        "every referenced id resolved"
    );
    assert_eq!(get_args["notFound"], json!([]));
}

#[test]
fn unresolvable_result_reference_is_an_in_band_error() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Email/get", {
            "accountId": ACCOUNT_ID,
            "ids": { "resultOf": "nope", "name": "Email/query", "path": "/ids" }
        }, "g1"])],
    );
    let (name, args, call_id) = nth(&body, 0);
    assert_eq!(name, "error");
    assert_eq!(call_id, "g1");
    assert_eq!(args["type"], json!("invalidResultReference"));
}

#[test]
fn created_ids_round_trip_through_a_back_reference() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![
            json!(["Mailbox/set", {
                "accountId": ACCOUNT_ID,
                "create": { "c0": { "name": "Fresh", "role": null } }
            }, "s0"]),
            json!(["Mailbox/set", {
                "accountId": ACCOUNT_ID,
                "update": { "#c0": { "name": "Renamed" } }
            }, "s1"]),
            json!(["Mailbox/get", {
                "accountId": ACCOUNT_ID,
                "ids": ["#c0"]
            }, "s2"]),
        ],
    );

    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "Mailbox/set");
    let created = args["created"].as_object().unwrap();
    assert!(
        created.contains_key("c0"),
        "created map keys the creation id"
    );
    let real_id = created["c0"]["id"].as_str().unwrap().to_string();
    assert!(!real_id.is_empty());

    // The response echoes creationId -> realId so a later request can reuse it.
    assert_eq!(body["createdIds"]["c0"], json!(&real_id));

    // `#c0` resolved to the real id in the update call.
    let (name, args, _) = nth(&body, 1);
    assert_eq!(name, "Mailbox/set");
    assert_eq!(args["updated"][&real_id]["name"], json!("Renamed"));
    assert!(
        args.get("notUpdated").is_none(),
        "back reference must not miss: {args}"
    );

    // And the get by back reference finds it.
    let (_, args, _) = nth(&body, 2);
    assert_eq!(
        args["list"].as_array().unwrap()[0]["name"],
        json!("Renamed")
    );
}

#[test]
fn set_reports_not_found_for_unknown_ids() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/set", {
            "accountId": ACCOUNT_ID,
            "update": { "mb_missing": { "name": "x" } },
            "destroy": ["mb_missing"]
        }, "s0"])],
    );
    let (_, args, _) = nth(&body, 0);
    assert_eq!(args["notUpdated"]["mb_missing"]["type"], json!("notFound"));
    assert_eq!(
        args["notDestroyed"]["mb_missing"]["type"],
        json!("notFound")
    );
}

#[test]
fn if_in_state_mismatch_is_an_in_band_error() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/set", {
            "accountId": ACCOUNT_ID,
            "ifInState": "999",
            "create": { "c0": { "name": "Nope" } }
        }, "s0"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "error");
    assert_eq!(args["type"], json!("stateMismatch"));
}

#[test]
fn changes_track_created_updated_destroyed() {
    let server = MockJmapServer::start();
    let handle = server.handle();

    let initial = handle.with_store(|store| store.mailboxes.state_str());
    handle.with_store_mut(|store| {
        let id = store.mailboxes.create(json!({ "name": "New" }));
        store
            .mailboxes
            .record_change(vec![id], Vec::new(), Vec::new());
        store
            .mailboxes
            .update("mb_inbox", &json!({ "name": "Inbox2" }));
        store
            .mailboxes
            .record_change(Vec::new(), vec!["mb_inbox".into()], Vec::new());
        store.mailboxes.destroy("mb_sent");
        store
            .mailboxes
            .record_change(Vec::new(), Vec::new(), vec!["mb_sent".into()]);
    });

    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/changes", {
            "accountId": ACCOUNT_ID,
            "sinceState": initial
        }, "c0"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "Mailbox/changes");
    assert_eq!(args["oldState"], json!(initial));
    assert_ne!(args["newState"], json!(initial));
    assert_eq!(args["hasMoreChanges"], json!(false));
    assert_eq!(args["created"].as_array().unwrap().len(), 1);
    assert_eq!(args["updated"], json!(["mb_inbox"]));
    assert_eq!(args["destroyed"], json!(["mb_sent"]));
    assert!(args["updatedProperties"].is_array());
}

#[test]
fn query_and_query_changes_report_added_and_removed() {
    let server = MockJmapServer::start();
    let handle = server.handle();

    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Email/query", {
            "accountId": ACCOUNT_ID,
            "filter": { "inMailbox": "mb_inbox" }
        }, "q0"])],
    );
    let (_, args, _) = nth(&body, 0);
    let state = args["queryState"].as_str().unwrap().to_string();
    assert_eq!(args["ids"].as_array().unwrap().len(), 2);

    // A new email in the inbox shows up as `added`.
    handle.with_store_mut(|store| {
        let id = store.emails.create(json!({
            "subject": "Third inbox message",
            "mailboxIds": { "mb_inbox": true },
            "keywords": {}
        }));
        store.emails.record_change(vec![id], Vec::new(), Vec::new());
    });

    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Email/queryChanges", {
            "accountId": ACCOUNT_ID,
            "filter": { "inMailbox": "mb_inbox" },
            "sinceQueryState": state
        }, "q1"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "Email/queryChanges");
    assert_eq!(args["oldQueryState"], json!(state));
    assert_ne!(args["newQueryState"], json!(state));
    assert_eq!(
        args["added"].as_array().unwrap().len(),
        3,
        "all current matches are reported as added: {args}"
    );
    assert!(args["added"][0].get("index").is_some());
    assert_eq!(args["removed"], json!([]));
}

#[test]
fn email_import_creates_email_and_thread() {
    let server = MockJmapServer::start();
    let handle = server.handle();
    let blob_id = handle.with_store_mut(|store| {
        store.blobs.put(
            b"From: someone@example.com\r\nTo: test@example.org\r\n\
               Subject: Imported via blob\r\n\r\nbody\r\n"
                .to_vec(),
            "message/rfc822",
        )
    });

    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Email/import", {
            "accountId": ACCOUNT_ID,
            "emails": { "c0": { "blobId": blob_id, "mailboxIds": { "mb_inbox": true } } }
        }, "s0"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "Email/import");
    let created = args["created"].as_object().unwrap();
    let email = &created["c0"];
    assert_eq!(email["subject"], json!("Imported via blob"));
    let email_id = email["id"].as_str().unwrap().to_string();
    let thread_id = email["threadId"].as_str().unwrap().to_string();

    // The thread exists and links back to the email.
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Thread/get", {
            "accountId": ACCOUNT_ID,
            "ids": [thread_id]
        }, "s1"])],
    );
    let (_, args, _) = nth(&body, 0);
    assert_eq!(
        args["list"].as_array().unwrap()[0]["emailIds"],
        json!([email_id])
    );
}

#[test]
fn email_copy_clones_into_another_mailbox() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Email/copy", {
            "fromAccountId": ACCOUNT_ID,
            "accountId": ACCOUNT_ID,
            "create": { "c0": {
                "id": "em_1",
                "mailboxIds": { "mb_archive": true }
            } }
        }, "s0"])],
    );
    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "Email/copy");
    assert_eq!(args["fromAccountId"], json!(ACCOUNT_ID));
    let copy = &args["created"]["c0"];
    assert_eq!(copy["subject"], json!("Welcome to JMAP"));
    assert_eq!(copy["mailboxIds"], json!({ "mb_archive": true }));
    assert_ne!(copy["id"], json!("em_1"));
}

#[test]
fn blob_copy_maps_old_ids_to_new_ones() {
    let server = MockJmapServer::start();
    let (_, body) = jmap(
        server.port(),
        &[CORE],
        vec![json!(["Blob/copy", {
            "accountId": ACCOUNT_ID,
            "blobIds": ["blob_0", "blob_missing"]
        }, "s0"])],
    );
    let (_, args, _) = nth(&body, 0);
    let copied = args["copied"].as_object().unwrap();
    assert_eq!(copied.len(), 1);
    let new_id = copied["blob_0"].as_str().unwrap().to_string();
    assert_ne!(new_id, "blob_0");
    assert_eq!(
        args["notCopied"]["blob_missing"]["type"],
        json!("blobNotFound")
    );
}

#[test]
fn sieve_validate_reports_invalid_script_in_the_response() {
    let server = MockJmapServer::start();
    let handle = server.handle();
    let good = handle.with_store_mut(|store| {
        store
            .blobs
            .put(b"require [\"fileinto\"];\r\n".to_vec(), "text/plain")
    });
    let bad = handle.with_store_mut(|store| {
        store
            .blobs
            .put(b"require INVALID;\r\n".to_vec(), "text/plain")
    });

    let (_, body) = jmap(
        server.port(),
        &[CORE, "urn:ietf:params:jmap:sieve"],
        vec![
            json!(["SieveScript/validate", {
                "accountId": ACCOUNT_ID,
                "blobId": good,
                "name": "filter"
            }, "v0"]),
            json!(["SieveScript/validate", {
                "accountId": ACCOUNT_ID,
                "blobId": bad,
                "name": "broken"
            }, "v1"]),
        ],
    );

    let (name, args, _) = nth(&body, 0);
    assert_eq!(name, "SieveScript/validate");
    assert!(args.get("error").is_none(), "valid script: {args}");

    let (name, args, _) = nth(&body, 1);
    assert_eq!(name, "SieveScript/validate");
    assert_eq!(args["error"]["type"], json!("invalidScript"));
}

// ---------------------------------------------------------------------------
// fault injection
// ---------------------------------------------------------------------------

#[test]
fn injected_method_error_replaces_the_real_response() {
    let server = MockJmapServer::start();
    server
        .handle()
        .inject_method_error("Mailbox/get", "serverFail", 1);

    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );
    let (name, args, call_id) = nth(&body, 0);
    assert_eq!((name.as_str(), call_id.as_str()), ("error", "c0"));
    assert_eq!(args["type"], json!("serverFail"));

    // Consumed: the next call works again.
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c1"])],
    );
    assert_eq!(nth(&body, 0).0, "Mailbox/get");
}

#[test]
fn injected_problem_short_circuits_the_route() {
    let server = MockJmapServer::start();
    server.handle().inject_problem("/jmap", 513, "overQuota", 2);

    for call in ["c0", "c1"] {
        let (status, headers, body) = send(
            server.port(),
            &build_request(
                "POST",
                "/jmap",
                &[
                    ("Authorization", AUTH.into()),
                    ("Content-Type", "application/json".into()),
                ],
                json!({"using": [CORE], "methodCalls": [["Mailbox/get", {}, call]]})
                    .to_string()
                    .as_bytes(),
            ),
        );
        assert_eq!(status, 513);
        assert_eq!(headers["content-type"], "application/problem+json");
        let doc: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(doc["type"], json!("overQuota"));
    }

    // Exhausted after two requests.
    let (status, _, _) = get(server.port(), "/.well-known/jmap");
    assert_eq!(status, 200);
}

#[test]
fn injected_raw_body_is_returned_verbatim() {
    let server = MockJmapServer::start();
    server
        .handle()
        .inject_raw("/jmap", 200, "text/html", b"<html>not jmap</html>", 1);
    let (status, headers, body) = send(
        server.port(),
        &build_request(
            "POST",
            "/jmap",
            &[("Authorization", AUTH.into())],
            b"{\"using\":[],\"methodCalls\":[]}",
        ),
    );
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "text/html");
    assert_eq!(body, b"<html>not jmap</html>");
}

#[test]
fn clear_injections_removes_pending_faults() {
    let server = MockJmapServer::start();
    let handle = server.handle();
    handle.inject_method_error("Mailbox/get", "serverFail", 1);
    handle.inject_problem("/jmap", 500, "boom", 1);
    handle.clear_injections();

    let (status, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );
    assert_eq!(status, 200);
    assert_eq!(nth(&body, 0).0, "Mailbox/get");
}

// ---------------------------------------------------------------------------
// blobs
// ---------------------------------------------------------------------------

#[test]
fn upload_returns_the_blob_metadata_shape() {
    let server = MockJmapServer::start();
    let payload = b"hello attachment".to_vec();
    let (status, headers, body) = send(
        server.port(),
        &build_request(
            "POST",
            &format!("/upload/{ACCOUNT_ID}"),
            &[
                ("Authorization", AUTH.into()),
                ("Content-Type", "application/pdf".into()),
            ],
            &payload,
        ),
    );
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["accountId"], json!(ACCOUNT_ID));
    assert_eq!(doc["type"], json!("application/pdf"));
    assert_eq!(doc["size"], json!(payload.len()));
    let blob_id = doc["blobId"].as_str().unwrap().to_string();
    assert!(!blob_id.is_empty());

    // And the bytes come back unchanged.
    let (status, headers, downloaded) = get(
        server.port(),
        &format!("/download/{ACCOUNT_ID}/{blob_id}/file.pdf"),
    );
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/pdf");
    assert_eq!(downloaded, payload);
}

#[test]
fn oversize_upload_is_rejected() {
    let server = MockJmapServer::start();
    server.handle().set_max_size_upload(16);
    let (status, headers, body) = send(
        server.port(),
        &build_request(
            "POST",
            &format!("/upload/{ACCOUNT_ID}"),
            &[
                ("Authorization", AUTH.into()),
                ("Content-Type", "application/octet-stream".into()),
            ],
            &[0u8; 64],
        ),
    );
    assert_eq!(status, 413);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("cannotUploadBlob"));
}

#[test]
fn upload_to_unknown_account_is_404() {
    let server = MockJmapServer::start();
    let (status, _, _) = send(
        server.port(),
        &build_request(
            "POST",
            "/upload/someotheraccount",
            &[("Authorization", AUTH.into())],
            b"x",
        ),
    );
    assert_eq!(status, 404);
}

#[test]
fn download_unknown_blob_is_404() {
    let server = MockJmapServer::start();
    let (status, headers, body) = get(
        server.port(),
        &format!("/download/{ACCOUNT_ID}/blob_nope/name.txt"),
    );
    assert_eq!(status, 404);
    assert_eq!(headers["content-type"], "application/problem+json");
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["type"], json!("notFound"));
}

// ---------------------------------------------------------------------------
// request log
// ---------------------------------------------------------------------------

#[test]
fn request_log_records_every_request() {
    let server = MockJmapServer::start();
    let handle = server.handle();
    let _ = get(server.port(), "/.well-known/jmap");
    let _ = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );

    let log = handle.requests_log();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].method, "GET");
    assert_eq!(log[0].path, "/.well-known/jmap");
    assert_eq!(log[1].method, "POST");
    assert_eq!(log[1].path, "/jmap");
    assert!(String::from_utf8_lossy(&log[1].body).contains("Mailbox/get"));

    handle.clear_log();
    assert!(handle.requests_log().is_empty());
}

#[test]
fn chunked_request_bodies_are_decoded() {
    let server = MockJmapServer::start();
    let payload = json!({"using": [CORE, MAIL], "methodCalls": [["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"]]}).to_string();
    let chunk = payload.as_bytes();
    let mut raw = b"POST /jmap HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic x\r\n\
                    Content-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n"
        .to_vec();
    raw.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
    raw.extend_from_slice(chunk);
    raw.extend_from_slice(b"\r\n0\r\n\r\n");

    let (status, _, body) = send(server.port(), &raw);
    assert_eq!(status, 200);
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["methodResponses"][0][0], json!("Mailbox/get"));
}

#[test]
fn server_shuts_down_cleanly() {
    let server = MockJmapServer::start();
    let port = server.port();
    let _ = get(port, "/.well-known/jmap");
    server.shutdown();
    // The port is released: a new connection must fail.
    assert!(TcpStream::connect(address(port)).is_err());
}

#[test]
fn empty_store_starts_without_fixtures() {
    let server = MockJmapServer::with_store(mock_jmap::store::Store::empty());
    let (_, body) = jmap(
        server.port(),
        &[CORE, MAIL],
        vec![json!(["Mailbox/get", {"accountId": ACCOUNT_ID}, "c0"])],
    );
    let (_, args, _) = nth(&body, 0);
    assert_eq!(args["list"], json!([]));
    assert_eq!(args["notFound"], json!([]));
}
