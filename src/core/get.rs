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

pub trait GetObject: Object {
    type GetArguments: Default;
}

#[derive(Debug, Clone, Serialize)]
pub struct GetRequest<O: GetObject> {
    #[serde(skip)]
    method: (Method, usize),

    #[serde(rename = "accountId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    ids: Option<Vec<String>>,

    #[serde(rename = "#ids")]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    ids_ref: Option<ResultReference>,

    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<Vec<O::Property>>,

    #[serde(rename = "#properties")]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    properties_ref: Option<ResultReference>,

    #[serde(flatten)]
    arguments: O::GetArguments,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetResponse<T> {
    #[serde(rename = "accountId")]
    account_id: Option<String>,

    state: String,

    list: Vec<T>,

    #[serde(rename = "notFound")]
    not_found: Vec<String>,
}

impl<O: GetObject> GetRequest<O> {
    pub fn new(params: RequestParams) -> Self {
        GetRequest {
            account_id: if O::requires_account_id() {
                params.account_id.into()
            } else {
                None
            },
            method: (params.method, params.call_id),
            ids: None,
            ids_ref: None,
            properties: None,
            properties_ref: None,
            arguments: O::GetArguments::default(),
        }
    }

    pub fn account_id(&mut self, account_id: impl Into<String>) -> &mut Self {
        if O::requires_account_id() {
            self.account_id = Some(account_id.into());
        }
        self
    }

    pub fn ids<U, V>(&mut self, ids: U) -> &mut Self
    where
        U: IntoIterator<Item = V>,
        V: Into<String>,
    {
        self.ids = Some(ids.into_iter().map(|v| v.into()).collect());
        self.ids_ref = None;
        self
    }

    pub fn ids_ref(&mut self, reference: ResultReference) -> &mut Self {
        self.ids_ref = reference.into();
        self.ids = None;
        self
    }

    pub fn properties(&mut self, properties: impl IntoIterator<Item = O::Property>) -> &mut Self {
        self.properties = Some(properties.into_iter().collect());
        self.properties_ref = None;
        self
    }

    pub fn properties_ref(&mut self, reference: ResultReference) -> &mut Self {
        self.properties_ref = Some(reference);
        self.properties = None;
        self
    }

    pub fn arguments(&mut self) -> &mut O::GetArguments {
        &mut self.arguments
    }

