//! # mock-jmap
//!
//! An RFC-compliant, in-memory **JMAP mock server** that acts as the shared test
//! infrastructure for [`jmap_client`]. It speaks plain HTTP (no TLS) on an
//! ephemeral loopback port and implements the parts of the JMAP RFCs the client
//! exercises:
//!
//! * **Session resource** — `GET /.well-known/jmap` per RFC 8620 §3.1 plus the
//!   mail/submission/vacationresponse/websocket/sieve capability blocks of
//!   RFC 8621, RFC 8887 and draft-ietf-jmap-sieve-12
//!   ([`session`]).
//! * **Method calls** — `POST /jmap` with `using` validation, ordered
//!   `methodResponses`, result-reference resolution and `createdIds` back
//!   references ([`dispatch`]).
//! * **Method handlers** — every method of `jmap_client::Method` answers with
//!   coherent fixture data ([`handlers`]).
//! * **Blobs** — `POST /upload/{accountId}` and
//!   `GET /download/{accountId}/{blobId}/{name}` per RFC 8620 §5.1/§5.2.
//! * **Auth** — every route requires an `Authorization` header with the
//!   `Basic` or `Bearer` scheme; anything else is a `401`
//!   `application/problem+json`.
//! * **Fault injection** — one-shot or N-times overrides for problem documents,
//!   method errors, and raw non-JSON bodies ([`MockHandle`]).
//!
//! ## Quick start
//!
//! ```no_run
//! use mock_jmap::MockJmapServer;
//!
//! let server = MockJmapServer::start();
//! // Point a JMAP client at `server.base_url()`; the fixture account is
//! // `test@example.org` (see [`mock_jmap::fixture_authorization`]) and any
//! // `Basic`/`Bearer` `Authorization` header is accepted.
//! assert!(server.base_url().starts_with("http://127.0.0.1:"));
//!
//! // Read the session document the mock serves.
//! let session = server.handle().session_json();
//! assert_eq!(session["username"], "test@example.org");
//!
//! // Make the next `Mailbox/get` fail with a method error.
//! let handle = server.handle();
//! handle.inject_method_error("Mailbox/get", "serverFail", 1);
//! ```
//!
//! ## API map
//!
//! * [`MockJmapServer`] — start/stop, urls, port, request log.
//! * [`MockHandle`] — cloneable control handle: fault injection, store access.
//! * [`RequestRecord`] — one recorded request (`method`, `path`, `body`).
//! * [`session::session_json`] — the session document, also useful standalone.
//! * [`problem`] — builders for problem documents and method errors.
//!
//! ## Phases
//!
//! This crate currently covers the HTTP core plus blob upload/download (plan P1).
//! EventSource and WebSocket (plan P2) attach at the reserved extension points:
//! [`http::HttpResponse::streaming`] and the `/ws` route, which today answers
//! `501 notImplemented`.

#![forbid(unsafe_code)]

pub mod dispatch;
pub mod eventsource;
pub mod handlers;
pub mod http;
pub mod problem;
pub mod push;
pub mod session;
pub mod store;
pub mod websocket;

use std::collections::BTreeMap;
use std::net::TcpListener as StdTcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::http::{HttpRequest, HttpResponse};
use crate::problem::problem_type::{
    CANNOT_UPLOAD_BLOB, NOT_FOUND, NOT_WEBSOCKET_UPGRADE, UNAUTHORIZED,
};
use crate::problem::problem_response;
use crate::push::PushBus;

/// The push notification types the mock emits, re-exported so tests can build
/// them without reaching into the module.
pub use crate::push::{Changed, PushEvent};
use crate::session::{session_json, ACCOUNT_ID, MAX_SIZE_UPLOAD};
use crate::store::Store;

/// One recorded request, for assertions in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    /// HTTP method (`GET`, `POST`, ...).
    pub method: String,
    /// Request path (no query string).
    pub path: String,
    /// Raw request body.
    pub body: Vec<u8>,
}

/// A pending problem-document override.
#[derive(Debug, Clone)]
struct ProblemInjection {
    path_substr: String,
    status: u16,
    problem_type: String,
    remaining: usize,
}

/// A pending raw-response override (for client-side parse-error tests).
#[derive(Debug, Clone)]
struct RawInjection {
    path_substr: String,
    status: u16,
    content_type: String,
    body: Vec<u8>,
    remaining: usize,
}

