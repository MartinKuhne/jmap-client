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

use crate::Error;
use ahash::AHashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

use super::{request::ResultReference, Object, RequestParams};

pub trait SetObject: Object {
    type SetArguments: Default;

    fn new(create_id: Option<usize>) -> Self;
    fn create_id(&self) -> Option<String>;
    fn set_id(&mut self, _id: Option<String>) {}
}

#[derive(Debug, Clone, Serialize)]
pub struct SetRequest<O: SetObject> {
    #[serde(rename = "accountId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,

    #[serde(rename = "ifInState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    if_in_state: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    create: Option<AHashMap<String, O>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<AHashMap<String, O>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    destroy: Option<Vec<String>>,

    #[serde(rename = "#destroy")]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    destroy_ref: Option<ResultReference>,

    #[serde(flatten)]
    arguments: O::SetArguments,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetResponse<O: SetObject> {
    #[serde(rename = "accountId")]
    account_id: Option<String>,

    #[serde(rename = "oldState")]
    old_state: Option<String>,

    #[serde(rename = "newState")]
    new_state: Option<String>,

    #[serde(rename = "created")]
    created: Option<AHashMap<String, O>>,

    #[serde(rename = "updated")]
    updated: Option<AHashMap<String, Option<O>>>,

    #[serde(rename = "destroyed")]
    destroyed: Option<Vec<String>>,

    #[serde(rename = "notCreated")]
    not_created: Option<AHashMap<String, SetError<O::Property>>>,

    #[serde(rename = "notUpdated")]
    not_updated: Option<AHashMap<String, SetError<O::Property>>>,

    #[serde(rename = "notDestroyed")]
    not_destroyed: Option<AHashMap<String, SetError<O::Property>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetError<U>
where
    U: Display,
{
    #[serde(rename = "type")]
    pub type_: SetErrorType,
    description: Option<String>,
    properties: Option<Vec<U>>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
pub enum SetErrorType {
    #[serde(rename = "forbidden")]
    Forbidden,
    #[serde(rename = "overQuota")]
    OverQuota,
    #[serde(rename = "tooLarge")]
    TooLarge,
    #[serde(rename = "rateLimit")]
    RateLimit,
    #[serde(rename = "notFound")]
    NotFound,
    #[serde(rename = "invalidPatch")]
    InvalidPatch,
    #[serde(rename = "willDestroy")]
    WillDestroy,
    #[serde(rename = "invalidProperties")]
    InvalidProperties,
    #[serde(rename = "singleton")]
    Singleton,
    #[serde(rename = "mailboxHasChild")]
    MailboxHasChild,
    #[serde(rename = "mailboxHasEmail")]
    MailboxHasEmail,
    #[serde(rename = "blobNotFound")]
    BlobNotFound,
    #[serde(rename = "tooManyKeywords")]
    TooManyKeywords,
    #[serde(rename = "tooManyMailboxes")]
    TooManyMailboxes,
    #[serde(rename = "forbiddenFrom")]
    ForbiddenFrom,
    #[serde(rename = "invalidEmail")]
    InvalidEmail,
    #[serde(rename = "tooManyRecipients")]
    TooManyRecipients,
    #[serde(rename = "noRecipients")]
    NoRecipients,
    #[serde(rename = "invalidRecipients")]
    InvalidRecipients,
    #[serde(rename = "forbiddenMailFrom")]
    ForbiddenMailFrom,
    #[serde(rename = "forbiddenToSend")]
    ForbiddenToSend,
    #[serde(rename = "cannotUnsend")]
    CannotUnsend,
    #[serde(rename = "alreadyExists")]
    AlreadyExists,
    #[serde(rename = "invalidScript")]
    InvalidScript,
    #[serde(rename = "scriptIsActive")]
    ScriptIsActive,
}

impl<O: SetObject> SetRequest<O> {
    pub fn new(params: RequestParams) -> Self {
        Self {
            account_id: if O::requires_account_id() {
                params.account_id.into()
            } else {
                None
            },
            if_in_state: None,
            create: None,
            update: None,
            destroy: None,
            destroy_ref: None,
            arguments: Default::default(),
        }
    }

    pub fn account_id(&mut self, account_id: impl Into<String>) -> &mut Self {
        if O::requires_account_id() {
            self.account_id = Some(account_id.into());
        }
        self
    }

    pub fn if_in_state(&mut self, if_in_state: impl Into<String>) -> &mut Self {
        self.if_in_state = Some(if_in_state.into());
        self
    }

    pub fn create(&mut self) -> &mut O {
        let create_id = self.create.as_ref().map_or(0, |c| c.len());
        let create_id_str = format!("c{}", create_id);
        self.create
            .get_or_insert_with(AHashMap::new)
            .insert(create_id_str.clone(), O::new(create_id.into()));
        self.create
            .as_mut()
            .unwrap()
            .get_mut(&create_id_str)
            .unwrap()
    }

    pub fn create_with_id(&mut self, create_id: impl Into<String>) -> &mut O {
        let create_id = create_id.into();
        self.create
            .get_or_insert_with(AHashMap::new)
            .insert(create_id.clone(), O::new(0.into()));
        self.create.as_mut().unwrap().get_mut(&create_id).unwrap()
    }

    pub fn create_item(&mut self, item: O) -> String {
        let create_id = self.create.as_ref().map_or(0, |c| c.len());
        let create_id_str = format!("c{}", create_id);
        self.create
            .get_or_insert_with(AHashMap::new)
            .insert(create_id_str.clone(), item);
        create_id_str
    }

    pub fn update(&mut self, id: impl Into<String>) -> &mut O {
        let id: String = id.into();
        self.update
            .get_or_insert_with(AHashMap::new)
            .insert(id.clone(), O::new(None));
        self.update.as_mut().unwrap().get_mut(&id).unwrap()
    }

    pub fn update_item(&mut self, id: impl Into<String>, item: O) {
        self.update
            .get_or_insert_with(AHashMap::new)
            .insert(id.into(), item);
    }

    pub fn destroy<U, V>(&mut self, ids: U) -> &mut Self
    where
        U: IntoIterator<Item = V>,
        V: Into<String>,
    {
        self.destroy
            .get_or_insert_with(Vec::new)
            .extend(ids.into_iter().map(|id| id.into()));
        self.destroy_ref = None;
        self
    }

    pub fn destroy_ref(&mut self, reference: ResultReference) -> &mut Self {
        self.destroy_ref = reference.into();
        self.destroy = None;
        self
    }

    pub fn arguments(&mut self) -> &mut O::SetArguments {
        &mut self.arguments
    }
}

impl<O: SetObject> SetResponse<O> {
    pub fn account_id(&self) -> Option<&str> {
        self.account_id.as_deref()
    }

    pub fn old_state(&self) -> Option<&str> {
        self.old_state.as_deref()
    }

    pub fn new_state(&self) -> &str {
        self.new_state.as_deref().unwrap_or("")
    }

    pub fn take_new_state(&mut self) -> String {
        self.new_state.take().unwrap_or_default()
    }

    pub fn created(&mut self, id: &str) -> crate::Result<O> {
        if let Some(result) = self.created.as_mut().and_then(|r| r.remove(id)) {
            Ok(result)
        } else if let Some(error) = self.not_created.as_mut().and_then(|r| r.remove(id)) {
            Err(error.to_string_error().into())
        } else {
            Err(Error::Internal(format!("Id {} not found.", id)))
        }
    }

    pub fn updated(&mut self, id: &str) -> crate::Result<Option<O>> {
        if let Some(result) = self.updated.as_mut().and_then(|r| r.remove(id)) {
            Ok(result)
        } else if let Some(error) = self.not_updated.as_mut().and_then(|r| r.remove(id)) {
            Err(error.to_string_error().into())
        } else {
            Err(Error::Internal(format!("Id {} not found.", id)))
        }
    }

    pub fn destroyed(&mut self, id: &str) -> crate::Result<()> {
        if self
            .destroyed
            .as_ref()
            .is_some_and(|r| r.iter().any(|i| i == id))
        {
            Ok(())
        } else if let Some(error) = self.not_destroyed.as_mut().and_then(|r| r.remove(id)) {
            Err(error.to_string_error().into())
        } else {
            Err(Error::Internal(format!("Id {} not found.", id)))
        }
    }

    pub fn created_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.created.as_ref().map(|map| map.keys())
    }

    pub fn updated_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.updated.as_ref().map(|map| map.keys())
    }

    pub fn take_updated_ids(&mut self) -> Option<Vec<String>> {
        self.updated
            .take()
            .map(|map| map.into_iter().map(|(k, _)| k).collect())
    }

    pub fn destroyed_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.destroyed.as_ref().map(|list| list.iter())
    }