    pub fn result_reference(&self, property: O::Property) -> ResultReference {
        ResultReference::new(
            self.method.0,
            self.method.1,
            format!("/list/*/{}", property),
        )
    }
}

impl<O> GetResponse<O> {
    pub fn account_id(&self) -> Option<&str> {
        self.account_id.as_deref()
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn take_state(&mut self) -> String {
        std::mem::take(&mut self.state)
    }

    pub fn list(&self) -> &[O] {
        &self.list
    }

    pub fn not_found(&self) -> &[String] {
        &self.not_found
    }

    pub fn take_list(&mut self) -> Vec<O> {
        std::mem::take(&mut self.list)
    }
    pub fn pop(&mut self) -> Option<O> {
        self.list.pop()
    }

    pub fn take_not_found(&mut self) -> Vec<String> {
        std::mem::take(&mut self.not_found)
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the generic `*/get` request builder: the account
    //! gate, the literal-versus-reference argument forms, and the
    //! result-reference helper (RFC 8620 §5.1, §3.6, §7.2.1).

    use super::*;
    use crate::{
        email::{Email, Property as EmailProperty},
        push_subscription::PushSubscription,
        Set,
    };
    use serde_json::json;

    /// RFC 8620 §5.1/§7.2.1 — `accountId` is written for a type that
    /// needs one and is a deliberate no-op for a type that does not.
    /// `PushSubscription` is not account scoped, so neither the
    /// constructor nor the setter may put the member on the wire.
    #[test]
    fn account_id_is_written_only_for_types_that_need_it() {
        let mut request =
            GetRequest::<Email<Set>>::new(RequestParams::new("u33084183", Method::GetEmail, 0));
        assert_eq!(request.account_id.as_deref(), Some("u33084183"));
        assert_eq!(
            request.account_id("ue411d190").account_id.as_deref(),
            Some("ue411d190"),
            "the setter overrides the account the params carried"
        );

        let mut push = GetRequest::<PushSubscription<Set>>::new(RequestParams::new(
            "u33084183",
            Method::GetPushSubscription,
            0,
        ));
        assert_eq!(push.account_id, None, "the constructor drops it as well");
        push.account_id("ue411d190");
        assert_eq!(
            push.account_id, None,
            "the setter cannot add an account to a type that takes none"
        );
        assert_eq!(
            serde_json::to_value(&push).unwrap(),
            json!({}),
            "no `accountId` member reaches the wire"
        );
    }

    /// RFC 8620 §3.6 — the literal and the reference form of each
    /// argument are mutually exclusive: setting one clears the other, so
    /// a request can never carry both `ids` and `#ids`.
    #[test]
    fn literal_and_reference_forms_are_mutually_exclusive() {
        let mut request =
            GetRequest::<Email<Set>>::new(RequestParams::new("u33084183", Method::GetEmail, 0));

        request.ids(["M1", "M2"]);
        assert_eq!(request.ids.as_deref().unwrap(), ["M1", "M2"].as_slice());
        assert!(request.ids_ref.is_none());

        request.ids_ref(ResultReference::new(Method::QueryEmail, 1, "/ids"));
        assert!(
            request.ids.is_none(),
            "the reference form clears the literal"
        );
        assert!(request.properties.is_none());
        assert!(request.properties_ref.is_none());

        request.properties([EmailProperty::Id, EmailProperty::Subject]);
        assert_eq!(
            request.properties.as_deref().unwrap(),
            [EmailProperty::Id, EmailProperty::Subject].as_slice()
        );
        assert!(request.properties_ref.is_none());

        request.properties_ref(ResultReference::new(Method::QueryEmail, 2, "/properties"));
        assert!(
            request.properties.is_none(),
            "the reference form clears the literal"
        );
        assert!(
            request.ids.is_none(),
            "and leaves the id reference it does not own alone"
        );

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "#ids": { "resultOf": "s1", "name": "Email/query", "path": "/ids" },
                "#properties": {
                    "resultOf": "s2",
                    "name": "Email/query",
                    "path": "/properties"
                }
            })
        );
    }

    /// RFC 8620 §3.6 — the reference a `*/get` hands out points at
    /// `/list/*/<property>`, keyed by the method and call id the request
    /// was built with, so a follow-up call reads back exactly the
    /// property it asked for.
    #[test]
    fn result_reference_points_at_the_list_wildcard_of_the_property() {
        let request =
            GetRequest::<Email<Set>>::new(RequestParams::new("u33084183", Method::GetEmail, 3));

        for (property, path) in [
            (EmailProperty::Id, "/list/*/id"),
            (EmailProperty::ThreadId, "/list/*/threadId"),
            (EmailProperty::Subject, "/list/*/subject"),
        ] {
            assert_eq!(
                serde_json::to_value(request.result_reference(property.clone())).unwrap(),
                json!({
                    "resultOf": "s3",
                    "name": "Email/get",
                    "path": path,
                }),
                "{property:?} must resolve against the same call"
            );
        }
    }

    /// The flattened per-type arguments ride alongside the standard
    /// members and are reachable for mutation through `arguments()`.
    #[test]
    fn flattened_arguments_are_reachable() {
        let mut request =
            GetRequest::<Email<Set>>::new(RequestParams::new("u33084183", Method::GetEmail, 0));

        request.arguments().fetch_text_body_values(true);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "accountId": "u33084183", "fetchTextBodyValues": true })
        );
    }
}
