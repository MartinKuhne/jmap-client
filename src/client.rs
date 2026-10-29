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
    blob,
    core::{
        request::{self, Request},
        response,
        session::{Session, URLPart},
    },
    Error,
};
use ahash::AHashSet;
use encodify::base64;
#[cfg(feature = "blocking")]
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::{
    header::{self},
    redirect,
};
// `blocking` takes precedence over `async`: it enables maybe-async's `is_sync`
// switch, which makes this API synchronous, so the async types must not be
// imported alongside their blocking counterparts (they share the same names).
#[cfg(all(feature = "async", not(feature = "blocking")))]
use reqwest::{Client as HttpClient, Response};
use serde::de::DeserializeOwned;
use std::{
    net::IpAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

const DEFAULT_TIMEOUT_MS: u64 = 10 * 1000;
static USER_AGENT: &str = concat!("jmap-client/", env!("CARGO_PKG_VERSION"));

/// The media type of an RFC 7807 problem document, which RFC 8620 §3.6.1 uses
/// for every request-level error.
const PROBLEM_JSON_MEDIA_TYPE: &str = "application/problem+json";

/// `true` if `content_type` names [`PROBLEM_JSON_MEDIA_TYPE`].
///
/// Only the type and subtype are compared: a media type is case-insensitive and
/// the `Content-Type` value may carry parameters such as `; charset=utf-8`
/// (RFC 7231 §3.1.1.1, RFC 7807 §2), none of which change what the body is. A
/// byte-for-byte comparison would send `application/problem+json; charset=utf-8`
/// down the generic `Error::Server` path and throw the structured problem
/// details away.
fn is_problem_json(content_type: &[u8]) -> bool {
    let Ok(value) = std::str::from_utf8(content_type) else {
        return false;
    };

    value
        .split(';')
        .next()
        .unwrap_or(value)
        .trim()
        .eq_ignore_ascii_case(PROBLEM_JSON_MEDIA_TYPE)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Credentials {
    Basic(String),
    Bearer(String),
}

pub struct Client {
    // `Arc`-shared so that the `'static` WebSocket stream can observe the *live*
    // session instead of a snapshot taken when the stream was created.
    session: Arc<parking_lot::Mutex<Arc<Session>>>,
    session_url: String,
    api_url: String,
    session_updated: Arc<AtomicBool>,
    trusted_hosts: Arc<AHashSet<String>>,

    upload_url: Vec<URLPart<blob::URLParameter>>,
    download_url: Vec<URLPart<blob::URLParameter>>,
    // EventSource requires the async runtime; see the module gate in lib.rs.
    #[cfg(all(feature = "async", not(feature = "blocking")))]
    event_source_url: Vec<URLPart<crate::event_source::URLParameter>>,

    headers: header::HeaderMap,
    default_account_id: String,
    timeout: Duration,
    pub(crate) accept_invalid_certs: bool,

    #[cfg(feature = "websockets")]
    pub(crate) authorization: String,
    #[cfg(feature = "websockets")]
    pub(crate) ws: tokio::sync::Mutex<Option<crate::client_ws::WsStream>>,
}

pub struct ClientBuilder {
    credentials: Option<Credentials>,
    trusted_hosts: AHashSet<String>,
    forwarded_for: Option<String>,
    accept_invalid_certs: bool,
    timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    /// Creates a new `ClientBuilder`.
    ///
    /// Setting the credentials is required to connect to the JMAP API.
    pub fn new() -> Self {
        // reqwest is used with the provider-neutral `rustls-no-provider` feature, so the
        // provider chosen by this crate's `ring`/`aws_lc_rs` features has to be installed
        // before any TLS client is built.
        crate::crypto::ensure_crypto_provider();

        Self {
            credentials: None,
            trusted_hosts: AHashSet::new(),
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            forwarded_for: None,
            accept_invalid_certs: false,
        }
    }

    /// Set up client credentials to connect to the JMAP API.
    ///
    /// The JMAP API URL is set using the [ClientBuilder.connect()](struct.ClientBuilder.html#method.connect) method.
    ///
    /// # Bearer authentication
    /// Pass a `&str` with the API Token.
    ///
    /// ```rust
    /// Client::new().credentials("some-api-token");
    /// ```
    ///
    /// Or use the longer form by using [Credentials::bearer()](enum.Credentials.html#method.bearer).
    /// ```rust
    /// let credentials = Credentials::bearer("some-api-token");
    /// Client::new().credentials(credentials);
    /// ```
    ///
    /// # Basic authentication
    /// Pass a `(&str, &str)` tuple, with the first position containing a username and the second containing a password.
    ///
    /// **It is not suggested to use this approach in production;** instead, if possible, use [Bearer authentication](struct.ClientBuilder.html#bearer-authentication).
    ///
    /// ```rust
    /// Client::new().credentials(("user@domain.com", "password"));
    /// ```
    ///
    /// Or use the longer form by using [Credentials::basic()](enum.Credentials.html#method.basic).
    /// ```rust
    /// let credentials = Credentials::basic("user@domain.com", "password");
    /// Client::new().credentials(credentials);
    /// ```
    pub fn credentials(mut self, credentials: impl Into<Credentials>) -> Self {
        self.credentials = Some(credentials.into());
        self
    }

    /// Set a timeout for all the requests to the JMAP API.
    ///
    /// The timeout can be changed after the `Client` has been created by using [Client.set_timeout()](struct.Client.html#method.set_timeout).
    ///
    /// By default the timeout is 10 seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Accepts invalid certificates for all the requests to the JMAP API.
    ///
    /// By default certificates are validated.
    ///
    /// # Warning
    /// **It is not suggested to use this approach in production;** this method should be used only for testing and as a last resort.
    ///
    /// [Read more in the reqwest docs](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.danger_accept_invalid_certs)
    pub fn accept_invalid_certs(mut self, accept_invalid_certs: bool) -> Self {
        self.accept_invalid_certs = accept_invalid_certs;
        self
    }

    /// Set a list of trusted hosts that will be checked when a redirect is required.
    ///
    /// The list can be changed after the `Client` has been created by using [Client.set_follow_redirects()](struct.Client.html#method.set_follow_redirects).
    ///
    /// The client will follow at most 5 redirects.
    pub fn follow_redirects(
        mut self,
        trusted_hosts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.trusted_hosts = trusted_hosts.into_iter().map(|h| h.into()).collect();
        self
    }

    /// Set the originating IP address of the client connecting to the JMAP API.
    pub fn forwarded_for(mut self, forwarded_for: IpAddr) -> Self {
        self.forwarded_for = Some(match forwarded_for {
            IpAddr::V4(addr) => format!("for={}", addr),
            IpAddr::V6(addr) => format!("for=\"{}\"", addr),
        });
        self
    }

    /// Connects to the JMAP API Session URL.
    ///
    /// Setting up [Credentials](struct.ClientBuilder.html#method.credentials) must be done before calling this function.
    #[maybe_async::maybe_async]
    pub async fn connect(self, url: &str) -> crate::Result<Client> {
        let authorization = match self.credentials.expect("Missing credentials") {
            Credentials::Basic(s) => format!("Basic {}", s),
            Credentials::Bearer(s) => format!("Bearer {}", s),
        };
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&authorization).unwrap(),
        );
        if let Some(forwarded_for) = self.forwarded_for {
            headers.insert(
                header::FORWARDED,
                header::HeaderValue::from_str(&forwarded_for).unwrap(),
            );
        }

        let trusted_hosts = Arc::new(self.trusted_hosts);

        let trusted_hosts_ = trusted_hosts.clone();
        let session_url = format!("{}/.well-known/jmap", url);
        let session: Session = serde_json::from_slice(
            &Client::handle_error(
                HttpClient::builder()
                    .timeout(self.timeout)
                    .danger_accept_invalid_certs(self.accept_invalid_certs)
                    .redirect(redirect::Policy::custom(move |attempt| {
                        if attempt.previous().len() > 5 {
                            attempt.error("Too many redirects.")
                        } else if matches!( attempt.url().host_str(), Some(host) if trusted_hosts_.contains(host) )
                        {
                                attempt.follow()
                        } else {
                            let message = format!(
                                "Aborting redirect request to unknown host '{}'.",
                                attempt.url().host_str().unwrap_or("")
                            );
                            attempt.error(message)
                        }
                    }))
                    .default_headers(headers.clone())
                    .build()?
                    .get(&session_url)
                    .send()
                    .await?,
            )
            .await?
            .bytes()
            .await?,
        )?;

        let default_account_id = session
            .primary_accounts()
            .next()
            .map(|a| a.1.to_string())
            .unwrap_or_default();

        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        Ok(Client {
            download_url: URLPart::parse(session.download_url())?,
            upload_url: URLPart::parse(session.upload_url())?,
            #[cfg(all(feature = "async", not(feature = "blocking")))]
            event_source_url: URLPart::parse(session.event_source_url())?,
            api_url: session.api_url().to_string(),
            session: Arc::new(parking_lot::Mutex::new(Arc::new(session))),
            session_url,
            session_updated: Arc::new(true.into()),
            accept_invalid_certs: self.accept_invalid_certs,
            trusted_hosts,
            #[cfg(feature = "websockets")]
            authorization,
            timeout: self.timeout,
            headers,
            default_account_id,
            #[cfg(feature = "websockets")]
            ws: None.into(),
        })
    }
}

