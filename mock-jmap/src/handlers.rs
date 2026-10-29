//! Method handlers for every entry of `jmap_client::Method`.
//!
//! Handlers are plain functions from a resolved argument object to a
//! [`CallOutcome`]. They share a small set of generic helpers
//! ([`get_response`], [`changes_response`], [`query_response`],
//! [`query_changes_response`], [`set_response`]) that implement the RFC shapes
//! once, so every type answers consistently.
//!
//! Data coherence rules the handlers maintain:
//! * a Set that creates an `Email` also creates its `Thread` and links
//!   `emailIds`,
//! * `Email/import` maps an uploaded blob to a new email (and thread),
//! * `Email/copy` clones an existing email into a new one,
//! * every mutation advances the collection state and the session state.

use serde_json::{json, Map, Value};

use crate::dispatch::{CallOutcome, Ctx};
use crate::problem::method_error_type::{
    ACCOUNT_NOT_FOUND, CANNOT_CALCULATE_CHANGES, INVALID_ARGUMENTS, UNKNOWN_METHOD,
};
use crate::session::ACCOUNT_ID;
use crate::store::Collection;

/// Runs the method named `name`.
pub fn dispatch(name: &str, ctx: Ctx) -> CallOutcome {
    match name {
        "Core/echo" => core_echo(ctx),

        "Blob/copy" => blob_copy(ctx),

        "PushSubscription/get" => simple_get(ctx, "PushSubscription"),
        "PushSubscription/set" => simple_set(ctx, "PushSubscription"),

        "Mailbox/get" => simple_get(ctx, "Mailbox"),
        "Mailbox/changes" => mailbox_changes(ctx),
        "Mailbox/query" => simple_query(ctx, "Mailbox"),
        "Mailbox/queryChanges" => simple_query_changes(ctx, "Mailbox"),
        "Mailbox/set" => simple_set(ctx, "Mailbox"),

        "Thread/get" => simple_get(ctx, "Thread"),
        "Thread/changes" => simple_changes(ctx, "Thread"),

        "Email/get" => simple_get(ctx, "Email"),
        "Email/changes" => simple_changes(ctx, "Email"),
        "Email/query" => simple_query(ctx, "Email"),
        "Email/queryChanges" => simple_query_changes(ctx, "Email"),
        "Email/set" => simple_set(ctx, "Email"),
        "Email/copy" => email_copy(ctx),
        "Email/import" => email_import(ctx),
        "Email/parse" => email_parse(ctx),

        "SearchSnippet/get" => search_snippet_get(ctx),

        "Identity/get" => simple_get(ctx, "Identity"),
        "Identity/changes" => simple_changes(ctx, "Identity"),
        "Identity/set" => simple_set(ctx, "Identity"),

        "EmailSubmission/get" => simple_get(ctx, "EmailSubmission"),
        "EmailSubmission/changes" => simple_changes(ctx, "EmailSubmission"),
        "EmailSubmission/query" => simple_query(ctx, "EmailSubmission"),
        "EmailSubmission/queryChanges" => simple_query_changes(ctx, "EmailSubmission"),
        "EmailSubmission/set" => simple_set(ctx, "EmailSubmission"),

        "VacationResponse/get" => simple_get(ctx, "VacationResponse"),
        "VacationResponse/set" => simple_set(ctx, "VacationResponse"),

        "SieveScript/get" => simple_get(ctx, "SieveScript"),
        "SieveScript/query" => simple_query(ctx, "SieveScript"),
        "SieveScript/set" => simple_set(ctx, "SieveScript"),
        "SieveScript/validate" => sieve_validate(ctx),

        "Principal/get" => simple_get(ctx, "Principal"),
        "Principal/changes" => simple_changes(ctx, "Principal"),
        "Principal/query" => simple_query(ctx, "Principal"),
        "Principal/queryChanges" => simple_query_changes(ctx, "Principal"),
        "Principal/set" => simple_set(ctx, "Principal"),

        _ => CallOutcome::error(UNKNOWN_METHOD),
    }
}

// --------------------------------------------------------------------------
// shared helpers
// --------------------------------------------------------------------------

/// The `accountId` argument, defaulting to the mock's single account.
pub fn account_id_of(args: &Value) -> String {
    args.get("accountId")
        .and_then(Value::as_str)
        .unwrap_or(ACCOUNT_ID)
        .to_string()
}

