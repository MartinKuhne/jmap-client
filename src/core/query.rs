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

pub trait QueryObject: Object {
    type QueryArguments: Default + Serialize;
    type Filter: Serialize;
    type Sort: Serialize;
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryRequest<O: QueryObject> {
    #[serde(skip)]
    method: (Method, usize),

    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "filter")]
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<Filter<O::Filter>>,

    #[serde(rename = "sort")]
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<Comparator<O::Sort>>>,

    #[serde(rename = "position")]
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<i32>,

    #[serde(rename = "anchor")]
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,

    #[serde(rename = "anchorOffset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor_offset: Option<i32>,

    #[serde(rename = "limit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,

    #[serde(rename = "calculateTotal")]
    #[serde(skip_serializing_if = "Option::is_none")]
    calculate_total: Option<bool>,

    #[serde(flatten)]
    arguments: O::QueryArguments,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Filter<T> {
    FilterOperator(FilterOperator<T>),
    FilterCondition(T),
}

#[derive(Debug, Clone, Serialize)]
pub struct FilterOperator<T> {
    operator: Operator,
    conditions: Vec<Filter<T>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Operator {
    #[serde(rename = "AND")]
    And,
    #[serde(rename = "OR")]
    Or,
    #[serde(rename = "NOT")]
    Not,
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparator<A> {
    #[serde(rename = "isAscending")]
    is_ascending: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    collation: Option<String>,

    #[serde(flatten)]
    arguments: A,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryResponse {
    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "queryState")]
    query_state: String,

    #[serde(rename = "canCalculateChanges")]
    can_calculate_changes: Option<bool>,

    #[serde(rename = "position")]
    position: i32,

    #[serde(rename = "ids")]
    ids: Vec<String>,

    #[serde(rename = "total")]
    total: Option<usize>,

    #[serde(rename = "limit")]
    limit: Option<usize>,
}

impl<O: QueryObject> QueryRequest<O> {
    pub fn new(params: RequestParams) -> Self {
        QueryRequest {
            account_id: params.account_id,
            method: (params.method, params.call_id),
            filter: None,
            sort: None,
            position: None,
            anchor: None,
            anchor_offset: None,
            limit: None,
            calculate_total: None,
            arguments: O::QueryArguments::default(),
        }
    }

    pub fn account_id(&mut self, account_id: impl Into<String>) -> &mut Self {
        self.account_id = account_id.into();
        self
    }

    pub fn filter(&mut self, filter: impl Into<Filter<O::Filter>>) -> &mut Self {
        self.filter = Some(filter.into());
        self
    }

    pub fn sort(&mut self, sort: impl IntoIterator<Item = Comparator<O::Sort>>) -> &mut Self {
        self.sort = Some(sort.into_iter().collect());
        self
    }

    pub fn position(&mut self, position: i32) -> &mut Self {
        self.position = position.into();
        self
    }

    pub fn anchor(&mut self, anchor: impl Into<String>) -> &mut Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub fn anchor_offset(&mut self, anchor_offset: i32) -> &mut Self {
        self.anchor_offset = anchor_offset.into();
        self
    }

    pub fn limit(&mut self, limit: usize) -> &mut Self {
        self.limit = Some(limit);
        self
    }

    pub fn calculate_total(&mut self, calculate_total: bool) -> &mut Self {
        self.calculate_total = Some(calculate_total);
        self
    }

    pub fn arguments(&mut self) -> &mut O::QueryArguments {
        &mut self.arguments
    }

    pub fn result_reference(&self) -> ResultReference {
        ResultReference::new(self.method.0, self.method.1, "/ids")
    }
}

impl QueryResponse {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn id(&self, pos: usize) -> &str {
        self.ids[pos].as_str()
    }

    pub fn take_ids(&mut self) -> Vec<String> {
        std::mem::take(&mut self.ids)
    }

    pub fn total(&self) -> Option<usize> {
        self.total
    }

    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    pub fn position(&self) -> i32 {
        self.position
    }

    pub fn take_query_state(&mut self) -> String {
        std::mem::take(&mut self.query_state)
    }

    pub fn query_state(&self) -> &str {
        &self.query_state
    }

