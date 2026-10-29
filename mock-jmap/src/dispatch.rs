//! `POST /jmap` — the JMAP method-call dispatcher.
//!
//! Implements RFC 8620 sections 3.2 (request envelope), 3.4 (response
//! envelope), 3.6 (result references) and 5.3 (creation ids and back
//! references):
//!
//! * `using` is validated against the capabilities the mock advertises; an
//!   unknown entry answers `400` with
//!   `urn:ietf:params:jmap:error:unknownCapability`.
//! * a body that is not JSON answers `400 urn:ietf:params:jmap:error:notJSON`;
//!   valid JSON that is not a JMAP request answers
//!   `400 urn:ietf:params:jmap:error:notRequest`.
//! * each method call is dispatched through [`crate::handlers`]; `methodResponses`
//!   preserves request order and call ids.
//! * a handler failure becomes a `["error", {...}, callId]` entry, never a
//!   top-level problem document.
//! * result references (`{"resultOf","name","path"}`) are resolved against
//!   responses produced earlier in the *same* request before the handler runs.
//! * `#creationId` back references are resolved from the `created` maps of
//!   earlier Set-style responses in the same request, and the accumulated map is
//!   echoed back as the response's `createdIds`.

use ahash::AHashMap;
use serde_json::{json, Map, Value};

use crate::handlers;
use crate::http::{HttpRequest, HttpResponse};
use crate::problem::method_error_type::{INVALID_ARGUMENTS, INVALID_RESULT_REFERENCE};
use crate::problem::problem_type::{NOT_JSON, NOT_REQUEST, UNKNOWN_CAPABILITY};
use crate::problem::{method_error, problem_response};
use crate::Shared;

/// Everything a method handler is allowed to look at.
pub struct Ctx<'a> {
    /// The mock's data store.
    pub store: &'a mut crate::store::Store,
    /// The call arguments, with all references already resolved.
    pub args: Value,
    /// Creation ids accumulated so far in this request (`creationId -> id`).
    pub created_ids: &'a AHashMap<String, String>,
    /// HTTP base url of the running mock.
    pub base_url: String,
    /// WebSocket url of the running mock.
    pub ws_url: String,
}

/// The result of running one method call.
#[derive(Debug, Clone)]
pub enum CallOutcome {
    /// A normal method response object.
    Ok(Value),
    /// A method error object (`{"type": "..."}`), tagged as `error` by the
    /// dispatcher.
    Err(Value),
}

impl CallOutcome {
    /// A method error with no extra members.
    pub fn error(type_: &str) -> Self {
        CallOutcome::Err(method_error(type_, Vec::new()))
    }

    /// A method error with extra members.
    pub fn error_with(type_: &str, extras: Vec<(&str, Value)>) -> Self {
        CallOutcome::Err(method_error(type_, extras))
    }
}

/// A *request level* failure: the whole envelope was rejected, as opposed to a
/// single method call answering an error.
///
/// Over HTTP this is the `application/problem+json` document of RFC 8620 §3.2;
/// over the WebSocket it is the `RequestError` frame of RFC 8887 §4.4. Same
/// semantics, two encodings — which is why [`dispatch_request`] reports it as
/// this type instead of an [`HttpResponse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestProblem {
    /// HTTP status the problem carries (`400` for the JMAP request errors).
    pub status: u16,
    /// Problem type URI, e.g. `urn:ietf:params:jmap:error:unknownCapability`.
    pub problem_type: String,
    /// Human readable summary.
    pub title: String,
    /// Optional detail.
    pub detail: Option<String>,
}

impl RequestProblem {
    pub fn new(status: u16, problem_type: &str, title: &str, detail: Option<String>) -> Self {
        Self {
            status,
            problem_type: problem_type.to_string(),
            title: title.to_string(),
            detail,
        }
    }

    /// The `application/problem+json` HTTP response (RFC 8620 §3.2).
    pub fn to_response(&self) -> HttpResponse {
        problem_response(
            self.status,
            &self.problem_type,
            &self.title,
            self.detail.as_deref(),
        )
    }

    /// The `RequestError` WebSocket frame (RFC 8887 §4.4).
    pub fn to_ws_frame(&self) -> Value {
        let mut frame = Map::new();
        frame.insert("@type".to_string(), json!("RequestError"));
        frame.insert("type".to_string(), json!(&self.problem_type));
        frame.insert("status".to_string(), json!(self.status));
        frame.insert("title".to_string(), json!(&self.title));
        if let Some(detail) = &self.detail {
            frame.insert("detail".to_string(), json!(detail));
        }
        Value::Object(frame)
    }
}

/// Handles `POST /jmap`.
pub fn handle_jmap_post(shared: &mut Shared, req: &HttpRequest) -> HttpResponse {
    let root: Value = match serde_json::from_slice(&req.body) {
        Ok(value) => value,
        Err(err) => {
            return problem_response(
                400,
                NOT_JSON,
                "Request body is not valid JSON",
                Some(&err.to_string()),
            )
        }
    };

    match dispatch_request(shared, root) {
        Ok(body) => HttpResponse::json(200, body),
        Err(problem) => problem.to_response(),
    }
}