/// `true` when the mock serves this account.
pub fn is_known_account(account_id: &str) -> bool {
    account_id == ACCOUNT_ID
}

fn string_field(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `X/get` response: `list` of the requested (or all) records plus `notFound`.
pub fn get_response(coll: &Collection, args: &Value, account_id: &str) -> Value {
    let mut list = Vec::new();
    let mut not_found = Vec::new();
    match args.get("ids") {
        Some(Value::Array(ids)) => {
            for id in ids.iter().filter_map(Value::as_str) {
                match coll.get(id) {
                    Some(record) => list.push(record.clone()),
                    None => not_found.push(id.to_string()),
                }
            }
        }
        _ => list = coll.all(),
    }
    json!({
        "accountId": account_id,
        "state": coll.state_str(),
        "list": list,
        "notFound": not_found,
    })
}

/// `X/changes` response, with optional extra members (e.g. `updatedProperties`).
pub fn changes_response(
    coll: &Collection,
    args: &Value,
    account_id: &str,
    extra: Vec<(&str, Value)>,
) -> CallOutcome {
    let Some(since) = string_field(args, "sinceState") else {
        return CallOutcome::error(INVALID_ARGUMENTS);
    };
    let (created, updated, destroyed) = match coll.changes_since(&since) {
        Ok(changes) => changes,
        Err(()) => return CallOutcome::error(CANNOT_CALCULATE_CHANGES),
    };
    let mut out = json!({
        "accountId": account_id,
        "oldState": since,
        "newState": coll.state_str(),
        "hasMoreChanges": false,
        "created": created,
        "updated": updated,
        "destroyed": destroyed,
    });
    for (key, value) in extra {
        out.as_object_mut().unwrap().insert(key.to_string(), value);
    }
    CallOutcome::Ok(out)
}

/// `X/query` response honouring `position`, `limit` and `calculateTotal`.
pub fn query_response(
    coll: &Collection,
    args: &Value,
    account_id: &str,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    let all: Vec<String> = coll
        .filter(&matches)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let total = all.len();
    let position = args
        .get("position")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize);
    let ids: Vec<String> = all
        .into_iter()
        .skip(position)
        .take(limit.unwrap_or(usize::MAX))
        .collect();

    let mut out = json!({
        "accountId": account_id,
        "queryState": coll.state_str(),
        "canCalculateChanges": true,
        "position": position as i64,
        "ids": ids,
        "total": total,
    });
    if let Some(limit) = limit {
        out.as_object_mut()
            .unwrap()
            .insert("limit".into(), json!(limit));
    }
    out
}

/// `X/queryChanges` response: the current result set added since
/// `sinceQueryState`, plus the ids removed since then.
pub fn query_changes_response(
    coll: &Collection,
    args: &Value,
    account_id: &str,
    matches: impl Fn(&Value) -> bool,
) -> CallOutcome {
    let Some(since) = string_field(args, "sinceQueryState") else {
        return CallOutcome::error(INVALID_ARGUMENTS);
    };
    let Ok(since_num) = since.parse::<u64>() else {
        return CallOutcome::error(CANNOT_CALCULATE_CHANGES);
    };
    let current_state = coll.state_str().parse::<u64>().unwrap_or(0);
    if since_num > current_state {
        return CallOutcome::error(CANNOT_CALCULATE_CHANGES);
    }

    let current: Vec<String> = coll
        .filter(&matches)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let (removed, added) = if since_num == current_state {
        (Vec::new(), Vec::new())
    } else {
        let removed = coll.destroyed_since(since_num);
        let added = current
            .iter()
            .enumerate()
            .map(|(index, id)| json!({ "id": id, "index": index }))
            .collect();
        (removed, added)
    };

    CallOutcome::Ok(json!({
        "accountId": account_id,
        "oldQueryState": since,
        "newQueryState": coll.state_str(),
        "total": current.len(),
        "removed": removed,
        "added": added,
    }))
}