    pub fn take_destroyed_ids(&mut self) -> Option<Vec<String>> {
        self.destroyed.take()
    }

    pub fn not_created_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.not_created.as_ref().map(|map| map.keys())
    }

    pub fn not_updated_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.not_updated.as_ref().map(|map| map.keys())
    }

    pub fn not_destroyed_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.not_destroyed.as_ref().map(|map| map.keys())
    }

    pub fn has_updated(&self) -> bool {
        self.updated.as_ref().is_some_and(|m| !m.is_empty())
    }

    pub fn has_created(&self) -> bool {
        self.created.as_ref().is_some_and(|m| !m.is_empty())
    }

    pub fn has_destroyed(&self) -> bool {
        self.destroyed.as_ref().is_some_and(|m| !m.is_empty())
    }

    pub fn unwrap_update_errors(&self) -> crate::Result<()> {
        if let Some(errors) = &self.not_updated {
            if let Some(err) = errors.values().next() {
                return Err(err.to_string_error().into());
            }
        }
        Ok(())
    }

    pub fn unwrap_create_errors(&self) -> crate::Result<()> {
        if let Some(errors) = &self.not_created {
            if let Some(err) = errors.values().next() {
                return Err(err.to_string_error().into());
            }
        }
        Ok(())
    }
}

