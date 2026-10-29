/*
 * Copyright Stalwart Labs LLC See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use crate::{
    email::{MailCapabilities, SubmissionCapabilities},
    URI,
};
use ahash::AHashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(rename = "capabilities")]
    capabilities: AHashMap<String, Capabilities>,

    #[serde(rename = "accounts")]
    accounts: AHashMap<String, Account>,

    #[serde(rename = "primaryAccounts")]
    primary_accounts: AHashMap<String, String>,

    #[serde(rename = "username")]
    username: String,

    #[serde(rename = "apiUrl")]
    api_url: String,

    #[serde(rename = "downloadUrl")]
    download_url: String,

    #[serde(rename = "uploadUrl")]
    upload_url: String,

    #[serde(rename = "eventSourceUrl")]
    event_source_url: String,

    #[serde(rename = "state")]
    state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    #[serde(rename = "name")]
    name: String,

    #[serde(rename = "isPersonal")]
    is_personal: bool,

    #[serde(rename = "isReadOnly")]
    is_read_only: bool,

    #[serde(rename = "accountCapabilities")]
    account_capabilities: AHashMap<String, Capabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Capabilities {
    Core(CoreCapabilities),
    Mail(MailCapabilities),
    Submission(SubmissionCapabilities),
    WebSocket(WebSocketCapabilities),
    Sieve(SieveCapabilities),
    Empty(EmptyCapabilities),
    Other(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreCapabilities {
    #[serde(rename = "maxSizeUpload")]
    max_size_upload: usize,

    #[serde(rename = "maxConcurrentUpload")]
    max_concurrent_upload: usize,

    #[serde(rename = "maxSizeRequest")]
    max_size_request: usize,

    #[serde(rename = "maxConcurrentRequests")]
    max_concurrent_requests: usize,

    #[serde(rename = "maxCallsInRequest")]
    max_calls_in_request: usize,

    #[serde(rename = "maxObjectsInGet")]
    max_objects_in_get: usize,

    #[serde(rename = "maxObjectsInSet")]
    max_objects_in_set: usize,

    #[serde(rename = "collationAlgorithms")]
    collation_algorithms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketCapabilities {
    #[serde(rename = "url")]
    url: String,
    #[serde(rename = "supportsPush")]
    supports_push: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SieveCapabilities {
    #[serde(rename = "implementation")]
    implementation: Option<String>,
    #[serde(rename = "maxSizeScriptName")]
    max_script_name: Option<usize>,
    #[serde(rename = "maxSizeScript")]
    max_script_size: Option<usize>,
    #[serde(rename = "maxNumberScripts")]
    max_scripts: Option<usize>,
    #[serde(rename = "maxNumberRedirects")]
    max_redirects: Option<usize>,
    #[serde(rename = "sieveExtensions")]
    extensions: Vec<String>,
    #[serde(rename = "notificationMethods")]
    notification_methods: Option<Vec<String>>,
    #[serde(rename = "externalLists")]
    ext_lists: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyCapabilities {}

impl Session {
    pub fn capabilities(&self) -> impl Iterator<Item = &String> {
        self.capabilities.keys()
    }

    pub fn capability(&self, capability: impl AsRef<str>) -> Option<&Capabilities> {
        self.capabilities.get(capability.as_ref())
    }

    pub fn has_capability(&self, capability: impl AsRef<str>) -> bool {
        self.capabilities.contains_key(capability.as_ref())
    }

    pub fn websocket_capabilities(&self) -> Option<&WebSocketCapabilities> {
        self.capabilities
            .get(URI::WebSocket.as_ref())
            .and_then(|v| match v {
                Capabilities::WebSocket(capabilities) => Some(capabilities),
                _ => None,
            })
    }

    pub fn core_capabilities(&self) -> Option<&CoreCapabilities> {
        self.capabilities
            .get(URI::Core.as_ref())
            .and_then(|v| match v {
                Capabilities::Core(capabilities) => Some(capabilities),
                _ => None,
            })
    }

    pub fn mail_capabilities(&self) -> Option<&MailCapabilities> {
        self.capabilities
            .get(URI::Mail.as_ref())
            .and_then(|v| match v {
                Capabilities::Mail(capabilities) => Some(capabilities),
                _ => None,
            })
    }

    pub fn submission_capabilities(&self) -> Option<&SubmissionCapabilities> {
        self.capabilities
            .get(URI::Submission.as_ref())
            .and_then(|v| match v {
                Capabilities::Submission(capabilities) => Some(capabilities),
                _ => None,
            })
    }

    pub fn sieve_capabilities(&self) -> Option<&SieveCapabilities> {
        self.capabilities
            .get(URI::Sieve.as_ref())
            .and_then(|v| match v {
                Capabilities::Sieve(capabilities) => Some(capabilities),
                _ => None,
            })
    }

    pub fn accounts(&self) -> impl Iterator<Item = &String> {
        self.accounts.keys()
    }

    pub fn account(&self, account: &str) -> Option<&Account> {
        self.accounts.get(account)
    }

    pub fn primary_accounts(&self) -> impl Iterator<Item = (&String, &String)> {
        self.primary_accounts.iter()
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    pub fn download_url(&self) -> &str {
        &self.download_url
    }

    pub fn upload_url(&self) -> &str {
        &self.upload_url
    }

    pub fn event_source_url(&self) -> &str {
        &self.event_source_url
    }

    pub fn state(&self) -> &str {
        &self.state
    }
}

impl Account {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_personal(&self) -> bool {
        self.is_personal
    }

    pub fn is_read_only(&self) -> bool {
        self.is_read_only
    }

    pub fn capabilities(&self) -> impl Iterator<Item = &String> {
        self.account_capabilities.keys()
    }

    pub fn capability(&self, capability: &str) -> Option<&Capabilities> {
        self.account_capabilities.get(capability)
    }
}

impl CoreCapabilities {
    pub fn max_size_upload(&self) -> usize {
        self.max_size_upload
    }

    pub fn max_concurrent_upload(&self) -> usize {
        self.max_concurrent_upload
    }

    pub fn max_size_request(&self) -> usize {
        self.max_size_request
    }

    pub fn max_concurrent_requests(&self) -> usize {
        self.max_concurrent_requests
    }

    pub fn max_calls_in_request(&self) -> usize {
        self.max_calls_in_request
    }

    pub fn max_objects_in_get(&self) -> usize {
        self.max_objects_in_get
    }

    pub fn max_objects_in_set(&self) -> usize {
        self.max_objects_in_set
    }

    pub fn collation_algorithms(&self) -> &[String] {
        &self.collation_algorithms
    }
}

impl WebSocketCapabilities {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn supports_push(&self) -> bool {
        self.supports_push
    }
}

impl SieveCapabilities {
    pub fn max_script_name_size(&self) -> usize {
        self.max_script_name.unwrap_or(512)
    }

    pub fn max_script_size(&self) -> Option<usize> {
        self.max_script_size
    }

    pub fn max_number_scripts(&self) -> Option<usize> {
        self.max_scripts
    }

    pub fn max_number_redirects(&self) -> Option<usize> {
        self.max_redirects
    }

    pub fn sieve_extensions(&self) -> &[String] {
        &self.extensions
    }

    pub fn notification_methods(&self) -> Option<&[String]> {
        self.notification_methods.as_deref()
    }

    pub fn external_lists(&self) -> Option<&[String]> {
        self.ext_lists.as_deref()
    }
}

pub trait URLParser: Sized {
    fn parse(value: &str) -> Option<Self>;
}

pub enum URLPart<T: URLParser> {
    Value(String),
    Parameter(T),
}

impl<T: URLParser> URLPart<T> {
    pub fn parse(url: &str) -> crate::Result<Vec<URLPart<T>>> {
        let mut parts = Vec::new();
        let mut buf = String::with_capacity(url.len());
        let mut in_parameter = false;

        for ch in url.chars() {
            match ch {
                '{' => {
                    if !buf.is_empty() {
                        parts.push(URLPart::Value(buf.clone()));
                        buf.clear();
                    }
                    in_parameter = true;
                }
                '}' => {
                    if in_parameter && !buf.is_empty() {
                        parts.push(URLPart::Parameter(T::parse(&buf).ok_or_else(|| {
                            crate::Error::Internal(format!(
                                "Invalid parameter '{}' in URL: {}",
                                buf, url
                            ))
                        })?));
                        buf.clear();
                    } else {
                        return Err(crate::Error::Internal(format!("Invalid URL: {}", url)));
                    }
                    in_parameter = false;
                }
                _ => {
                    buf.push(ch);
                }
            }
        }

        if !buf.is_empty() {
            if !in_parameter {
                parts.push(URLPart::Value(buf.clone()));
            } else {
                return Err(crate::Error::Internal(format!("Invalid URL: {}", url)));
            }
        }

        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the session resource accessors (RFC 8620 §2,
    //! RFC 8621 §2, RFC 8887 §4.2, draft-ietf-jmap-sieve-12 §2) and for
    //! the URL-template parser (RFC 8620 §2.4).

    use super::*;

    /// A session advertising every capability this crate models. The
    /// shapes are the ones each `Capabilities` variant expects.
    const SESSION: &str = r#"{
        "capabilities": {
            "urn:ietf:params:jmap:core": {
                "maxSizeUpload": 52428800,
                "maxConcurrentUpload": 4,
                "maxSizeRequest": 10485760,
                "maxConcurrentRequests": 8,
                "maxCallsInRequest": 16,
                "maxObjectsInGet": 500,
                "maxObjectsInSet": 500,
                "collationAlgorithms": [
                    "i;ascii-numeric", "i;ascii-casemap", "i;unicode-casemap"
                ]
            },
            "urn:ietf:params:jmap:mail": {
                "maxMailboxesPerEmail": null,
                "maxMailboxDepth": 10,
                "maxSizeMailboxName": 255,
                "maxSizeAttachmentsPerEmail": 33554432,
                "emailQuerySortOptions": ["receivedAt", "size", "subject"],
                "mayCreateTopLevelMailbox": true
            },
            "urn:ietf:params:jmap:submission": {
                "maxDelayedSend": 1209600,
                "submissionExtensions": ["SIZE", "DSN", "UTF8ONLY"]
            },
            "urn:ietf:params:jmap:vacationresponse": {},
            "urn:ietf:params:jmap:websocket": {
                "url": "wss://jmap.example.org/ws",
                "supportsPush": true
            },
            "urn:ietf:params:jmap:sieve": {
                "implementation": "Stalwart",
                "maxSizeScriptName": 255,
                "maxSizeScript": 1048576,
                "maxNumberScripts": 10,
                "maxNumberRedirects": 5,
                "sieveExtensions": ["fileinto", "mailbox", "envelope", "variables"],
                "notificationMethods": ["mailto"],
                "externalLists": ["https://example.org/lists"]
            },
            "urn:example:not-an-object": "strings cannot back any capability"
        },
        "accounts": {
            "u33084183": {
                "name": "test@example.org",
                "isPersonal": true,
                "isReadOnly": false,
                "accountCapabilities": {
                    "urn:ietf:params:jmap:mail": {
                        "maxMailboxesPerEmail": 5,
                        "maxMailboxDepth": 4,
                        "maxSizeMailboxName": 128,
                        "maxSizeAttachmentsPerEmail": 1048576,
                        "emailQuerySortOptions": ["receivedAt"],
                        "mayCreateTopLevelMailbox": false
                    }
                }
            },
            "uarchive0001": {
                "name": "archive@example.org",
                "isPersonal": false,
                "isReadOnly": true,
                "accountCapabilities": {}
            }
        },
        "primaryAccounts": {
            "urn:ietf:params:jmap:mail": "u33084183",
            "urn:ietf:params:jmap:submission": "u33084183"
        },
        "username": "test@example.org",
        "apiUrl": "https://jmap.example.org/jmap",
        "downloadUrl": "https://jmap.example.org/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
        "uploadUrl": "https://jmap.example.org/jmap/upload/{accountId}",
        "eventSourceUrl": "https://jmap.example.org/jmap/eventSource/?types={types}&closeafter={closeafter}&ping={ping}",
        "state": "75128aab4b1b"
    }"#;

    fn session() -> Session {
        serde_json::from_str(SESSION).expect("the session fixture must deserialise")
    }

    /// A minimal parser used to drive [`URLPart::parse`] without pulling a
    /// real blob/event-source URL parameter type into the test.
    #[derive(Debug, PartialEq, Eq)]
    enum TestParam {
        AccountId,
        BlobId,
        Name,
        Type,
    }

    impl URLParser for TestParam {
        fn parse(value: &str) -> Option<Self> {
            match value {
                "accountId" => Some(TestParam::AccountId),
                "blobId" => Some(TestParam::BlobId),
                "name" => Some(TestParam::Name),
                "type" => Some(TestParam::Type),
                _ => None,
            }
        }
    }

    /// `URLPart` has no `Debug`, so the error has to be pulled out by hand
    /// rather than with `unwrap_err` (Finding #23's pattern).
    fn parse_err(url: &str) -> crate::Error {
        match URLPart::<TestParam>::parse(url) {
            Ok(parts) => panic!("{url} must not parse, got {} parts", parts.len()),
            Err(error) => error,
        }
    }

    /// RFC 8620 §2 — the top-level capability map: which URIs are
    /// advertised, and what a lookup of one that is not returns.
    #[test]
    fn capability_lookup() {
        let session = session();
        assert_eq!(session.capabilities().count(), 7);
        assert!(session.has_capability(URI::Core));
        assert!(session.has_capability("urn:ietf:params:jmap:sieve"));
        assert!(!session.has_capability("urn:ietf:params:jmap:calendars"));
        assert!(session
            .capability("urn:ietf:params:jmap:calendars")
            .is_none());
        assert!(matches!(
            session.capability(URI::VacationResponse),
            Some(Capabilities::Empty(_))
        ));
        assert!(
            matches!(
                session.capability("urn:example:not-an-object"),
                Some(Capabilities::Other(_))
            ),
            "a non-object capability value falls through to `Other` (Finding #3)"
        );
    }

    /// RFC 8620 §2 — the core capability block.
    #[test]
    fn core_capabilities() {
        let session = session();
        let caps = session.core_capabilities().unwrap();
        assert_eq!(caps.max_size_upload(), 52_428_800);
        assert_eq!(caps.max_concurrent_upload(), 4);
        assert_eq!(caps.max_size_request(), 10_485_760);
        assert_eq!(caps.max_concurrent_requests(), 8);
        assert_eq!(caps.max_calls_in_request(), 16);
        assert_eq!(caps.max_objects_in_get(), 500);
        assert_eq!(caps.max_objects_in_set(), 500);
        assert_eq!(
            caps.collation_algorithms(),
            ["i;ascii-numeric", "i;ascii-casemap", "i;unicode-casemap"]
        );
    }

    /// RFC 8621 §2 — the mail capability block.
    #[test]
    fn mail_capabilities() {
        let session = session();
        let caps = session.mail_capabilities().unwrap();
        assert_eq!(caps.max_mailboxes_per_email(), None);
        assert_eq!(caps.max_mailbox_depth(), 10);
        assert_eq!(caps.max_size_mailbox_name(), 255);
        assert_eq!(caps.max_size_attachments_per_email(), 33_554_432);
        assert_eq!(
            caps.email_query_sort_options(),
            ["receivedAt", "size", "subject"]
        );
        assert!(caps.may_create_top_level_mailbox());
    }

    /// RFC 8621 §2 — the submission capability block.
    #[test]
    fn submission_capabilities() {
        let session = session();
        let caps = session.submission_capabilities().unwrap();
        assert_eq!(caps.max_delayed_send(), 1_209_600);
        assert_eq!(caps.submission_extensions(), ["SIZE", "DSN", "UTF8ONLY"]);
    }

    /// RFC 8887 §4.2 — the websocket capability block.
    #[test]
    fn websocket_capabilities() {
        let session = session();
        let caps = session.websocket_capabilities().unwrap();
        assert_eq!(caps.url(), "wss://jmap.example.org/ws");
        assert!(caps.supports_push());
    }

    /// draft-ietf-jmap-sieve-12 §2 — the sieve capability block,
    /// including the default `max_script_name_size()` falls back to when
    /// the server omits `maxSizeScriptName`.
    #[test]
    fn sieve_capabilities() {
        let session = session();
        let caps = session.sieve_capabilities().unwrap();
        assert_eq!(caps.max_script_name_size(), 255);
        assert_eq!(caps.max_script_size(), Some(1_048_576));
        assert_eq!(caps.max_number_scripts(), Some(10));
        assert_eq!(caps.max_number_redirects(), Some(5));
        assert_eq!(
            caps.sieve_extensions(),
            ["fileinto", "mailbox", "envelope", "variables"]
        );
        assert_eq!(caps.notification_methods().unwrap(), ["mailto"]);
        assert_eq!(
            caps.external_lists().unwrap(),
            ["https://example.org/lists"]
        );

        let minimal: SieveCapabilities =
            serde_json::from_value(serde_json::json!({ "sieveExtensions": [] })).unwrap();
        assert_eq!(
            minimal.max_script_name_size(),
            512,
            "an absent maxSizeScriptName falls back to 512"
        );
        assert_eq!(minimal.max_script_size(), None);
        assert_eq!(minimal.max_number_scripts(), None);
        assert_eq!(minimal.max_number_redirects(), None);
        assert!(minimal.sieve_extensions().is_empty());
        assert!(minimal.notification_methods().is_none());
        assert!(minimal.external_lists().is_none());
    }

    /// Every `*_capabilities()` helper has a `_ => None` arm for a session
    /// that puts a *differently shaped* object under the URI it looks for.
    /// Here each well-known URI holds another variant's payload, so no
    /// accessor may return a value.
    #[test]
    fn capability_accessors_return_none_when_the_variant_does_not_match() {
        let session: Session = serde_json::from_value(serde_json::json!({
            "capabilities": {
                "urn:ietf:params:jmap:core": {
                    "url": "wss://x/ws", "supportsPush": true
                },
                "urn:ietf:params:jmap:mail": {
                    "url": "wss://x/ws", "supportsPush": true
                },
                "urn:ietf:params:jmap:submission": {
                    "url": "wss://x/ws", "supportsPush": true
                },
                "urn:ietf:params:jmap:websocket": {
                    "maxSizeUpload": 1, "maxConcurrentUpload": 1, "maxSizeRequest": 1,
                    "maxConcurrentRequests": 1, "maxCallsInRequest": 1,
                    "maxObjectsInGet": 1, "maxObjectsInSet": 1,
                    "collationAlgorithms": []
                },
                "urn:ietf:params:jmap:sieve": {
                    "url": "wss://x/ws", "supportsPush": true
                }
            },
            "accounts": {},
            "primaryAccounts": {},
            "username": "test@example.org",
            "apiUrl": "https://x/jmap",
            "downloadUrl": "https://x/download/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://x/upload/{accountId}",
            "eventSourceUrl": "https://x/eventSource",
            "state": "s1"
        }))
        .unwrap();

        assert!(session.has_capability(URI::Core));
        assert!(session.core_capabilities().is_none());
        assert!(session.mail_capabilities().is_none());
        assert!(session.submission_capabilities().is_none());
        assert!(session.websocket_capabilities().is_none());
        assert!(session.sieve_capabilities().is_none());
        assert!(matches!(
            session.capability(URI::WebSocket.as_ref()),
            Some(Capabilities::Core(_))
        ));
    }

    /// A session that advertises none of the typed capabilities: every
    /// helper returns `None` because the lookup itself misses.
    #[test]
    fn capability_accessors_return_none_when_the_uri_is_absent() {
        let session: Session = serde_json::from_value(serde_json::json!({
            "capabilities": {},
            "accounts": {},
            "primaryAccounts": {},
            "username": "test@example.org",
            "apiUrl": "https://x/jmap",
            "downloadUrl": "https://x/download/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://x/upload/{accountId}",
            "eventSourceUrl": "https://x/eventSource",
            "state": "s1"
        }))
        .unwrap();

        assert_eq!(session.capabilities().count(), 0);
        assert!(session.core_capabilities().is_none());
        assert!(session.mail_capabilities().is_none());
        assert!(session.submission_capabilities().is_none());
        assert!(session.websocket_capabilities().is_none());
        assert!(session.sieve_capabilities().is_none());
    }

    /// RFC 8620 §2 — the account map, the primary-account map and the
    /// per-account capability block.
    #[test]
    fn accounts_and_primary_accounts() {
        let session = session();

        let mut ids: Vec<&String> = session.accounts().collect();
        ids.sort();
        assert_eq!(ids, ["u33084183", "uarchive0001"]);

        let account = session.account("u33084183").unwrap();
        assert_eq!(account.name(), "test@example.org");
        assert!(account.is_personal());
        assert!(!account.is_read_only());
        assert_eq!(
            account.capabilities().collect::<Vec<_>>(),
            ["urn:ietf:params:jmap:mail"]
        );
        assert!(account.capability("urn:ietf:params:jmap:mail").is_some());
        assert!(account.capability("urn:ietf:params:jmap:sieve").is_none());
        assert!(matches!(
            account.capability("urn:ietf:params:jmap:mail"),
            Some(Capabilities::Mail(_))
        ));

        let shared = session.account("uarchive0001").unwrap();
        assert!(!shared.is_personal());
        assert!(shared.is_read_only());
        assert_eq!(shared.capabilities().count(), 0);

        assert!(session.account("does-not-exist").is_none());

        let mut primaries: Vec<(&String, &String)> = session.primary_accounts().collect();
        primaries.sort();
        assert_eq!(
            primaries,
            vec![
                (
                    &"urn:ietf:params:jmap:mail".to_string(),
                    &"u33084183".to_string()
                ),
                (
                    &"urn:ietf:params:jmap:submission".to_string(),
                    &"u33084183".to_string()
                ),
            ]
        );
    }

    /// RFC 8620 §2 — the five URL templates and the session state string.
    #[test]
    fn session_urls_and_state() {
        let session = session();
        assert_eq!(session.username(), "test@example.org");
        assert_eq!(session.api_url(), "https://jmap.example.org/jmap");
        assert_eq!(
            session.download_url(),
            "https://jmap.example.org/jmap/download/{accountId}/{blobId}/{name}?accept={type}"
        );
        assert_eq!(
            session.upload_url(),
            "https://jmap.example.org/jmap/upload/{accountId}"
        );
        assert_eq!(
            session.event_source_url(),
            "https://jmap.example.org/jmap/eventSource/?types={types}&closeafter={closeafter}&ping={ping}"
        );
        assert_eq!(session.state(), "75128aab4b1b");
    }

    /// RFC 8620 §2.4 — a URL with no template tokens is one literal
    /// part; an empty URL is no parts at all.
    #[test]
    fn url_part_of_a_literal_url() {
        let parts = URLPart::<TestParam>::parse("https://jmap.example.org/jmap").unwrap();
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], URLPart::Value(v) if v == "https://jmap.example.org/jmap"));

        assert!(URLPart::<TestParam>::parse("").unwrap().is_empty());
    }

    /// RFC 8620 §2.4 — a template is split into literal values alternating
    /// with the parameters, in order.
    #[test]
    fn url_part_splits_a_template_in_order() {
        let parts =
            URLPart::<TestParam>::parse("https://x/download/{accountId}/{blobId}/{name}?a={type}")
                .unwrap();
        let shapes: Vec<String> = parts
            .iter()
            .map(|part| match part {
                URLPart::Value(value) => format!("value<{value}>"),
                URLPart::Parameter(param) => format!("param<{param:?}>"),
            })
            .collect();

        assert_eq!(
            shapes,
            vec![
                "value<https://x/download/>",
                "param<AccountId>",
                "value</>",
                "param<BlobId>",
                "value</>",
                "param<Name>",
                "value<?a=>",
                "param<Type>",
            ]
        );

        // a URL that is nothing but a parameter has no leading value part
        let parts = URLPart::<TestParam>::parse("{accountId}").unwrap();
        assert_eq!(parts.len(), 1);
        assert!(matches!(parts[0], URLPart::Parameter(TestParam::AccountId)));
    }

    /// The malformed-template arms: an unclosed token, a stray closing
    /// brace, an empty token and an unknown token name.
    #[test]
    fn url_part_rejects_malformed_templates() {
        let error = parse_err("https://x/upload/{accountId");
        assert_eq!(
            error.to_string(),
            "Internal error: Invalid URL: https://x/upload/{accountId",
            "an unterminated token never reaches the value arm"
        );

        let error = parse_err("https://x/upload/}");
        assert_eq!(
            error.to_string(),
            "Internal error: Invalid URL: https://x/upload/}",
            "a closing brace with no opening is rejected"
        );

        let error = parse_err("https://x/{}");
        assert_eq!(
            error.to_string(),
            "Internal error: Invalid URL: https://x/{}",
            "an empty token is rejected"
        );

        let error = parse_err("{bogus}");
        assert_eq!(
            error.to_string(),
            "Internal error: Invalid parameter 'bogus' in URL: {bogus}",
            "a token the parser does not know reports the token, not just the URL"
        );

        // a brace right after a closed parameter re-opens nothing: rejected
        assert!(URLPart::<TestParam>::parse("{accountId}}").is_err());
    }
}