/// `X/set` response: create / update / destroy with `notCreated`,
/// `notUpdated` and `notDestroyed` for unknown ids.
pub fn set_response(coll: &mut Collection, args: &Value, account_id: &str) -> CallOutcome {
    let old_state = coll.state_str();
    if let Some(if_in_state) = string_field(args, "ifInState") {
        if if_in_state != old_state {
            return CallOutcome::error("stateMismatch");
        }
    }

    let mut created = Map::new();
    let mut updated = Map::new();
    let mut destroyed = Vec::new();
    let mut not_created = Map::new();
    let mut not_updated = Map::new();
    let mut not_destroyed = Map::new();
    let mut created_ids = Vec::new();
    let mut updated_ids = Vec::new();
    let mut destroyed_ids = Vec::new();

    if let Some(create) = args.get("create").and_then(Value::as_object) {
        for (creation_id, object) in create {
            if object.get("id").and_then(Value::as_str).is_some() {
                not_created.insert(
                    creation_id.clone(),
                    json!({ "type": "invalidProperties", "properties": ["id"] }),
                );
                continue;
            }
            let id = coll.create(object.clone());
            created.insert(
                creation_id.clone(),
                coll.get(&id).cloned().unwrap_or(Value::Null),
            );
            created_ids.push(id);
        }
    }

    if let Some(updates) = args.get("update").and_then(Value::as_object) {
        for (id, patch) in updates {
            match coll.update(id, patch) {
                Some(record) => {
                    updated.insert(id.clone(), json!(record));
                    updated_ids.push(id.clone());
                }
                None => {
                    not_updated.insert(id.clone(), json!({ "type": "notFound" }));
                }
            }
        }
    }

    if let Some(destroy) = args.get("destroy").and_then(Value::as_array) {
        for id in destroy.iter().filter_map(Value::as_str) {
            if coll.destroy(id) {
                destroyed.push(id.to_string());
                destroyed_ids.push(id.to_string());
            } else {
                not_destroyed.insert(id.to_string(), json!({ "type": "notFound" }));
            }
        }
    }

    coll.record_change(created_ids, updated_ids, destroyed_ids);

    let mut out = json!({
        "accountId": account_id,
        "oldState": old_state,
        "newState": coll.state_str(),
    });
    let out = out.as_object_mut().unwrap();
    if !created.is_empty() {
        out.insert("created".to_string(), Value::Object(created));
    }
    if !updated.is_empty() {
        out.insert("updated".to_string(), Value::Object(updated));
    }
    if !destroyed.is_empty() {
        out.insert("destroyed".to_string(), json!(destroyed));
    }
    if !not_created.is_empty() {
        out.insert("notCreated".to_string(), Value::Object(not_created));
    }
    if !not_updated.is_empty() {
        out.insert("notUpdated".to_string(), Value::Object(not_updated));
    }
    if !not_destroyed.is_empty() {
        out.insert("notDestroyed".to_string(), Value::Object(not_destroyed));
    }
    CallOutcome::Ok(Value::Object(out.clone()))
}

// --------------------------------------------------------------------------
// generic per-type handlers
// --------------------------------------------------------------------------

fn simple_get(ctx: Ctx, type_name: &str) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let Some(coll) = ctx.store.collection(type_name) else {
        return CallOutcome::error(UNKNOWN_METHOD);
    };
    CallOutcome::Ok(get_response(coll, &ctx.args, &account_id))
}

fn simple_changes(ctx: Ctx, type_name: &str) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let Some(coll) = ctx.store.collection(type_name) else {
        return CallOutcome::error(UNKNOWN_METHOD);
    };
    changes_response(coll, &ctx.args, &account_id, Vec::new())
}

fn mailbox_changes(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let Some(coll) = ctx.store.collection("Mailbox") else {
        return CallOutcome::error(UNKNOWN_METHOD);
    };
    changes_response(
        coll,
        &ctx.args,
        &account_id,
        vec![(
            "updatedProperties",
            json!(["name", "parentId", "role", "sortOrder", "isSubscribed"]),
        )],
    )
}

fn simple_query(ctx: Ctx, type_name: &str) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let Some(coll) = ctx.store.collection(type_name) else {
        return CallOutcome::error(UNKNOWN_METHOD);
    };
    let filter = ctx.args.get("filter").cloned().unwrap_or(Value::Null);
    CallOutcome::Ok(query_response(coll, &ctx.args, &account_id, |record| {
        matches_filter(record, &filter)
    }))
}

fn simple_query_changes(ctx: Ctx, type_name: &str) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let Some(coll) = ctx.store.collection(type_name) else {
        return CallOutcome::error(UNKNOWN_METHOD);
    };
    let filter = ctx.args.get("filter").cloned().unwrap_or(Value::Null);
    query_changes_response(coll, &ctx.args, &account_id, |record| {
        matches_filter(record, &filter)
    })
}