    pub fn can_calculate_changes(&self) -> bool {
        self.can_calculate_changes.unwrap_or(false)
    }
}

impl<A> Comparator<A> {
    pub fn new(arguments: A) -> Self {
        Comparator {
            is_ascending: true,
            collation: None,
            arguments,
        }
    }

    pub fn descending(mut self) -> Self {
        self.is_ascending = false;
        self
    }

    pub fn ascending(mut self) -> Self {
        self.is_ascending = true;
        self
    }

    pub fn is_ascending(mut self, is_ascending: bool) -> Self {
        self.is_ascending = is_ascending;
        self
    }

    pub fn collation(mut self, collation: String) -> Self {
        self.collation = Some(collation);
        self
    }
}

impl<T> From<FilterOperator<T>> for Filter<T> {
    fn from(filter: FilterOperator<T>) -> Self {
        Filter::FilterOperator(filter)
    }
}

impl<T> From<T> for Filter<T> {
    fn from(filter: T) -> Self {
        Filter::FilterCondition(filter)
    }
}

impl<T> Filter<T> {
    pub fn operator(operator: Operator, conditions: Vec<Filter<T>>) -> Self {
        Filter::FilterOperator(FilterOperator {
            operator,
            conditions,
        })
    }

    pub fn and<U, V>(conditions: U) -> Self
    where
        U: IntoIterator<Item = V>,
        V: Into<Filter<T>>,
    {
        Filter::FilterOperator(FilterOperator {
            operator: Operator::And,
            conditions: conditions.into_iter().map(|t| t.into()).collect(),
        })
    }

    pub fn or<U, V>(conditions: U) -> Self
    where
        U: IntoIterator<Item = V>,
        V: Into<Filter<T>>,
    {
        Filter::FilterOperator(FilterOperator {
            operator: Operator::Or,
            conditions: conditions.into_iter().map(|t| t.into()).collect(),
        })
    }

    pub fn not<U, V>(conditions: U) -> Self
    where
        U: IntoIterator<Item = V>,
        V: Into<Filter<T>>,
    {
        Filter::FilterOperator(FilterOperator {
            operator: Operator::Not,
            conditions: conditions.into_iter().map(|t| t.into()).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the generic `*/query` request builder and the
    //! filter-combinator constructors (RFC 8620 §5.5).

    use super::*;
    use crate::{
        mailbox::{query::Filter as MailboxFilter, Mailbox},
        Set,
    };
    use serde_json::json;

    /// RFC 8620 §5.5 — `accountId` starts from whatever the
    /// `RequestParams` carried and the setter replaces it in place,
    /// leaving every other member of the request untouched.
    #[test]
    fn account_id_setter_replaces_the_account_the_params_carried() {
        let mut request = QueryRequest::<Mailbox<Set>>::new(RequestParams::new(
            "u33084183",
            Method::QueryMailbox,
            0,
        ));
        assert_eq!(request.account_id, "u33084183");

        // `impl Into<String>` takes an owned string without a copy
        let owned: String = "ue411d190".into();
        request.account_id(owned);
        assert_eq!(request.account_id, "ue411d190");

        // the setter hands back `&mut Self`, so calls chain
        request.account_id("a").account_id("b");
        assert_eq!(request.account_id, "b");

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "b",
                "sortAsTree": false,
                "filterAsTree": false
            }),
            "with no criteria set the request carries the account plus the \
             flattened Mailbox extension, whose two plain `bool` members are \
             always on the wire (Finding #35)"
        );
    }