impl Client {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Builds a `Client` from an already-known `Session` without any network access.
    ///
    /// Test-only helper: the regular construction path ([ClientBuilder::connect()](struct.ClientBuilder.html#method.connect))
    /// requires a reachable server, which makes it unusable for unit tests that only
    /// need to inspect what a `Client` would send based on its session.
    #[cfg(test)]
    pub(crate) fn from_session_for_tests(session: Session) -> Self {
        let session_url = "https://example.com/.well-known/jmap".to_string();
        let default_account_id = session
            .primary_accounts()
            .next()
            .map(|a| a.1.to_string())
            .unwrap_or_else(|| "account1".to_string());

        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(USER_AGENT),
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        Client {
            download_url: URLPart::parse(session.download_url()).unwrap(),
            upload_url: URLPart::parse(session.upload_url()).unwrap(),
            #[cfg(feature = "async")]
            event_source_url: URLPart::parse(session.event_source_url()).unwrap(),
            api_url: session.api_url().to_string(),
            session: Arc::new(parking_lot::Mutex::new(Arc::new(session))),
            session_url,
            session_updated: Arc::new(true.into()),
            accept_invalid_certs: false,
            trusted_hosts: Arc::new(AHashSet::new()),
            #[cfg(feature = "websockets")]
            authorization: "Bearer test-token".to_string(),
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            headers,
            default_account_id,
            #[cfg(feature = "websockets")]
            ws: None.into(),
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = timeout;
        self
    }

    pub fn set_follow_redirects(
        &mut self,
        trusted_hosts: impl IntoIterator<Item = impl Into<String>>,
    ) -> &mut Self {
        self.trusted_hosts = Arc::new(trusted_hosts.into_iter().map(|h| h.into()).collect());
        self
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn session(&self) -> Arc<Session> {
        self.session.lock().clone()
    }

    /// Handle to the *live* session, shared with anyone who needs to observe
    /// session refreshes instead of a snapshot taken at construction time.
    #[cfg(feature = "websockets")]
    pub(crate) fn session_handle(&self) -> Arc<parking_lot::Mutex<Arc<Session>>> {
        self.session.clone()
    }

    /// Handle to the shared "session is up to date" flag.
    #[cfg(feature = "websockets")]
    pub(crate) fn session_updated_flag(&self) -> Arc<AtomicBool> {
        self.session_updated.clone()
    }

    pub fn session_url(&self) -> &str {
        &self.session_url
    }

    pub fn headers(&self) -> &header::HeaderMap {
        &self.headers
    }

    pub(crate) fn redirect_policy(&self) -> redirect::Policy {
        let trusted_hosts = self.trusted_hosts.clone();
        redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > 5 {
                attempt.error("Too many redirects.")
            } else if matches!( attempt.url().host_str(), Some(host) if trusted_hosts.contains(host) )
            {
                attempt.follow()
            } else {
                let message = format!(
                    "Aborting redirect request to unknown host '{}'.",
                    attempt.url().host_str().unwrap_or("")
                );
                attempt.error(message)
            }
        })
    }