/// Runs a JMAP request envelope and returns the response body.
///
/// Transport independent: `POST /jmap` (RFC 8620 §3.2) and the WebSocket
/// `Request` frame (RFC 8887 §4.2) carry the same members, so both go through
/// here and the WebSocket path never re-implements `using` validation, ordering,
/// result references or `createdIds`.
///
/// On success the value is the response envelope:
/// `{"methodResponses": [...], "sessionState": "..."}`, plus `createdIds` when
/// something was created. A rejected envelope is a [`RequestProblem`], which the
/// caller renders for its transport.
pub fn dispatch_request(shared: &mut Shared, root: Value) -> Result<Value, RequestProblem> {
    let Some(root_obj) = root.as_object() else {
        return Err(RequestProblem::new(
            400,
            NOT_REQUEST,
            "Request is not a JSON object",
            None,
        ));
    };

    let Some(using) = root_obj.get("using").and_then(Value::as_array) else {
        return Err(RequestProblem::new(
            400,
            NOT_REQUEST,
            "Request has no \"using\" array",
            None,
        ));
    };
    let Some(calls) = root_obj.get("methodCalls").and_then(Value::as_array) else {
        return Err(RequestProblem::new(
            400,
            NOT_REQUEST,
            "Request has no \"methodCalls\" array",
            None,
        ));
    };

    let advertised = crate::session::advertised_capabilities();
    for capability in using.iter().filter_map(Value::as_str) {
        if !advertised.contains(&capability) {
            return Err(RequestProblem::new(
                400,
                UNKNOWN_CAPABILITY,
                "Request advertises a capability the server does not support",
                Some(capability.to_string()),
            ));
        }
    }

    // Creation ids supplied by the client from a previous request remain valid
    // for back references (RFC 8620 section 5.3).
    let mut created_ids: AHashMap<String, String> = root_obj
        .get("createdIds")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| value.as_str().map(|v| (key.clone(), v.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let mut newly_created: Map<String, Value> = Map::new();

    let base_url = shared.base_url.clone();
    let ws_url = shared.ws_url.clone();
    let mut pending_method_errors = std::mem::take(&mut shared.method_errors);

    let mut responses: Vec<Value> = Vec::with_capacity(calls.len());
    let mut produced: AHashMap<String, (String, Value)> = AHashMap::new();

    for call in calls {
        let Some(entry) = call.as_array() else {
            shared.method_errors = pending_method_errors;
            return Err(RequestProblem::new(
                400,
                NOT_REQUEST,
                "methodCalls entries must be [name, args, callId] triples",
                None,
            ));
        };
        let name = entry.first().and_then(Value::as_str);
        let args = entry.get(1);
        let call_id = entry.get(2).and_then(Value::as_str);
        let (Some(name), Some(args), Some(call_id)) = (name, args, call_id) else {
            shared.method_errors = pending_method_errors;
            return Err(RequestProblem::new(
                400,
                NOT_REQUEST,
                "methodCalls entries must be [name, args, callId] triples",
                None,
            ));
        };

        if let Some(injected) = take_method_error(&mut pending_method_errors, name) {
            push_error(&mut responses, &mut produced, call_id, injected);
            continue;
        }

        let resolved = match resolve_value(args, &produced, &created_ids) {
            Ok(value) => value,
            Err(err) => {
                push_error(&mut responses, &mut produced, call_id, err);
                continue;
            }
        };

        let ctx = Ctx {
            store: &mut shared.store,
            args: resolved,
            created_ids: &created_ids,
            base_url: base_url.clone(),
            ws_url: ws_url.clone(),
        };

        match handlers::dispatch(name, ctx) {
            CallOutcome::Ok(value) => {
                if let Some(created) = value.get("created").and_then(Value::as_object) {
                    for (creation_id, object) in created {
                        if let Some(id) = object.get("id").and_then(Value::as_str) {
                            created_ids.insert(creation_id.clone(), id.to_string());
                            newly_created.insert(creation_id.clone(), json!(id));
                        }
                    }
                }
                produced.insert(call_id.to_string(), (name.to_string(), value.clone()));
                responses.push(json!([name, value, call_id]));
            }
            CallOutcome::Err(err) => push_error(&mut responses, &mut produced, call_id, err),
        }
    }

    shared.method_errors = pending_method_errors;

    let session_state = shared.store.session_state_str();
    let mut body = json!({
        "methodResponses": responses,
        "sessionState": session_state,
    });
    if !newly_created.is_empty() {
        body.as_object_mut()
            .unwrap()
            .insert("createdIds".to_string(), Value::Object(newly_created));
    }

    Ok(body)
}

fn push_error(
    responses: &mut Vec<Value>,
    produced: &mut AHashMap<String, (String, Value)>,
    call_id: &str,
    err: Value,
) {
    produced.insert(call_id.to_string(), ("error".to_string(), err.clone()));
    responses.push(json!(["error", err, call_id]));
}

fn take_method_error(injections: &mut Vec<(String, String, usize)>, name: &str) -> Option<Value> {
    let index = injections
        .iter()
        .position(|(method, _, remaining)| method == name && *remaining > 0)?;
    let (method, err_type, remaining) = injections.remove(index);
    let remaining = remaining - 1;
    if remaining > 0 {
        injections.push((method, err_type.clone(), remaining));
    }
    Some(method_error(&err_type, Vec::new()))
}

/// Detects a JMAP result reference: an object with exactly the three string
/// members `resultOf`, `name` and `path` (RFC 8620 section 3.6).
fn as_result_reference(map: &Map<String, Value>) -> Option<(&str, &str, &str)> {
    if map.len() != 3 {
        return None;
    }
    let result_of = map.get("resultOf").and_then(Value::as_str)?;
    let name = map.get("name").and_then(Value::as_str)?;
    let path = map.get("path").and_then(Value::as_str)?;
    Some((result_of, name, path))
}

/// Recursively rewrites references out of an argument tree.
///
/// Handles, in order:
/// 1. a value that is a result reference object → the referenced value;
/// 2. a `#name` key holding a result reference → `name: <resolved>`;
/// 3. a `#creationId` key → the key becomes the real id of that created object;
/// 4. a `"#creationId"` string value → the real id.
fn resolve_value(
    value: &Value,
    produced: &AHashMap<String, (String, Value)>,
    created_ids: &AHashMap<String, String>,
) -> Result<Value, Value> {
    match value {
        Value::Object(map) => {
            if let Some((result_of, name, path)) = as_result_reference(map) {
                return resolve_reference(result_of, name, path, produced);
            }
            let mut out = Map::with_capacity(map.len());
            for (key, inner) in map {
                if let Some(reference) = key.strip_prefix('#') {
                    if as_result_reference(inner.as_object().unwrap_or(&Map::new())).is_some() {
                        let resolved = resolve_value(inner, produced, created_ids)?;
                        out.insert(reference.to_string(), resolved);
                    } else {
                        let real_id = created_ids.get(reference).ok_or_else(|| {
                            method_error(
                                INVALID_ARGUMENTS,
                                vec![(
                                    "detail",
                                    json!(format!("unknown creation id '#{reference}'")),
                                )],
                            )
                        })?;
                        let resolved = resolve_value(inner, produced, created_ids)?;
                        out.insert(real_id.clone(), resolved);
                    }
                } else {
                    out.insert(key.clone(), resolve_value(inner, produced, created_ids)?);
                }
            }
            Ok(Value::Object(out))
        }
        Value::Array(items) => Ok(Value::Array(
            items
                .iter()
                .map(|item| resolve_value(item, produced, created_ids))
                .collect::<Result<Vec<Value>, Value>>()?,
        )),
        // A `"#creationId"` *value* is rewritten only when the creation id is
        // known; otherwise the string is left alone, so unrelated values that
        // happen to start with a hash (`"#1"` in a subject) keep working.
        Value::String(text) => match text.strip_prefix('#').and_then(|id| created_ids.get(id)) {
            Some(real_id) => Ok(Value::String(real_id.clone())),
            None => Ok(value.clone()),
        },
        _ => Ok(value.clone()),
    }
}

fn resolve_reference(
    result_of: &str,
    name: &str,
    path: &str,
    produced: &AHashMap<String, (String, Value)>,
) -> Result<Value, Value> {
    let (produced_name, produced_value) = produced.get(result_of).ok_or_else(|| {
        method_error(
            INVALID_RESULT_REFERENCE,
            vec![(
                "detail",
                json!(format!(
                    "no response with callId '{result_of}' in this request"
                )),
            )],
        )
    })?;
    if produced_name != name {
        return Err(method_error(
            INVALID_RESULT_REFERENCE,
            vec![(
                "detail",
                json!(format!(
                    "reference names method '{name}' but callId '{result_of}' returned '{produced_name}'"
                )),
            )],
        ));
    }
    json_pointer(produced_value, path).ok_or_else(|| {
        method_error(
            INVALID_RESULT_REFERENCE,
            vec![(
                "detail",
                json!(format!(
                    "path '{path}' not found in response of '{result_of}'"
                )),
            )],
        )
    })
}

/// A minimal JSON pointer (RFC 6901) with the `*` wildcard JMAP allows.
///
/// Supported forms include `/newState`, `/oldState`, `/queryState`, `/total`,
/// `/ids`, `/list`, `/list/*/id` and `/list/*/<field>`.
pub fn json_pointer(root: &Value, path: &str) -> Option<Value> {
    let path = path.strip_prefix('/')?;
    let segments: Vec<&str> = path.split('/').collect();
    step(root, &segments)
}

fn step(value: &Value, segments: &[&str]) -> Option<Value> {
    if segments.is_empty() {
        return Some(value.clone());
    }
    let segment = segments[0];
    if segment == "*" {
        let items = value.as_array()?;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(step(item, &segments[1..])?);
        }
        Some(Value::Array(out))
    } else {
        let next = match value {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
        step(next, &segments[1..])
    }
}