/// Everything the mock server keeps in memory.
#[derive(Debug, Clone)]
pub struct Shared {
    /// The JMAP data store.
    pub store: Store,
    /// `http://127.0.0.1:<port>`.
    pub base_url: String,
    /// `ws://127.0.0.1:<port>/ws`.
    pub ws_url: String,
    /// Problem-document overrides, consumed before normal handling.
    pub(crate) problems: Vec<ProblemInjection>,
    /// Method-error overrides keyed by JMAP method name.
    pub method_errors: Vec<(String, String, usize)>,
    /// Raw-body overrides, consumed before everything else.
    pub(crate) raw: Vec<RawInjection>,
    /// Upload limit enforced by `POST /upload/{accountId}`.
    ///
    /// Defaults to [`session::MAX_SIZE_UPLOAD`]; tests lower it to exercise the
    /// oversize path without allocating 50 MiB.
    pub max_size_upload: usize,
    /// Every request the mock has seen.
    pub log: Vec<RequestRecord>,
    /// The fan-out bus behind EventSource and WebSocket push.
    pub push: PushBus,
}

impl Shared {
    fn take_problem(&mut self, path: &str) -> Option<(u16, String)> {
        let index = self
            .problems
            .iter()
            .position(|inj| inj.remaining > 0 && path.contains(&inj.path_substr))?;
        let mut inj = self.problems.remove(index);
        inj.remaining -= 1;
        let result = (inj.status, inj.problem_type.clone());
        if inj.remaining > 0 {
            self.problems.push(inj);
        }
        Some(result)
    }

    fn take_raw(&mut self, path: &str) -> Option<HttpResponse> {
        let index = self
            .raw
            .iter()
            .position(|inj| inj.remaining > 0 && path.contains(&inj.path_substr))?;
        let mut inj = self.raw.remove(index);
        inj.remaining -= 1;
        let response = HttpResponse::raw(
            inj.status,
            inj.content_type.clone(),
            inj.body.clone(),
        );
        if inj.remaining > 0 {
            self.raw.push(inj);
        }
        Some(response)
    }
}

/// A cloneable control handle to a running [`MockJmapServer`].
///
/// Use it to inject faults, inspect or mutate the data store, and read the
/// request log. Cloning is cheap: every clone talks to the same server state.
#[derive(Clone)]
pub struct MockHandle {
    inner: Arc<Mutex<Shared>>,
}

impl MockHandle {
    /// Answers requests whose path contains `path_substr` with a problem
    /// document of `type_` and `status`, for the next `times` requests.
    ///
    /// ```no_run
    /// # use mock_jmap::MockJmapServer;
    /// let server = MockJmapServer::start();
    /// server.handle().inject_problem("/jmap", 513, "overQuota", 2);
    /// ```
    pub fn inject_problem(&self, path_substr: &str, status: u16, type_: &str, times: usize) {
        self.inner.lock().unwrap().problems.push(ProblemInjection {
            path_substr: path_substr.to_string(),
            status,
            problem_type: type_.to_string(),
            remaining: times,
        });
    }

    /// Makes the next `times` calls of JMAP method `method` answer with a
    /// `["error", {"type": type_}, callId]` method response.
    pub fn inject_method_error(&self, method: &str, type_: &str, times: usize) {
        self.inner
            .lock()
            .unwrap()
            .method_errors
            .push((method.to_string(), type_.to_string(), times));
    }

    /// Responds with raw bytes (any content type) for the next `times` requests
    /// whose path contains `path_substr`.
    ///
    /// This is how a test drives the client's *own* JSON parsing failure
    /// (`jmap_client::Error::Parse`), which is distinct from the server
    /// answering `urn:ietf:params:jmap:error:notJSON`.
    pub fn inject_raw(
        &self,
        path_substr: &str,
        status: u16,
        content_type: &str,
        body: &[u8],
        times: usize,
    ) {
        self.inner.lock().unwrap().raw.push(RawInjection {
            path_substr: path_substr.to_string(),
            status,
            content_type: content_type.to_string(),
            body: body.to_vec(),
            remaining: times,
        });
    }

    /// Every request the mock has served so far.
    pub fn requests_log(&self) -> Vec<RequestRecord> {
        self.inner.lock().unwrap().log.clone()
    }

    /// Drops the request log.
    pub fn clear_log(&self) {
        self.inner.lock().unwrap().log.clear();
    }

    /// Removes every pending fault injection.
    pub fn clear_injections(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.problems.clear();
        guard.method_errors.clear();
        guard.raw.clear();
    }