    #[maybe_async::maybe_async]
    pub async fn send<R>(
        &self,
        request: &request::Request<'_>,
    ) -> crate::Result<response::Response<R>>
    where
        R: DeserializeOwned,
    {
        let response: response::Response<R> = serde_json::from_slice(
            &Client::handle_error(
                HttpClient::builder()
                    .redirect(self.redirect_policy())
                    .danger_accept_invalid_certs(self.accept_invalid_certs)
                    .timeout(self.timeout)
                    .default_headers(self.headers.clone())
                    .build()?
                    .post(&self.api_url)
                    .body(serde_json::to_string(&request)?)
                    .send()
                    .await?,
            )
            .await?
            .bytes()
            .await?,
        )?;

        if response.session_state() != self.session.lock().state() {
            self.session_updated.store(false, Ordering::Relaxed);
        }

        Ok(response)
    }

    #[maybe_async::maybe_async]
    pub async fn refresh_session(&self) -> crate::Result<()> {
        let session: Session = serde_json::from_slice(
            &Client::handle_error(
                HttpClient::builder()
                    .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                    .danger_accept_invalid_certs(self.accept_invalid_certs)
                    .redirect(self.redirect_policy())
                    .default_headers(self.headers.clone())
                    .build()?
                    .get(&self.session_url)
                    .send()
                    .await?,
            )
            .await?
            .bytes()
            .await?,
        )?;
        *self.session.lock() = Arc::new(session);
        self.session_updated.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn is_session_updated(&self) -> bool {
        self.session_updated.load(Ordering::Relaxed)
    }

    pub fn set_default_account_id(&mut self, defaul_account_id: impl Into<String>) -> &mut Self {
        self.default_account_id = defaul_account_id.into();
        self
    }

    pub fn default_account_id(&self) -> &str {
        &self.default_account_id
    }

    pub fn build(&self) -> Request<'_> {
        Request::new(self)
    }

