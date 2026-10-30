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

use serde::{Deserialize, Serialize};

use crate::Method;

use super::{request::ResultReference, Object, RequestParams};

pub trait ChangesObject: Object {
    type ChangesResponse;
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangesRequest {
    #[serde(skip)]
    method: (Method, usize),

    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "sinceState")]
    since_state: String,

    #[serde(rename = "maxChanges")]
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangesResponse<O: ChangesObject> {
    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "oldState")]
    old_state: String,

    #[serde(rename = "newState")]
    new_state: String,

    #[serde(rename = "hasMoreChanges")]
    has_more_changes: bool,

    created: Vec<String>,

    updated: Vec<String>,

    destroyed: Vec<String>,

    #[serde(flatten)]
    arguments: O::ChangesResponse,
}

impl ChangesRequest {
    pub fn new(params: RequestParams, since_state: String) -> Self {
        ChangesRequest {
            method: (params.method, params.call_id),
            account_id: params.account_id,
            since_state,
            max_changes: None,
        }
    }

    pub fn account_id(&mut self, account_id: impl Into<String>) -> &mut Self {
        self.account_id = account_id.into();
        self
    }

    pub fn max_changes(&mut self, max_changes: usize) -> &mut Self {
        self.max_changes = Some(max_changes);
        self
    }

    pub fn created_reference(&self) -> ResultReference {
        ResultReference::new(self.method.0, self.method.1, "/created")
    }

    pub fn updated_reference(&self) -> ResultReference {
        ResultReference::new(self.method.0, self.method.1, "/updated")
    }

    pub fn updated_properties_reference(&self) -> ResultReference {
        ResultReference::new(self.method.0, self.method.1, "/updatedProperties")
    }
}

impl<O: ChangesObject> ChangesResponse<O> {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn take_account_id(&mut self) -> String {
        std::mem::take(&mut self.account_id)
    }

    pub fn old_state(&self) -> &str {
        &self.old_state
    }

    pub fn new_state(&self) -> &str {
        &self.new_state
    }

    pub fn take_new_state(&mut self) -> String {
        std::mem::take(&mut self.new_state)
    }

    pub fn has_more_changes(&self) -> bool {
        self.has_more_changes
    }

    pub fn created(&self) -> &[String] {
        &self.created
    }

    pub fn take_created(&mut self) -> Vec<String> {
        std::mem::take(&mut self.created)
    }

    pub fn updated(&self) -> &[String] {
        &self.updated
    }

    pub fn take_updated(&mut self) -> Vec<String> {
        std::mem::take(&mut self.updated)
    }

    pub fn destroyed(&self) -> &[String] {
        &self.destroyed
    }

    pub fn take_destroyed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.destroyed)
    }

    pub fn arguments(&self) -> &O::ChangesResponse {
        &self.arguments
    }

    pub fn total_changes(&self) -> usize {
        self.created.len() + self.updated.len() + self.destroyed.len()
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the generic `*/changes` request builder
    //! (RFC 8620 §5.2) and its result-reference helpers (§3.6).

    use super::*;
    use serde_json::json;

    const SINCE: &str = "51a0c6b1-9c94-4b5a-9c69-1f0a2b3c4d5e";

    /// RFC 8620 §5.2 — `sinceState` arrives through the constructor,
    /// `maxChanges` is skipped until it is asked for, and `account_id`
    /// overrides the account the params carried.
    #[test]
    fn account_id_setter_replaces_the_account_the_params_carried() {
        let mut request = ChangesRequest::new(
            RequestParams::new("u33084183", Method::ChangesMailbox, 2),
            SINCE.to_string(),
        );
        assert_eq!(request.account_id, "u33084183");
        assert_eq!(request.since_state, SINCE);
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "sinceState": SINCE
            }),
            "`maxChanges` is skipped when unset"
        );

        let owned: String = "ue411d190".into();
        request.account_id(owned);
        assert_eq!(request.account_id, "ue411d190");

        // the setter hands back `&mut Self`, so calls chain
        request.account_id("a").account_id("b");
        assert_eq!(request.account_id, "b");

        request.max_changes(16);
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "b",
                "sinceState": SINCE,
                "maxChanges": 16
            })
        );
    }

    /// RFC 8620 §3.6 — the three helpers name the three list members of
    /// a `/changes` response and carry the request's own method name and
    /// call id, so a follow-up `*/get` can reference them by `resultOf`.
    #[test]
    fn the_reference_helpers_name_the_changes_lists() {
        let request = ChangesRequest::new(
            RequestParams::new("u33084183", Method::ChangesMailbox, 7),
            SINCE.to_string(),
        );

        for (reference, path) in [
            (request.created_reference(), "/created"),
            (request.updated_reference(), "/updated"),
            (request.updated_properties_reference(), "/updatedProperties"),
        ] {
            assert_eq!(
                serde_json::to_value(reference).unwrap(),
                json!({
                    "resultOf": "s7",
                    "name": "Mailbox/changes",
                    "path": path
                })
            );
        }
    }
}