    /// Read-only access to the data store.
    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> T) -> T {
        f(&self.inner.lock().unwrap().store)
    }

    /// Mutable access to the data store.
    pub fn with_store_mut<T>(&self, f: impl FnOnce(&mut Store) -> T) -> T {
        f(&mut self.inner.lock().unwrap().store)
    }

    /// Publishes `event` to every open EventSource stream and every
    /// push-enabled WebSocket, and returns the id the bus assigned.
    ///
    /// This is the primitive the other push helpers are built on. Use it when a
    /// test wants an exact payload:
    ///
    /// ```no_run
    /// # use mock_jmap::{MockJmapServer, PushEvent};
    /// let server = MockJmapServer::start();
    /// let id = server.handle().push_event(PushEvent::state_change_one(
    ///     "account1",
    ///     "Mailbox",
    ///     "42",
    /// ));
    /// assert_eq!(id, 1);
    /// ```
    pub fn push_event(&self, event: PushEvent) -> u64 {
        self.inner.lock().unwrap().push.publish(event)
    }

    /// Pushes a `StateChange` for one data type of the fixture account.
    ///
    /// The state string is whatever the caller passes, so a test can assert on
    /// the exact `changed` map without touching the store.
    pub fn trigger_state_change(&self, data_type: &str, new_state: &str) -> u64 {
        self.push_event(PushEvent::state_change_one(ACCOUNT_ID, data_type, new_state))
    }

    /// Pushes a `StateChange` for an explicit `accountId -> { type -> state }`
    /// map (RFC 8620 §7.1).
    pub fn broadcast_state_change(&self, changed: Changed) -> u64 {
        self.push_event(PushEvent::state_change(changed))
    }

    /// Changes the store the way *another* user's client would, then tells every
    /// open stream about it.
    ///
    /// `f` mutates the store and returns the JMAP type names it touched; the
    /// mock advances the session state and publishes one `StateChange` carrying
    /// each named collection's new state string. This is the `mejl-fake`
    /// pattern: from the client's side of the wire the data moved on its own.
    ///
    /// ```no_run
    /// # use mock_jmap::MockJmapServer;
    /// let server = MockJmapServer::start();
    /// server.handle().mutate_from_elsewhere(|store| {
    ///     store.mailboxes.create(serde_json::json!({ "name": "From elsewhere" }));
    ///     vec!["Mailbox".to_string()]
    /// });
    /// ```
    pub fn mutate_from_elsewhere<F>(&self, f: F) -> u64
    where
        F: FnOnce(&mut Store) -> Vec<String>,
    {
        let mut guard = self.inner.lock().unwrap();
        let touched = f(&mut guard.store);
        guard.store.touch();

        let mut inner: BTreeMap<String, String> = BTreeMap::new();
        for data_type in touched {
            if let Some(collection) = guard.store.collection(&data_type) {
                inner.insert(data_type.clone(), collection.state_str());
            }
        }
        let mut changed = Changed::new();
        changed.insert(ACCOUNT_ID.to_string(), inner);
        guard.push.publish(PushEvent::state_change(changed))
    }

    /// Pushes an `EmailPush` for the fixture account.
    ///
    /// `email` is the RFC 8621 §4.1.1 metadata a client uses to decide whether
    /// to fetch the message. See the plan's Findings log before sending this
    /// over an EventSource stream: `jmap_client`'s SSE parser only understands
    /// `StateChange` and `CalendarAlert`, so `EmailPush` is a WebSocket-only
    /// notification in practice.
    pub fn push_email(&self, email: Value) -> u64 {
        self.push_event(PushEvent::email_push(ACCOUNT_ID, email))
    }

    /// Delivers [`push_email`] with the metadata of a stored email.
    pub fn push_email_of(&self, email_id: &str) -> Option<u64> {
        let payload = {
            let guard = self.inner.lock().unwrap();
            guard
                .store
                .emails
                .get(email_id)
                .map(crate::push::email_push_payload)
        };
        payload.map(|email| self.push_email(email))
    }

    /// How many streams are attached to the bus right now.
    ///
    /// Every open EventSource counts, and so does every WebSocket from the
    /// moment it upgrades — a socket that has not sent `WebSocketPushEnable`
    /// still holds a receiver, it just drops what arrives.
    pub fn push_subscriber_count(&self) -> usize {
        self.inner.lock().unwrap().push.subscriber_count()
    }

    /// Id of the most recent push event, 0 if nothing has been pushed.
    pub fn last_push_id(&self) -> u64 {
        self.inner.lock().unwrap().push.last_id()
    }

    /// Overrides the upload limit the mock enforces (see [`Shared::max_size_upload`]).
    pub fn set_max_size_upload(&self, bytes: usize) {
        self.inner.lock().unwrap().max_size_upload = bytes;
    }

    /// The session document this mock serves.
    pub fn session_json(&self) -> Value {
        let guard = self.inner.lock().unwrap();
        session_json(&guard.base_url, &guard.ws_url)
    }

    /// `http://127.0.0.1:<port>`.
    pub fn base_url(&self) -> String {
        self.inner.lock().unwrap().base_url.clone()
    }

    /// `ws://127.0.0.1:<port>/ws`.
    pub fn ws_url(&self) -> String {
        self.inner.lock().unwrap().ws_url.clone()
    }
}

