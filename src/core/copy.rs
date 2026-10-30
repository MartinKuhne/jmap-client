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

use ahash::AHashMap;
use serde::{Deserialize, Serialize};

use crate::Error;

use super::{
    set::{SetError, SetObject},
    RequestParams,
};

#[derive(Debug, Clone, Serialize)]
pub struct CopyRequest<O: SetObject> {
    #[serde(rename = "fromAccountId")]
    from_account_id: String,

    #[serde(rename = "ifFromInState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    if_from_in_state: Option<String>,

    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "ifInState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    if_in_state: Option<String>,

    #[serde(rename = "create")]
    create: AHashMap<String, O>,

    #[serde(rename = "onSuccessDestroyOriginal")]
    on_success_destroy_original: bool,

    #[serde(rename = "destroyFromIfInState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    destroy_from_if_in_state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CopyResponse<O: SetObject> {
    #[serde(rename = "fromAccountId")]
    from_account_id: String,

    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "oldState")]
    old_state: Option<String>,

    #[serde(rename = "newState")]
    new_state: String,

    #[serde(rename = "created")]
    created: Option<AHashMap<String, O>>,

    #[serde(rename = "notCreated")]
    not_created: Option<AHashMap<String, SetError<O::Property>>>,
}

impl<T: SetObject> CopyRequest<T> {
    pub fn new(params: RequestParams, from_account_id: String) -> Self {
        CopyRequest {
            from_account_id,
            if_from_in_state: None,
            account_id: params.account_id,
            if_in_state: None,
            create: AHashMap::new(),
            on_success_destroy_original: false,
            destroy_from_if_in_state: None,
        }
    }

    pub fn account_id(&mut self, account_id: impl Into<String>) -> &mut Self {
        self.account_id = account_id.into();
        self
    }

    pub fn if_from_in_state(&mut self, if_from_in_state: impl Into<String>) -> &mut Self {
        self.if_from_in_state = Some(if_from_in_state.into());
        self
    }

    pub fn if_in_state(&mut self, if_in_state: impl Into<String>) -> &mut Self {
        self.if_in_state = Some(if_in_state.into());
        self
    }

    pub fn create(&mut self, id: impl Into<String>) -> &mut T {
        let id = id.into();
        let mut object = T::new(None);
        object.set_id(Some(id.clone()));
        self.create.insert(id.clone(), object);
        self.create.get_mut(&id).unwrap()
    }

    pub fn on_success_destroy_original(&mut self, on_success_destroy_original: bool) -> &mut Self {
        self.on_success_destroy_original = on_success_destroy_original;
        self
    }

    pub fn destroy_from_if_in_state(
        &mut self,
        destroy_from_if_in_state: impl Into<String>,
    ) -> &mut Self {
        self.destroy_from_if_in_state = Some(destroy_from_if_in_state.into());
        self
    }
}

impl<O: SetObject> CopyResponse<O> {
    pub fn from_account_id(&self) -> &str {
        &self.from_account_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn old_state(&self) -> Option<&str> {
        self.old_state.as_deref()
    }

    pub fn new_state(&self) -> &str {
        &self.new_state
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

    pub fn take_created(&mut self) -> Option<Vec<O>> {
        self.created
            .take()
            .map(|map| map.into_iter().map(|(_, v)| v).collect())
    }

    pub fn created_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.created.as_ref().map(|map| map.keys())
    }

    pub fn not_created_ids(&self) -> Option<impl Iterator<Item = &String>> {
        self.not_created.as_ref().map(|map| map.keys())
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the copy request builder and the copy response
    //! accessors (RFC 8621 §4.7).

    use super::*;
    use crate::{email::Email, Get, Method, Set};
    use serde_json::json;

    /// RFC 8621 §4.7 — the request starts as the minimal shape (source
    /// account, destination account, empty create map, original kept) and
    /// every optional member is written only once the caller asks for it.
    #[test]
    fn copy_request_starts_minimal_and_grows_through_the_builder() {
        let mut request = CopyRequest::<Email<Set>>::new(
            RequestParams::new("unused", Method::CopyEmail, 0),
            "ue150411c".to_string(),
        );

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "fromAccountId": "ue150411c",
                "accountId": "unused",
                "create": {},
                "onSuccessDestroyOriginal": false,
            }),
            "the two state preconditions are skipped until they are set"
        );