    /// RFC 8620 §5.5 — the two `From` impls decide which arm of the
    /// untagged `Filter` enum a value lands in: an operator tree goes to
    /// `FilterOperator`, a bare condition to `FilterCondition`. They are
    /// what lets `QueryRequest::filter` accept `impl Into<Filter<T>>`.
    #[test]
    fn the_from_impls_pick_the_matching_filter_arm() {
        let conditions: Vec<Filter<MailboxFilter>> =
            vec![MailboxFilter::is_subscribed(true).into()];
        let operator = FilterOperator {
            operator: Operator::And,
            conditions,
        };

        let filter: Filter<MailboxFilter> = operator.clone().into();
        assert!(
            matches!(filter, Filter::FilterOperator(_)),
            "`From<FilterOperator<T>>` must land in the operator arm"
        );
        assert_eq!(
            serde_json::to_value(&filter).unwrap(),
            json!({ "operator": "AND", "conditions": [{ "isSubscribed": true }] })
        );

        // the builder takes the very same `From` impl for a raw operator
        let mut request = QueryRequest::<Mailbox<Set>>::new(RequestParams::new(
            "u33084183",
            Method::QueryMailbox,
            1,
        ));
        request.filter(operator);
        assert!(
            matches!(request.filter, Some(Filter::FilterOperator(_))),
            "passing a `FilterOperator` to the builder must not wrap it twice"
        );

        let condition: Filter<MailboxFilter> = MailboxFilter::name("INBOX").into();
        let wrapped = matches!(
            condition,
            Filter::FilterCondition(MailboxFilter::Name { .. })
        );
        assert!(wrapped, "`From<T>` must land in the condition arm");
        assert_eq!(
            serde_json::to_value(&condition).unwrap(),
            json!({ "name": "INBOX" })
        );
    }

    /// RFC 8620 §5.5 — `Filter::operator` builds an arbitrary
    /// AND/OR/NOT tree. Each level serialises as its own
    /// `{operator, conditions}` object and `conditions` holds filters of
    /// exactly the same shape, so the nesting is unbounded.
    #[test]
    fn operator_builds_an_arbitrary_and_or_not_tree() {
        assert_eq!(serde_json::to_value(Operator::And).unwrap(), json!("AND"));
        assert_eq!(serde_json::to_value(Operator::Or).unwrap(), json!("OR"));
        assert_eq!(serde_json::to_value(Operator::Not).unwrap(), json!("NOT"));

        let inbox: Filter<MailboxFilter> = MailboxFilter::name("INBOX").into();
        let subscribed: Filter<MailboxFilter> = MailboxFilter::is_subscribed(true).into();
        let and_group: Filter<MailboxFilter> = Filter::and([inbox.clone(), subscribed.clone()]);
        let not_group: Filter<MailboxFilter> =
            Filter::operator(Operator::Not, vec![subscribed.clone()]);
        let tree: Filter<MailboxFilter> =
            Filter::operator(Operator::Or, vec![and_group, not_group]);

        assert_eq!(
            serde_json::to_value(&tree).unwrap(),
            json!({
                "operator": "OR",
                "conditions": [
                    {
                        "operator": "AND",
                        "conditions": [
                            { "name": "INBOX" },
                            { "isSubscribed": true }
                        ]
                    },
                    {
                        "operator": "NOT",
                        "conditions": [{ "isSubscribed": true }]
                    }
                ]
            })
        );

        // an operator with no conditions is still a well-formed filter
        assert_eq!(
            serde_json::to_value(Filter::<MailboxFilter>::operator(Operator::And, Vec::new()))
                .unwrap(),
            json!({ "operator": "AND", "conditions": [] })
        );
    }

    /// RFC 8620 §5.5 — a filter built with `Filter::operator` survives
    /// being handed to the request builder, which is the only way a
    /// caller gets a nested tree onto the wire.
    #[test]
    fn a_nested_operator_reaches_the_wire_through_the_builder() {
        let inbox: Filter<MailboxFilter> = MailboxFilter::name("INBOX").into();
        let any_role: Filter<MailboxFilter> = MailboxFilter::has_any_role(true).into();
        let inner: Filter<MailboxFilter> = Filter::or([inbox, any_role]);
        let outer: Filter<MailboxFilter> = Filter::operator(Operator::Not, vec![inner]);

        let mut request = QueryRequest::<Mailbox<Set>>::new(RequestParams::new(
            "u33084183",
            Method::QueryMailbox,
            2,
        ));
        request.filter(outer).limit(25);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "filter": {
                    "operator": "NOT",
                    "conditions": [
                        {
                            "operator": "OR",
                            "conditions": [{ "name": "INBOX" }, { "hasAnyRole": true }]
                        }
                    ]
                },
                "limit": 25,
                "sortAsTree": false,
                "filterAsTree": false
            })
        );
    }
}