impl<U: Display> SetError<U> {
    pub fn error(&self) -> &SetErrorType {
        &self.type_
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn properties(&self) -> Option<&[U]> {
        self.properties.as_deref()
    }

    pub fn to_string_error(&self) -> SetError<String> {
        SetError {
            type_: self.type_.clone(),
            description: self.description.as_ref().map(|s| s.to_string()),
            properties: self
                .properties
                .as_ref()
                .map(|s| s.iter().map(|s| s.to_string()).collect()),
        }
    }
}

impl<U: Display> Display for SetError<U> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.type_.fmt(f)?;
        if let Some(description) = &self.description {
            write!(f, ": {}", description)?;
        }
        if let Some(properties) = &self.properties {
            write!(
                f,
                " (properties: {})",
                properties
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}

impl Display for SetErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SetErrorType::Forbidden => write!(f, "forbidden"),
            SetErrorType::OverQuota => write!(f, "overQuota"),
            SetErrorType::TooLarge => write!(f, "tooLarge"),
            SetErrorType::RateLimit => write!(f, "rateLimit"),
            SetErrorType::NotFound => write!(f, "notFound"),
            SetErrorType::InvalidPatch => write!(f, "invalidPatch"),
            SetErrorType::WillDestroy => write!(f, "willDestroy"),
            SetErrorType::InvalidProperties => write!(f, "invalidProperties"),
            SetErrorType::Singleton => write!(f, "singleton"),
            SetErrorType::MailboxHasChild => write!(f, "mailboxHasChild"),
            SetErrorType::MailboxHasEmail => write!(f, "mailboxHasEmail"),
            SetErrorType::BlobNotFound => write!(f, "blobNotFound"),
            SetErrorType::TooManyKeywords => write!(f, "tooManyKeywords"),
            SetErrorType::TooManyMailboxes => write!(f, "tooManyMailboxes"),
            SetErrorType::ForbiddenFrom => write!(f, "forbiddenFrom"),
            SetErrorType::InvalidEmail => write!(f, "invalidEmail"),
            SetErrorType::TooManyRecipients => write!(f, "tooManyRecipients"),
            SetErrorType::NoRecipients => write!(f, "noRecipients"),
            SetErrorType::InvalidRecipients => write!(f, "invalidRecipients"),
            SetErrorType::ForbiddenMailFrom => write!(f, "forbiddenMailFrom"),
            SetErrorType::ForbiddenToSend => write!(f, "forbiddenToSend"),
            SetErrorType::CannotUnsend => write!(f, "cannotUnsend"),
            SetErrorType::AlreadyExists => write!(f, "alreadyExists"),
            SetErrorType::InvalidScript => write!(f, "invalidScript"),
            SetErrorType::ScriptIsActive => write!(f, "scriptIsActive"),
        }
    }
}