/// A running JMAP mock.
///
/// The server owns a background OS thread with its own tokio current-thread
/// runtime, so it works inside `#[test]`, `#[tokio::test]` and plain `main`
/// alike. Dropping (or calling [`MockJmapServer::shutdown`]) stops the thread.
pub struct MockJmapServer {
    handle: MockHandle,
    base_url: String,
    ws_url: String,
    port: u16,
    shutdown: Arc<Notify>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl MockJmapServer {
    /// Starts a mock with the standard fixture set.
    pub fn start() -> MockJmapServer {
        Self::with_store(Store::new())
    }

    /// Starts a mock seeded with a caller-provided store (e.g. [`Store::empty`]).
    pub fn with_store(store: Store) -> MockJmapServer {
        let std_listener =
            StdTcpListener::bind("127.0.0.1:0").expect("mock-jmap: bind 127.0.0.1:0 failed");
        std_listener
            .set_nonblocking(true)
            .expect("mock-jmap: set_nonblocking failed");
        let port = std_listener
            .local_addr()
            .expect("mock-jmap: local_addr failed")
            .port();
        let base_url = format!("http://127.0.0.1:{port}");
        let ws_url = format!("ws://127.0.0.1:{port}/ws");

        let shared = Arc::new(Mutex::new(Shared {
            store,
            base_url: base_url.clone(),
            ws_url: ws_url.clone(),
            problems: Vec::new(),
            method_errors: Vec::new(),
            raw: Vec::new(),
            max_size_upload: MAX_SIZE_UPLOAD,
            log: Vec::new(),
            push: PushBus::new(),
        }));
        let shutdown = Arc::new(Notify::new());

        let loop_shared = Arc::clone(&shared);
        let loop_shutdown = Arc::clone(&shutdown);
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("mock-jmap: failed to build tokio runtime");
            runtime.block_on(async move {
                let listener = TcpListener::from_std(std_listener).expect("mock-jmap: from_std failed");
                accept_loop(listener, loop_shared, loop_shutdown).await;
            });
        });

        MockJmapServer {
            handle: MockHandle { inner: shared },
            base_url,
            ws_url,
            port,
            shutdown,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// `http://127.0.0.1:<port>` — pass this to `Client::connect`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `ws://127.0.0.1:<port>/ws` — the WebSocket endpoint advertised in the
    /// session resource.
    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }

    /// The ephemeral port the mock bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// A cloneable control handle (fault injection, store access, log).
    pub fn handle(&self) -> MockHandle {
        self.handle.clone()
    }

    /// Every request the mock has served so far.
    pub fn requests_log(&self) -> Vec<RequestRecord> {
        self.handle.requests_log()
    }

    /// Stops the background server and joins its thread.
    pub fn shutdown(self) {
        self.stop();
    }

    fn stop(&self) {
        self.shutdown.notify_one();
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

impl Drop for MockJmapServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn accept_loop(listener: TcpListener, shared: Arc<Mutex<Shared>>, shutdown: Arc<Notify>) {
    loop {
        tokio::select! {
            _ = shutdown.notified() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _peer)) => {
                    let shared = Arc::clone(&shared);
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, shared).await;
                    });
                }
                Err(_) => break,
            },
        }
    }
}

async fn serve_connection(stream: tokio::net::TcpStream, shared: Arc<Mutex<Shared>>) {
    let mut stream = stream;
    let Ok(raw) =
        tokio::time::timeout(Duration::from_secs(60), http::read_request_raw(&mut stream)).await
    else {
        return;
    };
    let Ok(Some(raw)) = raw else {
        return;
    };

    // A WebSocket upgrade takes the socket over for good; nothing below may
    // answer it with a plain HTTP response.
    if raw.request.method == "GET"
        && raw.request.path == "/ws"
        && websocket::is_upgrade(&raw.request)
    {
        shared
            .lock()
            .expect("mock-jmap: shared state poisoned")
            .log
            .push(RequestRecord {
                method: raw.request.method.clone(),
                path: raw.request.path.clone(),
                body: raw.request.body.clone(),
            });
        websocket::serve(shared, stream, raw).await;
        return;
    }

    let request = raw.request;
    let response = {
        let mut guard = shared.lock().expect("mock-jmap: shared state poisoned");
        guard.log.push(RequestRecord {
            method: request.method.clone(),
            path: request.path.clone(),
            body: request.body.clone(),
        });
        route(&mut guard, &request)
    };

    // A streaming response is a routing decision, not a body: the EventSource
    // leg owns the socket from here and keeps writing chunks.
    if response.is_streaming() {
        let _ = eventsource::serve(&shared, &mut stream, &request, &response.body).await;
        return;
    }

    let _ = http::write_response(&mut stream, &response).await;
}

