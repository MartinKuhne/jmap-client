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

use serde::Serialize;

use crate::{
    core::query::{self, QueryObject},
    Set,
};

use super::{Mailbox, QueryArguments, Role};

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Filter {
    ParentId {
        #[serde(rename = "parentId")]
        value: Option<String>,
    },
    Name {
        #[serde(rename = "name")]
        value: String,
    },
    Role {
        #[serde(rename = "role")]
        value: Option<Role>,
    },
    HasAnyRole {
        #[serde(rename = "hasAnyRole")]
        value: bool,
    },
    IsSubscribed {
        #[serde(rename = "isSubscribed")]
        value: bool,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "property")]
pub enum Comparator {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "sortOrder")]
    SortOrder,
    #[serde(rename = "parentId")]
    ParentId,
}

impl Filter {
    pub fn parent_id(value: Option<impl Into<String>>) -> Self {
        Filter::ParentId {
            value: value.map(Into::into),
        }
    }

    pub fn name(value: impl Into<String>) -> Self {
        Filter::Name {
            value: value.into(),
        }
    }

    pub fn role(value: Role) -> Self {
        Filter::Role {
            value: if !matches!(value, Role::None) {
                value.into()
            } else {
                None
            },
        }
    }

    pub fn has_any_role(value: bool) -> Self {
        Filter::HasAnyRole { value }
    }

    pub fn is_subscribed(value: bool) -> Self {
        Filter::IsSubscribed { value }
    }
}

impl Comparator {
    pub fn name() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Name)
    }

    pub fn sort_order() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::SortOrder)
    }

    pub fn parent_id() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::ParentId)
    }
}

impl QueryArguments {
    pub fn sort_as_tree(&mut self, value: bool) -> &mut Self {
        self.sort_as_tree = value;
        self
    }

    pub fn filter_as_tree(&mut self, value: bool) -> &mut Self {
        self.filter_as_tree = value;
        self
    }
}

impl QueryObject for Mailbox<Set> {
    type QueryArguments = QueryArguments;

    type Filter = Filter;

    type Sort = Comparator;
}

#[cfg(test)]
mod tests {
    //! Unit tests for the Mailbox filter and comparator constructors
    //! (RFC 8621 §2.3).

    use super::*;
    use crate::{core::query::QueryRequest, core::RequestParams, Method};
    use serde_json::json;

    /// RFC 8621 §2.3 — `role` is nullable, and the constructor maps
    /// `Role::None` onto an explicit JSON `null` rather than the empty
    /// string `Role` itself serialises to. A filter for "no role" is
    /// therefore a member that is present and null.
    #[test]
    fn role_filter_maps_role_none_to_an_explicit_null() {
        assert_eq!(
            serde_json::to_value(Filter::role(Role::None)).unwrap(),
            json!({ "role": null }),
            "`Role::None` must not serialise as `\"\"`"
        );

        for (role, wire) in [
            (Role::Inbox, "inbox"),
            (Role::Sent, "sent"),
            (Role::Trash, "trash"),
            (Role::Drafts, "drafts"),
            (Role::Junk, "junk"),
            (Role::Archive, "archive"),
            (Role::Important, "important"),
            (Role::Other("blog".to_string()), "blog"),
        ] {
            assert_eq!(
                serde_json::to_value(Filter::role(role.clone())).unwrap(),
                json!({ "role": wire }),
                "a real role keeps its wire name inside the filter"
            );
        }
    }

    /// RFC 8621 §2.3 — `parentId` is nullable too, and unlike `role` the
    /// constructor passes the `Option` straight through: `None` means
    /// "top level", which is a null member, not an absent one.
    #[test]
    fn parent_id_filter_passes_the_optional_value_through() {
        assert_eq!(
            serde_json::to_value(Filter::parent_id(None::<String>)).unwrap(),
            json!({ "parentId": null })
        );
        assert_eq!(
            serde_json::to_value(Filter::parent_id(Some("MB23cfa8094c0f41e6"))).unwrap(),
            json!({ "parentId": "MB23cfa8094c0f41e6" })
        );
    }

    /// RFC 8621 §2.3 — the three comparator constructors each name one of
    /// the three sortable properties, wrapped in the generic envelope
    /// that defaults `isAscending` to true and omits `collation`.
    #[test]
    fn the_comparator_constructors_name_their_property() {
        for (comparator, property) in [
            (Comparator::name(), "name"),
            (Comparator::sort_order(), "sortOrder"),
            (Comparator::parent_id(), "parentId"),
        ] {
            assert_eq!(
                serde_json::to_value(&comparator).unwrap(),
                json!({ "isAscending": true, "property": property })
            );
            assert_eq!(
                serde_json::to_value(comparator.descending()).unwrap(),
                json!({ "isAscending": false, "property": property })
            );
        }

        assert_eq!(
            serde_json::to_value(Comparator::name().collation("i;unicode-casemap".to_string()))
                .unwrap(),
            json!({
                "isAscending": true,
                "collation": "i;unicode-casemap",
                "property": "name"
            }),
            "`collation` is omitted unless it is set"
        );
    }

    /// RFC 8621 §2.3 — a whole `Mailbox/query` with a filter, a sort and
    /// the two tree arguments flattened onto the same object.
    #[test]
    fn a_tree_query_serialises_the_rfc_8621_shape() {
        let subscribed: query::Filter<Filter> = Filter::is_subscribed(true).into();
        let inbox_role: query::Filter<Filter> = Filter::role(Role::Inbox).into();
        let mut request = QueryRequest::<Mailbox<Set>>::new(RequestParams::new(
            "u33084183",
            Method::QueryMailbox,
            0,
        ));
        request
            .filter(query::Filter::and([subscribed, inbox_role]))
            .sort([
                Comparator::sort_order(),
                Comparator::parent_id().descending(),
            ])
            .position(0)
            .limit(10)
            .calculate_total(true);
        request.arguments().sort_as_tree(true).filter_as_tree(false);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "filter": {
                    "operator": "AND",
                    "conditions": [
                        { "isSubscribed": true },
                        { "role": "inbox" }
                    ]
                },
                "sort": [
                    { "isAscending": true, "property": "sortOrder" },
                    { "isAscending": false, "property": "parentId" }
                ],
                "position": 0,
                "limit": 10,
                "calculateTotal": true,
                "sortAsTree": true,
                "filterAsTree": false
            })
        );
    }
}