pub fn from_timestamp(timestamp: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(timestamp, 0).unwrap_or_default()
}

pub fn string_not_set(string: &Option<String>) -> bool {
    matches!(string, Some(string) if string.is_empty())
}

pub fn date_not_set(date: &Option<DateTime<Utc>>) -> bool {
    matches!(date, Some(date) if date.timestamp() == 0)
}

pub fn list_not_set<O>(list: &Option<Vec<O>>) -> bool {
    matches!(list, Some(list) if list.is_empty() )
}

pub fn map_not_set<K, V>(list: &Option<AHashMap<K, V>>) -> bool {
    matches!(list, Some(list) if list.is_empty() )
}

#[cfg(test)]
mod tests {
    //! Unit tests for the generic `Set` machinery: `SetRequest` create /
    //! update / destroy bookkeeping (RFC 8620 §5.3), the `accountId`
    //! gating rule, `SetError` composition and the `*_not_set` patch
    //! predicates. `Mailbox` is used as the concrete object; the
    //! account-less `PushSubscription` supplies the negative arm. No
    //! server, no socket, no async.

    use super::*;
    use crate::mailbox::{Mailbox, Property, SetArguments};
    use crate::push_subscription::PushSubscription;
    use crate::{Get, Method, Set};
    use serde_json::json;

    fn mailbox_request() -> SetRequest<Mailbox<Set>> {
        SetRequest::new(RequestParams::new("account-1", Method::SetMailbox, 0))
    }

    /// RFC 8620 §5.3 — a bare request carries only the account id; every
    /// `create()` call takes the next `c<n>` key, counted from the map it
    /// already holds.
    #[test]
    fn create_allocates_sequential_c_keys() {
        let mut req = mailbox_request();
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({ "accountId": "account-1" })
        );

        assert_eq!(req.create().create_id(), Some("c0".to_string()));
        assert_eq!(req.create().create_id(), Some("c1".to_string()));
        assert_eq!(req.create().create_id(), Some("c2".to_string()));

