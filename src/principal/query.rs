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

use super::{Principal, Type};

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Filter {
    Email {
        #[serde(rename = "email")]
        value: String,
    },
    Name {
        #[serde(rename = "name")]
        value: String,
    },
    DomainName {
        #[serde(rename = "domainName")]
        value: String,
    },
    Text {
        #[serde(rename = "text")]
        value: String,
    },
    Type {
        #[serde(rename = "type")]
        value: Type,
    },
    Timezone {
        #[serde(rename = "timezone")]
        value: String,
    },
    Members {
        #[serde(rename = "members")]
        value: String,
    },
    QuotaLt {
        #[serde(rename = "quotaLowerThan")]
        value: u32,
    },
    QuotaGt {
        #[serde(rename = "quotaGreaterThan")]
        value: u32,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "property")]
pub enum Comparator {
    #[serde(rename = "type")]
    Type,
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "email")]
    Email,
}

impl Filter {
    pub fn name(value: impl Into<String>) -> Self {
        Filter::Name {
            value: value.into(),
        }
    }

    pub fn domain_name(value: impl Into<String>) -> Self {
        Filter::DomainName {
            value: value.into(),
        }
    }

    pub fn email(value: impl Into<String>) -> Self {
        Filter::Email {
            value: value.into(),
        }
    }

    pub fn text(value: impl Into<String>) -> Self {
        Filter::Text {
            value: value.into(),
        }
    }

    pub fn timezone(value: impl Into<String>) -> Self {
        Filter::Timezone {
            value: value.into(),
        }
    }

    pub fn members(value: impl Into<String>) -> Self {
        Filter::Members {
            value: value.into(),
        }
    }

    pub fn ptype(value: Type) -> Self {
        Filter::Type { value }
    }

    pub fn quota_lower_than(value: u32) -> Self {
        Filter::QuotaLt { value }
    }

    pub fn quota_greater_than(value: u32) -> Self {
        Filter::QuotaGt { value }
    }
}

impl Comparator {
    pub fn name() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Name)
    }

    pub fn email() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Email)
    }

    pub fn ptype() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Type)
    }
}

impl QueryObject for Principal<Set> {
    type QueryArguments = ();

    type Filter = Filter;

    type Sort = Comparator;
}

#[cfg(test)]
mod tests {
    //! Unit tests for the Principal filter and comparator constructors
    //! (RFC 9670 §4.4).

    use super::*;
    use crate::{core::query::QueryRequest, core::RequestParams, Method};
    use serde_json::json;

    /// RFC 9670 §4.4 — the three sortable properties are `type`, `name`
    /// and `email`. The `type` pair is spelled `ptype` on both sides
    /// because `type` is a Rust keyword, but the wire name is the RFC's.
    #[test]
    fn the_comparator_constructors_name_their_property() {
        for (comparator, property) in [
            (Comparator::name(), "name"),
            (Comparator::email(), "email"),
            (Comparator::ptype(), "type"),
        ] {
            assert_eq!(
                serde_json::to_value(&comparator).unwrap(),
                json!({ "isAscending": true, "property": property }),
                "the generic envelope defaults `isAscending` to true"
            );
            assert_eq!(
                serde_json::to_value(comparator.descending()).unwrap(),
                json!({ "isAscending": false, "property": property })
            );
        }

        assert_eq!(
            serde_json::to_value(Comparator::ptype().is_ascending(false)).unwrap(),
            json!({ "isAscending": false, "property": "type" }),
            "`is_ascending(bool)` is the same knob as `descending()`"
        );
    }

    /// RFC 9670 §4.4 — `Filter::ptype` filters on the principal's type
    /// under the wire name `type`, one member per RFC type.
    #[test]
    fn filter_ptype_carries_the_principal_type() {
        for (ptype, wire) in [
            (Type::Individual, "individual"),
            (Type::Group, "group"),
            (Type::Resource, "resource"),
            (Type::Location, "location"),
            (Type::Domain, "domain"),
            (Type::List, "list"),
            (Type::Other, "other"),
        ] {
            assert_eq!(
                serde_json::to_value(Filter::ptype(ptype)).unwrap(),
                json!({ "type": wire })
            );
        }
    }

    /// RFC 9670 §4.4 — the remaining filter constructors, each a
    /// single-member object under its own RFC name. `timezone` keeps the
    /// crate's spelling, not the RFC's `timeZone` (Finding #69).
    #[test]
    fn every_filter_condition_serialises_with_its_own_key() {
        let cases: Vec<(Filter, serde_json::Value)> = vec![
            (
                Filter::email("joe@example.com"),
                json!({ "email": "joe@example.com" }),
            ),
            (Filter::name("Joe Bloggs"), json!({ "name": "Joe Bloggs" })),
            (
                Filter::domain_name("example.com"),
                json!({ "domainName": "example.com" }),
            ),
            (Filter::text("bloggs"), json!({ "text": "bloggs" })),
            (
                Filter::timezone("Europe/Berlin"),
                json!({ "timezone": "Europe/Berlin" }),
            ),
            (Filter::members("p1234"), json!({ "members": "p1234" })),
            (
                Filter::quota_lower_than(1024),
                json!({ "quotaLowerThan": 1024 }),
            ),
            (
                Filter::quota_greater_than(4_194_304),
                json!({ "quotaGreaterThan": 4_194_304 }),
            ),
            (Filter::ptype(Type::List), json!({ "type": "list" })),
        ];

        assert_eq!(cases.len(), 9, "every `PrincipalFilter` variant");
        for (filter, expected) in cases {
            let serialised = serde_json::to_value(&filter).unwrap();
            assert_eq!(
                serialised, expected,
                "{filter:?} must serialise as {expected}"
            );
            assert_eq!(
                serialised.as_object().unwrap().len(),
                1,
                "a filter condition carries exactly one property"
            );
        }
    }

    /// RFC 9670 §4.4 / RFC 8620 §5.5 — a whole `Principal/query`: the
    /// filter tree, the `ptype` comparator and the window arguments.
    /// Principal has no per-type query extension, so nothing is
    /// flattened onto the request.
    #[test]
    fn a_principal_query_serialises_the_rfc_9670_shape() {
        let mut request = QueryRequest::<Principal<Set>>::new(RequestParams::new(
            "u33084183",
            Method::QueryPrincipal,
            4,
        ));
        let domain: query::Filter<Filter> = Filter::domain_name("example.com").into();
        let groups: query::Filter<Filter> = Filter::ptype(Type::Group).into();
        request
            .filter(query::Filter::and([domain, groups]))
            .sort([Comparator::ptype(), Comparator::name().descending()])
            .anchor("p4242")
            .anchor_offset(-1)
            .limit(50);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "filter": {
                    "operator": "AND",
                    "conditions": [
                        { "domainName": "example.com" },
                        { "type": "group" }
                    ]
                },
                "sort": [
                    { "isAscending": true, "property": "type" },
                    { "isAscending": false, "property": "name" }
                ],
                "anchor": "p4242",
                "anchorOffset": -1,
                "limit": 50
            })
        );
    }
}
