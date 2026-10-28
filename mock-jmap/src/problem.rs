//! JMAP error documents.
//!
//! Two distinct shapes exist in JMAP and the client models both, so the mock must
//! be able to produce both:
//!
//! * a **problem detail** document (RFC 8620 section 3.1), returned with a
//!   non-2xx status and `Content-Type: application/problem+json`. This is what
//!   `jmap_client::Error::Problem` is built from
//!   (`jmap_client::core::error::ProblemDetails`).
//! * a **method error** (RFC 8620 section 3.4), which travels *inside* an
//!   otherwise successful `200 OK` response as
//!   `["error", { "type": "..." }, callId]` in `methodResponses`. This is what
//!   `jmap_client::Error::Method` is built from
//!   (`jmap_client::core::error::MethodError`).
//!
//! Note on the content type: `Client::handle_error` compares the header bytes
//! against `application/problem+json` exactly, so the mock must not append a
//! `; charset=utf-8` parameter.

use serde_json::{json, Value};

use crate::http::HttpResponse;

/// Problem-detail types from RFC 8620 section 3.1 that the client recognises
/// explicitly (`jmap_client::core::error::JMAPError`).
pub mod problem_type {
    pub const UNKNOWN_CAPABILITY: &str = "urn:ietf:params:jmap:error:unknownCapability";
    pub const NOT_JSON: &str = "urn:ietf:params:jmap:error:notJSON";
    pub const NOT_REQUEST: &str = "urn:ietf:params:jmap:error:notRequest";
    pub const LIMIT: &str = "urn:ietf:params:jmap:error:limit";

    /// RFC 8620 section 3.1 requires a `401 Unauthorized` for missing/invalid
    /// credentials but does not register a URN for it, so the mock uses the
    /// bare token. The client deserialises it as `ProblemType::Other`.
    pub const UNAUTHORIZED: &str = "unauthorized";

    /// RFC 8620 section 5.1 upload rejection. The RFC problem type is
    /// `urn:ietf:params:jmap:error:limit`; `cannotUploadBlob` is the
    /// implementation-flavoured alias servers such as Stalwart return. Both are
    /// exposed so tests can pick.
    pub const CANNOT_UPLOAD_BLOB: &str = "cannotUploadBlob";

    /// Returned by routes reserved for a later phase (EventSource, WebSocket).
    pub const NOT_IMPLEMENTED: &str = "notImplemented";

    /// Generic "resource does not exist" problem (client: `ProblemType::Other`).
    pub const NOT_FOUND: &str = "notFound";
}

/// Method-error types (RFC 8620 section 3.4 + RFC 8621 / draft-jmap-sieve).
///
/// Mirrors `jmap_client::core::error::MethodErrorType`; anything outside this
/// list fails to deserialise on the client, so prefer these constants.
pub mod method_error_type {
    pub const SERVER_UNAVAILABLE: &str = "serverUnavailable";
    pub const SERVER_FAIL: &str = "serverFail";
    pub const SERVER_PARTIAL_FAIL: &str = "serverPartialFail";
    pub const UNKNOWN_METHOD: &str = "unknownMethod";
    pub const INVALID_ARGUMENTS: &str = "invalidArguments";
    pub const INVALID_RESULT_REFERENCE: &str = "invalidResultReference";
    pub const FORBIDDEN: &str = "forbidden";
    pub const ACCOUNT_NOT_FOUND: &str = "accountNotFound";
    pub const ACCOUNT_NOT_SUPPORTED_BY_METHOD: &str = "accountNotSupportedByMethod";
    pub const ACCOUNT_READ_ONLY: &str = "accountReadOnly";
    pub const REQUEST_TOO_LARGE: &str = "requestTooLarge";
    pub const CANNOT_CALCULATE_CHANGES: &str = "cannotCalculateChanges";
    pub const STATE_MISMATCH: &str = "stateMismatch";
    pub const ALREADY_EXISTS: &str = "alreadyExists";
    pub const FROM_ACCOUNT_NOT_FOUND: &str = "fromAccountNotFound";
    pub const FROM_ACCOUNT_NOT_SUPPORTED_BY_METHOD: &str = "fromAccountNotSupportedByMethod";
    pub const ANCHOR_NOT_FOUND: &str = "anchorNotFound";
    pub const UNSUPPORTED_SORT: &str = "unsupportedSort";
    pub const UNSUPPORTED_FILTER: &str = "unsupportedFilter";
    pub const TOO_MANY_CHANGES: &str = "tooManyChanges";
}

/// Builds a problem-detail document.
pub fn problem(type_: &str, status: u16, title: &str, detail: Option<&str>) -> Value {
    let mut doc = json!({
        "type": type_,
        "status": status,
        "title": title,
    });
    if let Some(detail) = detail {
        doc.as_object_mut()
            .unwrap()
            .insert("detail".to_string(), json!(detail));
    }
    doc
}

/// Builds a problem-detail HTTP response.
pub fn problem_response(
    status: u16,
    type_: &str,
    title: &str,
    detail: Option<&str>,
) -> HttpResponse {
    HttpResponse::raw(
        status,
        "application/problem+json",
        problem(type_, status, title, detail)
            .to_string()
            .into_bytes(),
    )
}

/// Builds a method-error object (`{"type": ..., extras}`).
///
/// `extras` are optional additional members (`detail`, `canCalculateChanges`,
/// ...). The client only reads `type`, but real servers send more and tests may
/// want to assert on it.
pub fn method_error(type_: &str, extras: Vec<(&str, Value)>) -> Value {
    let mut obj = json!({ "type": type_ });
    for (key, value) in extras {
        obj.as_object_mut().unwrap().insert(key.to_string(), value);
    }
    obj
}

/// Builds a `["error", {...}, callId]` method response.
pub fn tagged_method_error(type_: &str, call_id: &str, extras: Vec<(&str, Value)>) -> Value {
    json!(["error", method_error(type_, extras), call_id])
}