        let value = serde_json::to_value(&req).unwrap();
        let created = value["create"].as_object().unwrap();
        assert_eq!(created.len(), 3);
        for key in ["c0", "c1", "c2"] {
            assert!(created.contains_key(key), "{key} must be a create key");
        }
    }

    /// A caller-chosen create key is honoured verbatim, independently of
    /// the counter the object itself reports.
    #[test]
    fn create_with_id_uses_the_callers_key() {
        let mut req = mailbox_request();
        let item = req.create_with_id("myOwnId");
        assert_eq!(item.create_id(), Some("c0".to_string()));
        item.name("Inbox");

        let value = serde_json::to_value(&req).unwrap();
        let created = value["create"].as_object().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created["myOwnId"]["name"], json!("Inbox"));
        assert!(!created.contains_key("c0"));
    }

    /// `create_item` inserts a pre-built object and reports the key it got.
    #[test]
    fn create_item_reports_the_key_it_allocated() {
        let mut req = mailbox_request();
        assert_eq!(req.create_item(Mailbox::<Set>::new(None)), "c0");
        assert_eq!(req.create_item(Mailbox::<Set>::new(None)), "c1");
        assert_eq!(
            serde_json::to_value(&req).unwrap()["create"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
    }

    /// RFC 8620 §5.3 — `update` keys on the real id, not a create id.
    #[test]
    fn update_and_update_item_key_on_the_real_id() {
        let mut req = mailbox_request();
        req.update("mb-1").name("Renamed");
        req.update_item("mb-2", Mailbox::<Set>::new(None));

        let value = serde_json::to_value(&req).unwrap();
        let updated = value["update"].as_object().unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(updated["mb-1"]["name"], json!("Renamed"));
        assert_eq!(updated["mb-2"], json!({}));
        assert!(value.get("create").is_none());
    }

    /// RFC 8620 §5.3 — the by-id list and the result-reference form of
    /// `destroy` are mutually exclusive: setting one clears the other.
    #[test]
    fn destroy_by_id_and_destroy_by_reference_are_exclusive() {
        let mut req = mailbox_request();
        req.destroy(["mb-1", "mb-2"]);
        req.destroy(["mb-3"]);
        assert_eq!(
            serde_json::to_value(&req).unwrap()["destroy"],
            json!(["mb-1", "mb-2", "mb-3"])
        );

        req.destroy_ref(ResultReference::new(Method::QueryMailbox, 1, "/upToIds"));
        let value = serde_json::to_value(&req).unwrap();
        assert!(value.get("destroy").is_none());
        assert_eq!(
            value["#destroy"],
            json!({ "resultOf": "s1", "name": "Mailbox/query", "path": "/upToIds" })
        );

        req.destroy(["mb-9"]);
        let value = serde_json::to_value(&req).unwrap();
        assert!(value.get("#destroy").is_none());
        assert_eq!(value["destroy"], json!(["mb-9"]));
    }

    /// RFC 8620 §5.3 — `ifInState` is the optimistic-concurrency guard.
    #[test]
    fn if_in_state_is_sent_under_its_rfc_name() {
        let mut req = mailbox_request();
        req.if_in_state("415");
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({ "accountId": "account-1", "ifInState": "415" })
        );
    }

    /// `accountId` is only ever stored for objects that declare they need
    /// one; a `PushSubscription` is account-less and must stay that way
    /// even when a caller tries to set it.
    #[test]
    fn account_id_is_dropped_for_objects_that_do_not_require_it() {
        assert!(Mailbox::<Set>::requires_account_id());
        assert!(!PushSubscription::<Set>::requires_account_id());

        let mut req = mailbox_request();
        req.account_id("other-account");
        assert_eq!(
            serde_json::to_value(&req).unwrap()["accountId"],
            json!("other-account")
        );

        let mut push = SetRequest::<PushSubscription<Set>>::new(RequestParams::new(
            "account-1",
            Method::SetPushSubscription,
            0,
        ));
        assert!(push
            .account_id("other-account")
            .create()
            .create_id()
            .is_some());
        assert_eq!(
            serde_json::to_value(&push).unwrap(),
            json!({ "create": { "c0": {} } })
        );
    }

    /// The per-method arguments are flattened into the request body and
    /// stay reachable for mutation through `arguments()`.
    #[test]
    fn arguments_are_flattened_into_the_request_body() {
        let mut req = mailbox_request();
        *req.arguments() = SetArguments::default();

        let value = serde_json::to_value(&req).unwrap();
        assert!(
            value.get("arguments").is_none(),
            "arguments must be flattened, not nested"
        );
        assert_eq!(value, json!({ "accountId": "account-1" }));
    }

    /// RFC 8620 §5.3 — `SetError` renders type, then description, then
    /// the offending property list, and `to_string_error` widens the
    /// property type without losing anything.
    #[test]
    fn set_error_display_composes_its_three_parts() {
        let err: SetError<Property> = serde_json::from_value(json!({
            "type": "invalidProperties",
            "description": "Only name and parentId may be changed.",
            "properties": ["role", "sortOrder"]
        }))
        .unwrap();

        assert_eq!(err.error(), &SetErrorType::InvalidProperties);
        assert_eq!(
            err.description(),
            Some("Only name and parentId may be changed.")
        );
        assert_eq!(err.properties().unwrap().len(), 2);
        assert_eq!(
            err.to_string(),
            "invalidProperties: Only name and parentId may be changed. \
             (properties: role, sortOrder)"
        );

        let widened = err.to_string_error();
        assert_eq!(widened.error(), &SetErrorType::InvalidProperties);
        assert_eq!(widened.properties().unwrap(), &["role", "sortOrder"]);
        assert_eq!(widened.to_string(), err.to_string());
    }

    /// A bare `SetError` renders as just its type name.
    #[test]
    fn bare_set_error_renders_only_its_type() {
        let err: SetError<String> = serde_json::from_value(json!({ "type": "overQuota" })).unwrap();
        assert_eq!(err.description(), None);
        assert!(err.properties().is_none());
        assert_eq!(err.to_string(), "overQuota");
    }

    /// Every `SetErrorType` must `Display` exactly the wire name it
    /// `Deserialize`s from — the two tables cannot drift apart.
    #[test]
    fn set_error_type_display_matches_its_wire_name() {
        let cases: Vec<(SetErrorType, &str)> = vec![
            (SetErrorType::Forbidden, "forbidden"),
            (SetErrorType::OverQuota, "overQuota"),
            (SetErrorType::TooLarge, "tooLarge"),
            (SetErrorType::RateLimit, "rateLimit"),
            (SetErrorType::NotFound, "notFound"),
            (SetErrorType::InvalidPatch, "invalidPatch"),
            (SetErrorType::WillDestroy, "willDestroy"),
            (SetErrorType::InvalidProperties, "invalidProperties"),
            (SetErrorType::Singleton, "singleton"),
            (SetErrorType::MailboxHasChild, "mailboxHasChild"),
            (SetErrorType::MailboxHasEmail, "mailboxHasEmail"),
            (SetErrorType::BlobNotFound, "blobNotFound"),
            (SetErrorType::TooManyKeywords, "tooManyKeywords"),
            (SetErrorType::TooManyMailboxes, "tooManyMailboxes"),
            (SetErrorType::ForbiddenFrom, "forbiddenFrom"),
            (SetErrorType::InvalidEmail, "invalidEmail"),
            (SetErrorType::TooManyRecipients, "tooManyRecipients"),
            (SetErrorType::NoRecipients, "noRecipients"),
            (SetErrorType::InvalidRecipients, "invalidRecipients"),
            (SetErrorType::ForbiddenMailFrom, "forbiddenMailFrom"),
            (SetErrorType::ForbiddenToSend, "forbiddenToSend"),
            (SetErrorType::CannotUnsend, "cannotUnsend"),
            (SetErrorType::AlreadyExists, "alreadyExists"),
            (SetErrorType::InvalidScript, "invalidScript"),
            (SetErrorType::ScriptIsActive, "scriptIsActive"),
        ];

        assert_eq!(cases.len(), 25);
        for (variant, wire) in cases {
            assert_eq!(variant.to_string(), wire);
            assert_eq!(
                serde_json::from_value::<SetErrorType>(json!(wire)).unwrap(),
                variant,
                "{wire} must deserialize back to the same variant"
            );
        }
    }

    /// RFC 8620 §5.3 — the `*_not_set` predicates fire only on a *present*
    /// but empty/zero value; `None` never counts as "not set".
    #[test]
    fn not_set_predicates_only_fire_on_present_but_empty_values() {
        assert!(string_not_set(&Some(String::new())));
        assert!(!string_not_set(&Some("Inbox".to_string())));
        assert!(!string_not_set(&None));

        assert!(date_not_set(&Some(from_timestamp(0))));
        assert!(!date_not_set(&Some(from_timestamp(1_720_000_000))));
        assert!(!date_not_set(&None));

        assert!(list_not_set(&Some(Vec::<String>::new())));
        assert!(!list_not_set(&Some(vec!["a".to_string()])));
        assert!(!list_not_set::<String>(&None));

        let mut empty = AHashMap::new();
        assert!(map_not_set(&Some(empty.clone())));
        empty.insert("k", 1);
        assert!(!map_not_set(&Some(empty)));
        assert!(!map_not_set::<String, i32>(&None));
    }

    /// `from_timestamp` maps a Unix second onto an RFC 3339 instant and
    /// falls back to the epoch rather than panicking on garbage.
    #[test]
    fn from_timestamp_maps_seconds_and_falls_back_to_the_epoch() {
        assert_eq!(
            serde_json::to_value(from_timestamp(1_720_000_000)).unwrap(),
            json!("2024-07-03T09:46:40Z")
        );
        assert_eq!(from_timestamp(0).timestamp(), 0);
        // Out of range for chrono: unwrap_or_default() keeps it at the epoch.
        assert_eq!(from_timestamp(i64::MAX).timestamp(), 0);
    }

    /// RFC 8620 §5.3 — the full `Mailbox/set` response example, read
    /// through every accessor of `SetResponse`. `myRights` uses the
    /// crate's `mayXxx` object form rather than the RFC 8621 §7.1 letter
    /// array (see the matching Finding).
    #[test]
    fn set_response_reads_the_rfc_example() {
        let mut res: SetResponse<Mailbox<Get>> = serde_json::from_str(
            r#"{
            "accountId": "account-1",
            "oldState": "415",
            "newState": "416",
            "created": {
                "c1": {
                    "id": "f57bcc4a-8d87-456c-ace9-3426a5866e03",
                    "name": "Inbox",
                    "parentId": "bafc6987-4882-40b4-9d54-7973d4698d30",
                    "role": "inbox",
                    "sortOrder": 0,
                    "totalEmails": 0,
                    "unreadEmails": 0,
                    "totalThreads": 0,
                    "unreadThreads": 0,
                    "myRights": {
                        "mayReadItems": true,
                        "mayAddItems": true,
                        "mayRemoveItems": true,
                        "maySetSeen": true,
                        "maySetKeywords": true,
                        "mayCreateChild": true,
                        "mayRename": true,
                        "mayDelete": true,
                        "maySubmit": true
                    },
                    "isSubscribed": false
                }
            },
            "updated": { "mb-2": null },
            "destroyed": ["mb-3"],
            "notCreated": {
                "c2": {
                    "type": "overQuota",
                    "description": "The account is over its storage quota."
                }
            },
            "notUpdated": {
                "mb-9": { "type": "notFound" }
            }
        }"#,
        )
        .unwrap();

        assert_eq!(res.account_id(), Some("account-1"));
        assert_eq!(res.old_state(), Some("415"));
        assert_eq!(res.new_state(), "416");
        assert!(res.has_created());
        assert!(res.has_updated());
        assert!(res.has_destroyed());
        assert!(res.unwrap_create_errors().is_err());
        assert!(res.unwrap_update_errors().is_err());

        // Id views, taken before the lookups below drain the maps.
        assert_eq!(res.created_ids().unwrap().count(), 1);
        assert_eq!(res.updated_ids().unwrap().count(), 1);
        assert_eq!(res.destroyed_ids().unwrap().count(), 1);
        assert_eq!(res.not_created_ids().unwrap().count(), 1);
        assert_eq!(res.not_updated_ids().unwrap().count(), 1);
        assert!(res.not_destroyed_ids().is_none());

        assert_eq!(
            res.created("c1").unwrap().id(),
            Some("f57bcc4a-8d87-456c-ace9-3426a5866e03")
        );
        assert!(res.updated("mb-2").unwrap().is_none());
        assert!(res.destroyed("mb-3").is_ok());

        let not_created = res
            .created("c2")
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            not_created.contains("overQuota") && not_created.contains("storage quota"),
            "unexpected error text: {not_created}"
        );
        assert!(res
            .updated("mb-9")
            .unwrap_err()
            .to_string()
            .contains("notFound"));

        // Every successful lookup removes the entry it read, so the id
        // views now see the maps empty while `destroyed` is untouched.
        assert_eq!(res.created_ids().unwrap().count(), 0);
        assert_eq!(res.updated_ids().unwrap().count(), 0);
        assert_eq!(res.not_created_ids().unwrap().count(), 0);
        assert_eq!(res.not_updated_ids().unwrap().count(), 0);
        assert_eq!(res.destroyed_ids().unwrap().count(), 1);
    }

    /// The `take_*` views remove what they hand back, so a second read sees
    /// nothing.
    #[test]
    fn take_views_drain_the_response() {
        let mut res: SetResponse<Mailbox<Get>> = serde_json::from_value(json!({
            "newState": "416",
            "updated": { "mb-2": null },
            "destroyed": ["mb-3"]
        }))
        .unwrap();

        assert_eq!(res.take_new_state(), "416");
        assert_eq!(res.new_state(), "");
        assert_eq!(res.take_new_state(), "");

        assert_eq!(res.take_updated_ids().unwrap(), vec!["mb-2"]);
        assert!(res.take_updated_ids().is_none());
        assert!(!res.has_updated());

        assert_eq!(res.take_destroyed_ids().unwrap(), vec!["mb-3"]);
        assert!(res.take_destroyed_ids().is_none());
        assert!(!res.has_destroyed());
    }

    /// An id that appears in neither the result map nor the error map is an
    /// internal inconsistency, reported as such.
    #[test]
    fn unknown_ids_report_an_internal_lookup_failure() {
        let mut res: SetResponse<Mailbox<Get>> =
            serde_json::from_value(json!({ "newState": "1" })).unwrap();

        for message in [
            res.created("ghost").unwrap_err().to_string(),
            res.updated("ghost").unwrap_err().to_string(),
            res.destroyed("ghost").unwrap_err().to_string(),
        ] {
            assert!(
                message.contains("ghost not found"),
                "unexpected error text: {message}"
            );
        }
    }

    /// A response with no error maps at all is a clean success.
    #[test]
    fn a_clean_response_unwraps_to_ok() {
        let res: SetResponse<Mailbox<Get>> = serde_json::from_value(json!({
            "newState": "417",
            "updated": { "mb-1": null }
        }))
        .unwrap();
        assert!(res.unwrap_create_errors().is_ok());
        assert!(res.unwrap_update_errors().is_ok());
        assert!(!res.has_created());
        assert!(!res.has_destroyed());
    }

    /// An *empty* error map is also a clean success: the guard has to look
    /// for a member, not merely for the key being present.
    #[test]
    fn an_empty_error_map_is_not_an_error() {
        let res: SetResponse<Mailbox<Get>> = serde_json::from_value(json!({
            "newState": "418",
            "notCreated": {},
            "notUpdated": {}
        }))
        .unwrap();
        assert!(res.unwrap_create_errors().is_ok());
        assert!(res.unwrap_update_errors().is_ok());
        assert_eq!(res.not_created_ids().unwrap().count(), 0);
        assert_eq!(res.not_updated_ids().unwrap().count(), 0);
    }

    /// The trait's default `set_id` is a no-op, and that is exactly what
    /// the `<Get>` flavours inherit: they are not patch targets, so an id
    /// written back is dropped rather than overwriting the fetched one.
    #[test]
    fn the_default_set_id_is_a_noop() {
        let mut got: Mailbox<Get> = serde_json::from_value(json!({ "id": "mb-1" })).unwrap();
        got.set_id(Some("mb-2".to_string()));
        assert_eq!(
            got.id(),
            Some("mb-1"),
            "the default impl must leave the fetched id alone"
        );
    }
}