    pub fn download_url(&self) -> &[URLPart<blob::URLParameter>] {
        &self.download_url
    }

    pub fn upload_url(&self) -> &[URLPart<blob::URLParameter>] {
        &self.upload_url
    }

    #[cfg(all(feature = "async", not(feature = "blocking")))]
    pub fn event_source_url(&self) -> &[URLPart<crate::event_source::URLParameter>] {
        &self.event_source_url
    }

    #[maybe_async::maybe_async]
    pub async fn handle_error(response: Response) -> crate::Result<Response> {
        if response.status().is_success() {
            Ok(response)
        } else if response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|content_type| is_problem_json(content_type.as_bytes()))
        {
            Err(Error::Problem(serde_json::from_slice(
                &response.bytes().await?,
            )?))
        } else {
            Err(Error::Server(format!("{}", response.status())))
        }
    }
}

impl Credentials {
    pub fn basic(username: &str, password: &str) -> Self {
        Credentials::Basic(base64::STANDARD.encode(format!("{}:{}", username, password)))
    }

    pub fn bearer(token: impl Into<String>) -> Self {
        Credentials::Bearer(token.into())
    }
}

impl From<&str> for Credentials {
    fn from(s: &str) -> Self {
        Credentials::bearer(s.to_string())
    }
}

impl From<String> for Credentials {
    fn from(s: String) -> Self {
        Credentials::bearer(s)
    }
}

impl From<(&str, &str)> for Credentials {
    fn from((username, password): (&str, &str)) -> Self {
        Credentials::basic(username, password)
    }
}