fn simple_set(ctx: Ctx, type_name: &str) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    if !is_known_account(&account_id) {
        return CallOutcome::error(ACCOUNT_NOT_FOUND);
    }
    let outcome = match ctx.store.collection_mut(type_name) {
        Some(coll) => set_response(coll, &ctx.args, &account_id),
        None => return CallOutcome::error(UNKNOWN_METHOD),
    };
    if matches!(outcome, CallOutcome::Ok(_)) {
        ctx.store.touch();
    }
    outcome
}

// --------------------------------------------------------------------------
// Core / Blob
// --------------------------------------------------------------------------

fn core_echo(ctx: Ctx) -> CallOutcome {
    CallOutcome::Ok(ctx.args)
}

fn blob_copy(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let from_account_id =
        string_field(&ctx.args, "fromAccountId").unwrap_or_else(|| account_id.clone());
    let mut copied = Map::new();
    let mut not_copied = Map::new();
    for blob_id in string_array(&ctx.args, "blobIds") {
        match ctx.store.blobs.copy(&blob_id) {
            Some(new_id) => {
                copied.insert(blob_id, json!(new_id));
            }
            None => {
                not_copied.insert(blob_id, json!({ "type": "blobNotFound" }));
            }
        }
    }
    ctx.store.touch();
    let mut out = json!({
        "fromAccountId": from_account_id,
        "accountId": account_id,
    });
    let out = out.as_object_mut().unwrap();
    if !copied.is_empty() {
        out.insert("copied".to_string(), Value::Object(copied));
    }
    if !not_copied.is_empty() {
        out.insert("notCopied".to_string(), Value::Object(not_copied));
    }
    CallOutcome::Ok(Value::Object(out.clone()))
}

// --------------------------------------------------------------------------
// Email copy / import / parse, SearchSnippet, Sieve validate
// --------------------------------------------------------------------------

