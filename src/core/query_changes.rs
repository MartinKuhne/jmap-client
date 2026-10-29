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

use super::{
    query::{Comparator, Filter, QueryObject},
    RequestParams,
};

#[derive(Debug, Clone, Serialize)]
pub struct QueryChangesRequest<O: QueryObject> {
    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "filter")]
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<Filter<O::Filter>>,

    #[serde(rename = "sort")]
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<Comparator<O::Sort>>>,

    #[serde(rename = "sinceQueryState")]
    since_query_state: String,

    #[serde(rename = "maxChanges")]
    #[serde(skip_serializing_if = "Option::is_none")]
    max_changes: Option<usize>,

    #[serde(rename = "upToId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    up_to_id: Option<String>,

    #[serde(rename = "calculateTotal")]
    calculate_total: bool,

    #[serde(flatten)]
    arguments: O::QueryArguments,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryChangesResponse {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "oldQueryState")]
    old_query_state: String,
    #[serde(rename = "newQueryState")]
    new_query_state: String,
    #[serde(rename = "total")]
    total: Option<usize>,
    #[serde(rename = "removed")]
    removed: Vec<String>,
    #[serde(rename = "added")]
    added: Vec<AddedItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddedItem {
    id: String,
    index: usize,
}

impl<O: QueryObject> QueryChangesRequest<O> {
    pub fn new(params: RequestParams, since_query_state: String) -> Self {
        QueryChangesRequest {
            account_id: params.account_id,
            filter: None,
            sort: None,
            since_query_state,
            max_changes: None,
            up_to_id: None,
            calculate_total: false,
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

    pub fn max_changes(&mut self, max_changes: usize) -> &mut Self {
        self.max_changes = Some(max_changes);
        self
    }

    pub fn up_to_id(&mut self, up_to_id: impl Into<String>) -> &mut Self {
        self.up_to_id = Some(up_to_id.into());
        self
    }

    pub fn calculate_total(&mut self, calculate_total: bool) -> &mut Self {
        self.calculate_total = calculate_total;
        self
    }

    pub fn arguments(&mut self) -> &mut O::QueryArguments {
        &mut self.arguments
    }
}

impl QueryChangesResponse {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn old_query_state(&self) -> &str {
        &self.old_query_state
    }

    pub fn new_query_state(&self) -> &str {
        &self.new_query_state
    }

    pub fn total(&self) -> Option<usize> {
        self.total
    }

    pub fn removed(&self) -> &[String] {
        &self.removed
    }

    pub fn added(&self) -> &[AddedItem] {
        &self.added
    }
}

impl AddedItem {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the generic `*/queryChanges` request builder
    //! (RFC 8620 §5.6, RFC 8621 §2.4).

    use super::*;
    use crate::{
        mailbox::{
            query::{Comparator as MailboxComparator, Filter as MailboxFilter},
            Mailbox,
        },
        Method, Set,
    };
    use serde_json::json;

    const SINCE: &str = "51a0c6b1-9c94-4b5a-9c69-1f0a2b3c4d5e";

    /// RFC 8620 §5.6 — `sinceQueryState` is the one member the
    /// constructor takes explicitly, `account_id` overrides the account
    /// the params carried, and `calculateTotal` is a plain `bool` so it
    /// is on the wire even when nobody sets it (Finding #35).
    #[test]
    fn account_id_setter_replaces_the_account_the_params_carried() {
        let mut request = QueryChangesRequest::<Mailbox<Set>>::new(
            RequestParams::new("u33084183", Method::QueryChangesMailbox, 1),
            SINCE.to_string(),
        );
        assert_eq!(request.account_id, "u33084183");
        assert_eq!(request.since_query_state, SINCE);

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
                "sinceQueryState": SINCE,
                "calculateTotal": false,
                "sortAsTree": false,
                "filterAsTree": false
            }),
            "the untouched request carries the account, the state it is \
             changing from, the always-present `calculateTotal`, and the \
             flattened Mailbox tree members (Finding #35)"
        );
    }

    /// RFC 8620 §5.6 / RFC 8621 §2.4 — every optional member of the
    /// request, set through the fluent API, lands on the wire under its
    /// own camelCase name alongside the Mailbox tree arguments.
    #[test]
    fn the_full_request_shape_carries_every_optional_member() {
        let mut request = QueryChangesRequest::<Mailbox<Set>>::new(
            RequestParams::new("u33084183", Method::QueryChangesMailbox, 2),
            SINCE.to_string(),
        );
        let subscribed: Filter<MailboxFilter> = MailboxFilter::is_subscribed(true).into();
        request
            .filter(Filter::and([subscribed]))
            .sort([MailboxComparator::name().descending()])
            .max_changes(16)
            .up_to_id("MB42")
            .calculate_total(true);
        request.arguments().sort_as_tree(true).filter_as_tree(true);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "filter": {
                    "operator": "AND",
                    "conditions": [{ "isSubscribed": true }]
                },
                "sort": [{ "isAscending": false, "property": "name" }],
                "sinceQueryState": SINCE,
                "maxChanges": 16,
                "upToId": "MB42",
                "calculateTotal": true,
                "sortAsTree": true,
                "filterAsTree": true
            })
        );
    }

    /// RFC 8620 §5.6 — `upToId` and `maxChanges` are independent:
    /// setting one leaves the other skipped, and `calculateTotal` stays
    /// `false` until it is asked for.
    #[test]
    fn optional_members_stay_off_the_wire_until_they_are_set() {
        let mut request = QueryChangesRequest::<Mailbox<Set>>::new(
            RequestParams::new("u33084183", Method::QueryChangesMailbox, 3),
            SINCE.to_string(),
        );
        let wire = serde_json::to_value(&request).unwrap();
        assert!(
            wire.get("upToId").is_none() && wire.get("maxChanges").is_none(),
            "neither optional member is serialised by default, got {wire}"
        );

        request.up_to_id("MB7");
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "sinceQueryState": SINCE,
                "calculateTotal": false,
                "upToId": "MB7",
                "sortAsTree": false,
                "filterAsTree": false
            })
        );

        request.max_changes(0);
        assert_eq!(
            serde_json::to_value(&request).unwrap()["maxChanges"],
            json!(0),
            "the client does not validate `maxChanges` (Finding #36)"
        );
    }
}