impl From<(String, String)> for Credentials {
    fn from((username, password): (String, String)) -> Self {
        Credentials::basic(&username, &password)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::response::{Response, TaggedMethodResponse};
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_deserialize() {
        let _r: Response<TaggedMethodResponse> = serde_json::from_slice(
            br#"{"sessionState": "123", "methodResponses": [[ "Email/query", {
                "accountId": "A1",
                "queryState": "abcdefg",
                "canCalculateChanges": true,
                "position": 0,
                "total": 101,
                "ids": [ "msg1023", "msg223", "msg110", "msg93", "msg91",
                    "msg38", "msg36", "msg33", "msg11", "msg1" ]
            }, "t0" ],
            [ "Email/get", {
                "accountId": "A1",
                "state": "123456",
                "list": [{
                    "id": "msg1023",
                    "threadId": "trd194"
                }, {
                    "id": "msg223",
                    "threadId": "trd114"
                }
                ],
                "notFound": []
            }, "t1" ],
            [ "Thread/get", {
                "accountId": "A1",
                "state": "123456",
                "list": [{
                    "id": "trd194",
                    "emailIds": [ "msg1020", "msg1021", "msg1023" ]
                }, {
                    "id": "trd114",
                    "emailIds": [ "msg201", "msg223" ]
                }
                ],
                "notFound": []
            }, "t2" ]]}"#,
        )
        .unwrap();
    }

    /// A session as `GET /.well-known/jmap` answers it (RFC 8620 §3.1),
    /// enough to build a `Client` with no network access at all.
    fn test_session() -> Session {
        serde_json::from_value(serde_json::json!({
            "capabilities": {
                "urn:ietf:params:jmap:core": {
                    "maxSizeUpload": 52_428_800,
                    "maxConcurrentUpload": 2,
                    "maxSizeRequest": 10_485_760,
                    "maxConcurrentRequests": 2,
                    "maxCallsInRequest": 16,
                    "maxObjectsInGet": 512,
                    "maxObjectsInSet": 512,
                    "collationAlgorithms": ["i;ascii-numeric", "i;unicode-casemap"]
                },
                "urn:ietf:params:jmap:mail": {
                    "maxMailboxesPerEmail": null,
                    "maxMailboxDepth": 10,
                    "maxSizeMailboxName": 46,
                    "maxSizeAttachmentsPerEmail": 26_214_400,
                    "emailQuerySortOptions": ["receivedAt", "subject", "size"],
                    "mayCreateTopLevelMailbox": true
                }
            },
            "accounts": {
                "u33084183": {
                    "name": "test@example.org",
                    "isPersonal": true,
                    "isReadOnly": false,
                    "accountCapabilities": {}
                }
            },
            "primaryAccounts": { "urn:ietf:params:jmap:mail": "u33084183" },
            "username": "test@example.org",
            "apiUrl": "https://example.com/jmap/session",
            "downloadUrl":
                "https://example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://example.com/jmap/upload/{accountId}",
            "eventSourceUrl":
                "https://example.com/jmap/eventSource?types={types}&closeafter={closeafter}&ping={ping}",
            "state": "75128aab4b1b"
        }))
        .expect("the session fixture must deserialise")
    }

    /// `ClientBuilder::default()` is the same fresh builder `Client::new()`
    /// hands back: no credentials, nothing trusted, no `Forwarded` header,
    /// certificates validated, and the ten-second default timeout.
    #[test]
    fn builder_defaults_are_the_safe_ones() {
        let builder = ClientBuilder::default();

        assert!(builder.credentials.is_none());
        assert!(builder.trusted_hosts.is_empty());
        assert!(builder.forwarded_for.is_none());
        assert!(
            !builder.accept_invalid_certs,
            "certificate validation is on until the caller turns it off"
        );
        assert_eq!(builder.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        assert_eq!(builder.timeout, Duration::from_secs(10));
    }

    /// `accept_invalid_certs` is a plain setter: both values round-trip
    /// through the builder, and the last call wins.
    #[test]
    fn accept_invalid_certs_sets_the_flag() {
        assert!(Client::new().accept_invalid_certs(true).accept_invalid_certs);
        assert!(!Client::new().accept_invalid_certs(false).accept_invalid_certs);
        assert!(
            !Client::new()
                .accept_invalid_certs(true)
                .accept_invalid_certs(false)
                .accept_invalid_certs,
            "the setter overwrites rather than latching"
        );
    }

    /// `follow_redirects` collects whatever the caller passes — borrowed
    /// or owned — into the trusted-host set the redirect policy reads.
    #[test]
    fn follow_redirects_collects_the_trusted_hosts() {
        let builder = Client::new().follow_redirects(["mail.example.org", "api.example.org"]);
        assert_eq!(builder.trusted_hosts.len(), 2);
        assert!(builder.trusted_hosts.contains("mail.example.org"));
        assert!(builder.trusted_hosts.contains("api.example.org"));

        let builder = Client::new().follow_redirects(vec!["owned.example.org".to_string()]);
        assert_eq!(builder.trusted_hosts.len(), 1);
        assert!(builder.trusted_hosts.contains("owned.example.org"));

        let builder = Client::new()
            .follow_redirects(["a.example.org"])
            .follow_redirects(Vec::<String>::new());
        assert!(
            builder.trusted_hosts.is_empty(),
            "a second call replaces the set instead of extending it"
        );
    }

    /// RFC 7239 §6.1 — an IPv6 `for=` value has to be quoted, because an
    /// unquoted address contains the `:` the header grammar uses as its
    /// own separator; an IPv4 address is written bare.
    #[test]
    fn forwarded_for_quotes_ipv6_but_not_ipv4() {
        let builder = Client::new().forwarded_for(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)));
        assert_eq!(builder.forwarded_for.as_deref(), Some("for=192.0.2.1"));

        let builder = Client::new().forwarded_for(IpAddr::V6("2001:db8::1".parse().unwrap()));
        assert_eq!(builder.forwarded_for.as_deref(), Some("for=\"2001:db8::1\""));

        let builder = Client::new().forwarded_for(IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(builder.forwarded_for.as_deref(), Some("for=\"::1\""));
    }

    /// `Client::set_follow_redirects` replaces the whole trusted-host set
    /// on a live client, which is how a caller tightens or widens the
    /// policy after connecting.
    #[test]
    fn set_follow_redirects_replaces_the_trusted_host_set() {
        let mut client = Client::from_session_for_tests(test_session());
        assert!(client.trusted_hosts.is_empty());

        client.set_follow_redirects(["mail.example.org", "api.example.org"]);
        assert_eq!(client.trusted_hosts.len(), 2);
        assert!(client.trusted_hosts.contains("mail.example.org"));

        client.set_follow_redirects(vec!["only.example.org".to_string()]);
        assert_eq!(client.trusted_hosts.len(), 1);
        assert!(
            !client.trusted_hosts.contains("mail.example.org"),
            "the previous set is replaced, not merged"
        );

        client.set_follow_redirects(Vec::<String>::new());
        assert!(client.trusted_hosts.is_empty());
    }

    /// The four `From` impls split into two families: one string is a
    /// bearer token, a two-element tuple is a Basic username/password pair
    /// encoded with the standard base64 alphabet over `user:pass`.
    #[test]
    fn credentials_from_impls_pick_bearer_or_basic() {
        assert_eq!(
            Credentials::from("some-api-token"),
            Credentials::bearer("some-api-token")
        );
        assert_eq!(
            Credentials::from("some-api-token".to_string()),
            Credentials::Bearer("some-api-token".to_string())
        );

        assert_eq!(
            Credentials::from(("user@example.com", "password")),
            Credentials::Basic("dXNlckBleGFtcGxlLmNvbTpwYXNzd29yZA==".to_string())
        );
        assert_eq!(
            Credentials::from(("user@example.com".to_string(), "password".to_string())),
            Credentials::basic("user@example.com", "password")
        );
        assert_eq!(
            Credentials::from(("user@example.com".to_string(), "password".to_string())),
            Credentials::from(("user@example.com", "password")),
            "the owned-tuple form agrees with the borrowed one byte for byte"
        );
    }

    /// `Credentials::basic` puts the `:` between user and password *inside*
    /// the base64, so the separator never appears in the clear; a token
    /// passed to `bearer` is carried verbatim.
    #[test]
    fn basic_encodes_the_separator_and_bearer_does_not_touch_the_token() {
        assert_eq!(
            Credentials::basic("a", "b"),
            Credentials::Basic("YTpi".to_string())
        );
        assert_eq!(
            Credentials::bearer("a:b"),
            Credentials::Bearer("a:b".to_string())
        );
        assert_ne!(
            Credentials::basic("ab", ""),
            Credentials::basic("a", "b"),
            "the separator position is part of the encoding"
        );
    }

    /// RFC 7807 §2 / RFC 8620 §3.6.1: a problem document is recognised by its
    /// media type alone, so parameters such as `charset` and the case of the
    /// type must not change how the error is routed.
    #[test]
    fn problem_json_media_type_ignores_parameters_and_case() {
        // The exact spelling `handle_error` used to byte-compare still matches.
        assert!(is_problem_json(b"application/problem+json"));
        // Parameters belong to the header value, not to the media type.
        assert!(is_problem_json(b"application/problem+json; charset=utf-8"));
        assert!(is_problem_json(b"application/problem+json;charset=UTF-8"));
        assert!(is_problem_json(b"application/problem+json ; p=1"));
        // Surrounding whitespace and case are tolerated (RFC 7231 §3.1.1.1).
        assert!(is_problem_json(b"  application/problem+json  "));
        assert!(is_problem_json(b"Application/Problem+JSON"));
        assert!(is_problem_json(b"APPLICATION/PROBLEM+JSON; CHARSET=UTF-8"));
        // Anything else is not a problem document.
        assert!(!is_problem_json(b"application/json"));
        assert!(!is_problem_json(b"text/plain"));
        assert!(!is_problem_json(b"application/problem+xml"));
        assert!(!is_problem_json(b"application/problem+jsonx"));
        assert!(!is_problem_json(b"text/problem+json; charset=utf-8"));
        assert!(!is_problem_json(b""));
    }
}