        request
            .account_id("u6c6c41ac")
            .if_from_in_state("7ee7e9263a6d")
            .if_in_state("5a0d2447ed26")
            .on_success_destroy_original(true)
            .destroy_from_if_in_state("7ee7e9263a6d");

        // `create` hands back the new object so the caller can fill it in
        let created = request.create("k45");
        created.mailbox_ids(["a"]);
        created.keywords(["keyword"]);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "fromAccountId": "ue150411c",
                "ifFromInState": "7ee7e9263a6d",
                "accountId": "u6c6c41ac",
                "ifInState": "5a0d2447ed26",
                "create": {
                    "k45": {
                        "id": "k45",
                        "mailboxIds": { "a": true },
                        "keywords": { "keyword": true }
                    }
                },
                "onSuccessDestroyOriginal": true,
                "destroyFromIfInState": "7ee7e9263a6d"
            }),
            "the RFC §4.7 request shape, member for member"
        );
    }

    /// `account_id` overrides the account the builder was seeded from,
    /// which is how a caller copies into an account other than the
    /// session default.
    #[test]
    fn account_id_overrides_the_params_account() {
        let mut request = CopyRequest::<Email<Set>>::new(
            RequestParams::new("session-default", Method::CopyEmail, 1),
            "source".to_string(),
        );
        assert_eq!(request.account_id, "session-default");
        request.account_id("chosen");
        assert_eq!(request.account_id, "chosen");
        assert_eq!(request.if_from_in_state, None);
        assert_eq!(request.if_in_state, None);
    }

    /// RFC 8621 §4.7 — `take_created` drains the whole `created` map
    /// into a `Vec`, discarding the creation ids. A second call reports
    /// `None`: the records are gone, not merely empty.
    #[test]
    fn take_created_drains_the_created_map() {
        let mut response: CopyResponse<Email<Get>> = serde_json::from_value(json!({
            "fromAccountId": "ue150411c",
            "accountId": "u6c6c41ac",
            "oldState": "7ee7e9263a6d",
            "newState": "5a0d2447ed26",
            "created": {
                "k45": {
                    "id": "M138f9954a5cd2423daeafa55",
                    "blobId": "G6b9fb047cba722c48c611e79233d057c6b0b74e8",
                    "threadId": "T2f242ea424a4079a",
                    "size": 11721
                }
            },
            "notCreated": null
        }))
        .unwrap();

        assert_eq!(response.from_account_id(), "ue150411c");
        assert_eq!(response.account_id(), "u6c6c41ac");
        assert_eq!(response.old_state(), Some("7ee7e9263a6d"));
        assert_eq!(response.new_state(), "5a0d2447ed26");

        let taken = response
            .take_created()
            .expect("the response carried records");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].id(), Some("M138f9954a5cd2423daeafa55"));
        assert_eq!(taken[0].thread_id(), Some("T2f242ea424a4079a"));
        assert_eq!(taken[0].size(), 11721);

        assert!(
            response.take_created().is_none(),
            "the map is taken, not emptied"
        );
        assert!(response.created_ids().is_none());
    }

    /// A response that never carried a `created` member drains to `None`
    /// rather than to an empty `Vec`, so callers can tell "no member" from
    /// "member with no records".
    #[test]
    fn take_created_reports_none_when_the_member_is_absent() {
        let mut response: CopyResponse<Email<Get>> = serde_json::from_value(json!({
            "fromAccountId": "ue150411c",
            "accountId": "u6c6c41ac",
            "newState": "5a0d2447ed26",
            "notCreated": {
                "k45": { "type": "notFound", "description": "Email not found" }
            }
        }))
        .unwrap();

        assert!(response.old_state().is_none(), "`oldState` is optional");
        assert!(response.take_created().is_none());
        assert_eq!(
            response.created("k45").unwrap_err().to_string(),
            "Set failed: notFound: Email not found"
        );
    }
}
