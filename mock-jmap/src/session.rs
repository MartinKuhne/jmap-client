//! The JMAP session resource (`GET /.well-known/jmap`).
//!
//! Shape per RFC 8620 section 3.1 plus the capability blocks of RFC 8621
//! (mail/submission/vacationresponse), RFC 8887 (websocket) and
//! draft-ietf-jmap-sieve-12 (sieve).
//!
//! Every block the client models is advertised so that
//! `jmap_client::core::session::Capabilities` picks the intended variant:
//!
//! | capability | client variant |
//! |---|---|
//! | `urn:ietf:params:jmap:core` | `Capabilities::Core` |
//! | `urn:ietf:params:jmap:mail` | `Capabilities::Mail` |
//! | `urn:ietf:params:jmap:submission` | `Capabilities::Submission` |
//! | `urn:ietf:params:jmap:websocket` | `Capabilities::WebSocket` |
//! | `urn:ietf:params:jmap:sieve` | `Capabilities::Sieve` |
//! | `urn:ietf:params:jmap:vacationresponse` / `:blob` / `:quota` / `:principals` | `Capabilities::Empty` |
//!
//! Note: `Capabilities::Other` is only reachable for a *non-object* capability
//! value, because the field-less `EmptyCapabilities` variant comes first and
//! serde ignores unknown fields. See the plan's Findings log.

use serde_json::{json, Value};

/// The single account the mock serves.
pub const ACCOUNT_ID: &str = "account1";

/// Username of the single account.
pub const USERNAME: &str = "test@example.org";

/// `maxSizeUpload` advertised by the core capability; the upload route enforces
/// it so tests can trigger the 413 path.
pub const MAX_SIZE_UPLOAD: usize = 52_428_800;

/// Builds the session resource for a server rooted at `base_url` whose WebSocket
/// endpoint is `ws_url`.
pub fn session_json(base_url: &str, ws_url: &str) -> Value {
    json!({
        "capabilities": {
            "urn:ietf:params:jmap:core": {
                "maxSizeUpload": MAX_SIZE_UPLOAD,
                "maxConcurrentUpload": 4,
                "maxSizeRequest": 10_485_760,
                "maxConcurrentRequests": 8,
                "maxCallsInRequest": 16,
                "maxObjectsInGet": 512,
                "maxObjectsInSet": 512,
                "collationAlgorithms": [
                    "i;ascii-numeric",
                    "i;ascii-casemap",
                    "i;unicode-casemap"
                ]
            },
            "urn:ietf:params:jmap:mail": {
                "maxMailboxesPerEmail": null,
                "maxMailboxDepth": 10,
                "maxSizeMailboxName": 46,
                "maxSizeAttachmentsPerEmail": 26_214_400,
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
            },
            "urn:ietf:params:jmap:submission": {
                "maxDelayedSend": 4_102_444_800u64,
                "submissionExtensions": ["SIZE", "DSN", "UTF8ONLY"]
            },
            "urn:ietf:params:jmap:vacationresponse": {},
            "urn:ietf:params:jmap:websocket": {
                "url": ws_url,
                "supportsPush": true
            },
            "urn:ietf:params:jmap:sieve": {
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
            },
            "urn:ietf:params:jmap:principals": {},
            "urn:ietf:params:jmap:blob": {},
            "urn:ietf:params:jmap:quota": {},
            "urn:ietf:params:jmap:mdn": {
                "maxSizeMaildropReport": 1_048_576
            }
        },
        "accounts": {
            ACCOUNT_ID: {
                "name": USERNAME,
                "isPersonal": true,
                "isReadOnly": false,
                "accountCapabilities": {
                    "urn:ietf:params:jmap:mail": {
                        "maxMailboxesPerEmail": null,
                        "maxMailboxDepth": 10,
                        "maxSizeMailboxName": 46,
                        "maxSizeAttachmentsPerEmail": 26_214_400,
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
                    },
                    "urn:ietf:params:jmap:submission": {
                        "maxDelayedSend": 4_102_444_800u64,
                        "submissionExtensions": ["SIZE", "DSN", "UTF8ONLY"]
                    },
                    "urn:ietf:params:jmap:vacationresponse": {},
                    "urn:ietf:params:jmap:sieve": {
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
                    },
                    "urn:ietf:params:jmap:principals": {},
                    "urn:ietf:params:jmap:quota": {}
                }
            }
        },
        "primaryAccounts": {
            "urn:ietf:params:jmap:mail": ACCOUNT_ID,
            "urn:ietf:params:jmap:submission": ACCOUNT_ID,
            "urn:ietf:params:jmap:vacationresponse": ACCOUNT_ID,
            "urn:ietf:params:jmap:sieve": ACCOUNT_ID,
            "urn:ietf:params:jmap:principals": ACCOUNT_ID
        },
        "username": USERNAME,
        "apiUrl": format!("{base_url}/jmap"),
        "downloadUrl": format!("{base_url}/download/{{accountId}}/{{blobId}}/{{name}}?accept={{type}}"),
        "uploadUrl": format!("{base_url}/upload/{{accountId}}"),
        "eventSourceUrl": format!(
            "{base_url}/eventSource?types={{types}}&closeafter={{closeafter}}&ping={{ping}}"
        ),
        "state": "1"
    })
}

/// The capability ids advertised at the top level of the session resource.
///
/// `POST /jmap` validates the request's `using` array against this set and
/// answers `urn:ietf:params:jmap:error:unknownCapability` otherwise
/// (RFC 8620 section 3.2).
pub fn advertised_capabilities() -> Vec<&'static str> {
    [
        "urn:ietf:params:jmap:core",
        "urn:ietf:params:jmap:mail",
        "urn:ietf:params:jmap:submission",
        "urn:ietf:params:jmap:vacationresponse",
        "urn:ietf:params:jmap:websocket",
        "urn:ietf:params:jmap:sieve",
        "urn:ietf:params:jmap:principals",
        "urn:ietf:params:jmap:blob",
        "urn:ietf:params:jmap:quota",
        "urn:ietf:params:jmap:mdn",
    ]
    .to_vec()
}