fn email_copy(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let from_account_id =
        string_field(&ctx.args, "fromAccountId").unwrap_or_else(|| account_id.clone());
    let old_state = ctx.store.emails.state_str();

    let mut created = Map::new();
    let mut not_created = Map::new();
    let mut new_email_ids = Vec::new();
    let mut new_thread_ids = Vec::new();

    if let Some(create) = ctx.args.get("create").and_then(Value::as_object) {
        for (creation_id, spec) in create {
            let source_id = spec.get("id").and_then(Value::as_str).unwrap_or_default();
            let Some(source) = ctx.store.emails.get(source_id).cloned() else {
                not_created.insert(creation_id.clone(), json!({ "type": "notFound" }));
                continue;
            };
            let mut copy = source;
            if let Some(mailbox_ids) = spec.get("mailboxIds") {
                copy["mailboxIds"] = mailbox_ids.clone();
            }
            if let Some(keywords) = spec.get("keywords") {
                copy["keywords"] = keywords.clone();
            }
            let thread_id = ctx.store.threads.create(json!({ "emailIds": [] }));
            let email_id = ctx.store.emails.create(copy);
            link_thread(&mut ctx.store.threads, &thread_id, std::slice::from_ref(&email_id));
            created.insert(
                creation_id.clone(),
                ctx.store
                    .emails
                    .get(&email_id)
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            new_email_ids.push(email_id);
            new_thread_ids.push(thread_id);
        }
    }

    ctx.store
        .threads
        .record_change(new_thread_ids, Vec::new(), Vec::new());
    ctx.store
        .emails
        .record_change(new_email_ids, Vec::new(), Vec::new());
    ctx.store.touch();

    let mut out = json!({
        "fromAccountId": from_account_id,
        "accountId": account_id,
        "oldState": old_state,
        "newState": ctx.store.emails.state_str(),
    });
    let out = out.as_object_mut().unwrap();
    if !created.is_empty() {
        out.insert("created".to_string(), Value::Object(created));
    }
    if !not_created.is_empty() {
        out.insert("notCreated".to_string(), Value::Object(not_created));
    }
    CallOutcome::Ok(Value::Object(out.clone()))
}

fn email_import(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let old_state = ctx.store.emails.state_str();
    let mut created = Map::new();
    let mut not_created = Map::new();
    let mut new_email_ids = Vec::new();
    let mut new_thread_ids = Vec::new();

    if let Some(emails) = ctx.args.get("emails").and_then(Value::as_object) {
        for (creation_id, spec) in emails {
            let blob_id = spec
                .get("blobId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(blob) = ctx.store.blobs.get(blob_id).cloned() else {
                not_created.insert(creation_id.clone(), json!({ "type": "blobNotFound" }));
                continue;
            };
            let subject = rfc822_header(&blob.bytes, "Subject")
                .unwrap_or_else(|| "Imported message".to_string());
            let message_id = rfc822_header(&blob.bytes, "Message-ID")
                .unwrap_or_else(|| format!("{creation_id}@mock.invalid"));
            let from = rfc822_header(&blob.bytes, "From")
                .unwrap_or_else(|| "unknown@mock.invalid".to_string());
            let to = rfc822_header(&blob.bytes, "To").unwrap_or_else(|| ACCOUNT_ID.to_string());

            let thread_id = ctx.store.threads.create(json!({ "emailIds": [] }));
            let record = json!({
                "blobId": blob_id,
                "threadId": thread_id,
                "mailboxIds": spec.get("mailboxIds").cloned().unwrap_or_else(|| json!({})),
                "keywords": spec.get("keywords").cloned().unwrap_or_else(|| json!({})),
                "size": blob.bytes.len(),
                "receivedAt": spec
                    .get("receivedAt")
                    .cloned()
                    .unwrap_or(json!("2026-01-01T00:00:00Z")),
                "messageId": [message_id],
                "subject": subject,
                "from": [{ "name": from, "email": from }],
                "to": [{ "name": to, "email": to }],
                "preview": format!("{subject} (imported)"),
            });
            let email_id = ctx.store.emails.create(record);
            link_thread(&mut ctx.store.threads, &thread_id, std::slice::from_ref(&email_id));
            created.insert(
                creation_id.clone(),
                ctx.store
                    .emails
                    .get(&email_id)
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            new_email_ids.push(email_id);
            new_thread_ids.push(thread_id);
        }
    }

    ctx.store
        .threads
        .record_change(new_thread_ids, Vec::new(), Vec::new());
    ctx.store
        .emails
        .record_change(new_email_ids, Vec::new(), Vec::new());
    ctx.store.touch();

    let mut out = json!({
        "accountId": account_id,
        "oldState": old_state,
        "newState": ctx.store.emails.state_str(),
    });
    let out = out.as_object_mut().unwrap();
    if !created.is_empty() {
        out.insert("created".to_string(), Value::Object(created));
    }
    if !not_created.is_empty() {
        out.insert("notCreated".to_string(), Value::Object(not_created));
    }
    CallOutcome::Ok(Value::Object(out.clone()))
}

fn email_parse(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let mut parsed = Map::new();
    let mut not_parsable = Vec::new();
    let mut not_found = Vec::new();

    for blob_id in string_array(&ctx.args, "blobIds") {
        let Some(blob) = ctx.store.blobs.get(&blob_id).cloned() else {
            not_found.push(blob_id);
            continue;
        };
        if !blob.content_type.contains("rfc822") {
            not_parsable.push(blob_id);
            continue;
        }
        let subject = rfc822_header(&blob.bytes, "Subject").unwrap_or_default();
        let from = rfc822_header(&blob.bytes, "From").unwrap_or_default();
        let to = rfc822_header(&blob.bytes, "To").unwrap_or_default();
        let message_id = rfc822_header(&blob.bytes, "Message-ID").unwrap_or_default();
        parsed.insert(
            blob_id.clone(),
            json!({
                "blobId": blob_id,
                "messageId": [message_id],
                "subject": subject,
                "from": [{ "name": from, "email": from }],
                "to": [{ "name": to, "email": to }],
                "sender": [],
                "cc": [],
                "bcc": [],
                "replyTo": [],
                "receivedAt": "2026-01-01T00:00:00Z",
                "size": blob.bytes.len(),
                "preview": subject,
                "textBody": ["text-1"],
                "bodyValues": {
                    "text-1": {
                        "value": rfc822_body(&blob.bytes),
                        "isEncodingProblem": false,
                        "isTruncated": false
                    }
                },
                "attachments": []
            }),
        );
    }

    let mut out = json!({ "accountId": account_id });
    let out = out.as_object_mut().unwrap();
    if !parsed.is_empty() {
        out.insert("parsed".to_string(), Value::Object(parsed));
    }
    if !not_parsable.is_empty() {
        out.insert("notParsable".to_string(), json!(not_parsable));
    }
    if !not_found.is_empty() {
        out.insert("notFound".to_string(), json!(not_found));
    }
    CallOutcome::Ok(Value::Object(out.clone()))
}

fn search_snippet_get(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let filter = ctx.args.get("filter").cloned().unwrap_or(Value::Null);
    let keyword = find_keyword(&filter).unwrap_or_default();
    let requested = string_array(&ctx.args, "emailIds");

    let candidates: Vec<(String, Value)> = if requested.is_empty() {
        ctx.store
            .emails
            .filter(|record| matches_filter(record, &filter))
    } else {
        requested
            .iter()
            .filter_map(|id| {
                ctx.store
                    .emails
                    .get(id)
                    .cloned()
                    .map(|record| (id.clone(), record))
            })
            .collect()
    };

    let mut list = Vec::new();
    let mut not_found = Vec::new();
    for (id, record) in candidates {
        let subject = record
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let preview = record
            .get("preview")
            .and_then(Value::as_str)
            .unwrap_or(subject);
        if !keyword.is_empty()
            && !subject.to_lowercase().contains(&keyword)
            && !preview.to_lowercase().contains(&keyword)
        {
            if !requested.is_empty() {
                not_found.push(id);
            }
            continue;
        }
        list.push(json!({
            "emailId": id,
            "subject": highlight(subject, &keyword),
            "preview": highlight(preview, &keyword),
        }));
    }

    let mut out = json!({ "accountId": account_id, "list": list });
    if !not_found.is_empty() {
        out.as_object_mut()
            .unwrap()
            .insert("notFound".to_string(), json!(not_found));
    }
    CallOutcome::Ok(out)
}

fn sieve_validate(ctx: Ctx) -> CallOutcome {
    let account_id = account_id_of(&ctx.args);
    let Some(blob_id) = string_field(&ctx.args, "blobId") else {
        return CallOutcome::error(INVALID_ARGUMENTS);
    };
    let Some(blob) = ctx.store.blobs.get(&blob_id).cloned() else {
        return CallOutcome::Ok(json!({
            "accountId": account_id,
            "error": { "type": "blobNotFound", "detail": format!("blob '{blob_id}' not found") }
        }));
    };
    let text = String::from_utf8_lossy(&blob.bytes).to_string();
    if text.contains("INVALID") {
        return CallOutcome::Ok(json!({
            "accountId": account_id,
            "error": {
                "type": "invalidScript",
                "detail": "line 1: unexpected token 'INVALID'"
            }
        }));
    }
    CallOutcome::Ok(json!({ "accountId": account_id }))
}

// --------------------------------------------------------------------------
// tiny utilities
// --------------------------------------------------------------------------

fn link_thread(threads: &mut Collection, thread_id: &str, email_ids: &[String]) {
    if let Some(mut thread) = threads.get(thread_id).cloned() {
        thread["emailIds"] = Value::Array(email_ids.iter().map(|id| json!(id)).collect());
        threads.replace(thread_id, thread);
    }
}

/// Extracts an RFC 822 header value from raw message bytes.
pub fn rfc822_header(bytes: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let head = text.split("\r\n\r\n").next().unwrap_or(&text);
    let prefix = format!("{name}:");
    for line in head.lines() {
        if let Some(rest) = line.strip_prefix(prefix.as_str()) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Extracts the body of an RFC 822 message.
pub fn rfc822_body(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    match text.split_once("\r\n\r\n") {
        Some((_, body)) => body.to_string(),
        None => match text.split_once("\n\n") {
            Some((_, body)) => body.to_string(),
            None => String::new(),
        },
    }
}

/// First string value found under a `text`/`subject`/`from`/`to` filter key.
fn find_keyword(filter: &Value) -> Option<String> {
    match filter {
        Value::Object(map) => {
            for key in ["text", "subject", "from", "to"] {
                if let Some(value) = map.get(key).and_then(Value::as_str) {
                    return Some(value.to_lowercase());
                }
            }
            for value in map.values() {
                if let Some(keyword) = find_keyword(value) {
                    return Some(keyword);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_keyword),
        _ => None,
    }
}

fn highlight(text: &str, keyword: &str) -> String {
    if keyword.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    match lower.find(keyword) {
        Some(at) if text.is_char_boundary(at) && text.is_char_boundary(at + keyword.len()) => {
            format!(
                "{}<mark>{}</mark>",
                &text[..at],
                &text[at..at + keyword.len()]
            )
        }
        _ => text.to_string(),
    }
}

/// A deliberately permissive JMAP filter evaluator.
///
/// Supports the `AND` / `OR` / `NOT` operators and the common condition names
/// across mailbox/email/identity/submission/sieve/principal queries. Unknown
/// conditions match everything, so a test that sends a filter the mock does not
/// model still gets a coherent (rather than empty) answer.
pub fn matches_filter(record: &Value, filter: &Value) -> bool {
    let Value::Object(map) = filter else {
        return true;
    };
    if let Some(operator) = map.get("operator").and_then(Value::as_str) {
        let conditions: Vec<Value> = map
            .get("conditions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return match operator {
            "AND" => conditions
                .iter()
                .all(|condition| matches_filter(record, condition)),
            "OR" => conditions
                .iter()
                .any(|condition| matches_filter(record, condition)),
            "NOT" => !conditions
                .iter()
                .any(|condition| matches_filter(record, condition)),
            _ => true,
        };
    }
    map.iter()
        .all(|(key, expected)| matches_condition(record, key, expected))
}

fn matches_condition(record: &Value, key: &str, expected: &Value) -> bool {
    let text_of = |field: &str| -> String {
        record
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    let addresses = |field: &str| -> String {
        record
            .get(field)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("email").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
            .to_lowercase()
    };
    let expected_text = expected.as_str().unwrap_or_default().to_lowercase();

    match key {
        "inMailbox" => record
            .get("mailboxIds")
            .and_then(Value::as_object)
            .map(|ids| {
                expected
                    .as_str()
                    .map(|id| ids.get(id).and_then(Value::as_bool).unwrap_or(false))
                    .unwrap_or(false)
            })
            .unwrap_or(false),
        "inMailboxOtherThan" => {
            let excluded: Vec<String> = expected
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            record
                .get("mailboxIds")
                .and_then(Value::as_object)
                .map(|ids| ids.keys().any(|id| !excluded.iter().any(|skip| skip == id)))
                .unwrap_or(false)
        }
        "hasKeyword" | "keyword" => record
            .get("keywords")
            .and_then(Value::as_object)
            .map(|keywords| {
                expected
                    .as_str()
                    .map(|kw| keywords.get(kw).and_then(Value::as_bool).unwrap_or(false))
                    .unwrap_or(false)
            })
            .unwrap_or(false),
        "hasNoKeyword" => record
            .get("keywords")
            .and_then(Value::as_object)
            .map(|keywords| {
                expected
                    .as_str()
                    .map(|kw| !keywords.contains_key(kw))
                    .unwrap_or_else(|| keywords.is_empty())
            })
            .unwrap_or(true),
        "text" => {
            text_of("subject").contains(&expected_text)
                || text_of("preview").contains(&expected_text)
                || addresses("from").contains(&expected_text)
                || addresses("to").contains(&expected_text)
        }
        "subject" => text_of("subject").contains(&expected_text),
        "from" => addresses("from").contains(&expected_text),
        "to" => addresses("to").contains(&expected_text),
        "name" => text_of("name").contains(&expected_text),
        "email" => text_of("email") == expected_text,
        "type" => text_of("type") == expected_text,
        "isActive" => {
            record
                .get("isActive")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                == expected.as_bool().unwrap_or(false)
        }
        "hasAttachment" => {
            record
                .get("attachments")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false)
                == expected.as_bool().unwrap_or(false)
        }
        "after" | "before" | "afterReceivedAt" | "beforeReceivedAt" | "afterCreated"
        | "beforeCreated" | "afterSent" | "beforeSent" | "minSize" | "maxSize" => true,
        "allInThreadWith" | "anyInThreadWith" | "threadId" => record
            .get("threadId")
            .and_then(Value::as_str)
            .map(|id| Some(id) == expected.as_str())
            .unwrap_or(false),
        other => match record.get(other) {
            Some(actual) => loose_eq(actual, expected),
            None => true,
        },
    }
}

fn loose_eq(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        _ => false,
    }
}