/// Routes one request. The caller holds the state lock.
pub fn route(shared: &mut Shared, request: &HttpRequest) -> HttpResponse {
    // RFC 8620 3.1: unauthenticated requests are rejected before anything else.
    if !request.is_authenticated() {
        return problem_response(
            401,
            UNAUTHORIZED,
            "Missing or invalid Authorization header",
            None,
        );
    }

    if let Some(response) = shared.take_raw(&request.path) {
        return response;
    }
    if let Some((status, problem_type)) = shared.take_problem(&request.path) {
        return problem_response(
            status,
            &problem_type,
            "Injected problem response",
            Some(&request.path),
        );
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/.well-known/jmap") => {
            HttpResponse::json(200, session_json(&shared.base_url, &shared.ws_url))
        }
        ("POST", "/jmap") => dispatch::handle_jmap_post(shared, request),
        ("POST", path) if path.starts_with("/upload/") => handle_upload(shared, request),
        ("GET", path) if path.starts_with("/download/") => handle_download(shared, request),
        ("GET", "/eventSource") | ("GET", "/eventSource/") => eventsource::route_marker(),
        // A real upgrade never reaches this line: `serve_connection` hands the
        // socket to [`websocket::serve`] first. Landing here means the client
        // asked for `/ws` without upgrading.
        ("GET", "/ws") => problem_response(
            400,
            NOT_WEBSOCKET_UPGRADE,
            "Not a WebSocket upgrade request",
            Some("Send `Upgrade: websocket` and `Connection: Upgrade` to open the JMAP WebSocket"),
        ),
        _ => problem_response(404, NOT_FOUND, "No such route", Some(&request.path)),
    }
}

/// `POST /upload/{accountId}` — RFC 8620 §5.1.
fn handle_upload(shared: &mut Shared, request: &HttpRequest) -> HttpResponse {
    let account_id = request.path.trim_start_matches("/upload/").trim_end_matches('/');
    if account_id != ACCOUNT_ID {
        return problem_response(404, NOT_FOUND, "Unknown account", Some(account_id));
    }
    if request.body.len() > shared.max_size_upload {
        return problem_response(
            413,
            CANNOT_UPLOAD_BLOB,
            "Blob exceeds the server's maxSizeUpload",
            Some(&format!(
                "{} bytes uploaded, maxSizeUpload is {}",
                request.body.len(),
                shared.max_size_upload
            )),
        );
    }
    let content_type = request
        .header("content-type")
        .unwrap_or("application/octet-stream")
        .to_string();
    let size = request.body.len();
    let blob_id = shared.store.blobs.put(request.body.clone(), content_type.clone());
    shared.store.touch();

    HttpResponse::json(
        200,
        json!({
            "accountId": account_id,
            "blobId": blob_id,
            "type": content_type,
            "size": size,
        }),
    )
}

/// `GET /download/{accountId}/{blobId}/{name}` — RFC 8620 §5.2.
fn handle_download(shared: &mut Shared, request: &HttpRequest) -> HttpResponse {
    let rest = request.path.trim_start_matches("/download/");
    let mut parts = rest.split('/');
    let account_id = parts.next().unwrap_or_default();
    let blob_id = parts.next().unwrap_or_default();

    if account_id != ACCOUNT_ID {
        return problem_response(404, NOT_FOUND, "Unknown account", Some(account_id));
    }
    match shared.store.blobs.get(blob_id) {
        Some(blob) => HttpResponse::raw(200, blob.content_type.clone(), blob.bytes.clone()),
        None => problem_response(404, NOT_FOUND, "Blob not found", Some(blob_id)),
    }
}

/// The `Authorization` value the mock accepts for the fixture account, so tests
/// do not have to build it themselves.
pub fn fixture_authorization() -> (&'static str, &'static str) {
    ("test@example.org", "secret")
}
