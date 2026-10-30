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

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    core::{
        query::{self, QueryObject},
        set::from_timestamp,
    },
    Set,
};

use super::{Email, QueryArguments};

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Filter {
    InMailbox {
        #[serde(rename = "inMailbox")]
        value: String,
    },
    InMailboxOtherThan {
        #[serde(rename = "inMailboxOtherThan")]
        value: Vec<String>,
    },
    Before {
        #[serde(rename = "before")]
        value: DateTime<Utc>,
    },
    After {
        #[serde(rename = "after")]
        value: DateTime<Utc>,
    },
    MinSize {
        #[serde(rename = "minSize")]
        value: u32,
    },
    MaxSize {
        #[serde(rename = "maxSize")]
        value: u32,
    },
    AllInThreadHaveKeyword {
        #[serde(rename = "allInThreadHaveKeyword")]
        value: String,
    },
    SomeInThreadHaveKeyword {
        #[serde(rename = "someInThreadHaveKeyword")]
        value: String,
    },
    NoneInThreadHaveKeyword {
        #[serde(rename = "noneInThreadHaveKeyword")]
        value: String,
    },
    HasKeyword {
        #[serde(rename = "hasKeyword")]
        value: String,
    },
    NotKeyword {
        #[serde(rename = "notKeyword")]
        value: String,
    },
    HasAttachment {
        #[serde(rename = "hasAttachment")]
        value: bool,
    },
    Text {
        #[serde(rename = "text")]
        value: String,
    },
    From {
        #[serde(rename = "from")]
        value: String,
    },
    To {
        #[serde(rename = "to")]
        value: String,
    },
    Cc {
        #[serde(rename = "cc")]
        value: String,
    },
    Bcc {
        #[serde(rename = "bcc")]
        value: String,
    },
    Subject {
        #[serde(rename = "subject")]
        value: String,
    },
    Body {
        #[serde(rename = "body")]
        value: String,
    },
    Header {
        #[serde(rename = "header")]
        value: Vec<String>,
    },

    // Non-standard
    Id {
        #[serde(rename = "id")]
        value: Vec<String>,
    },
    SentBefore {
        #[serde(rename = "sentBefore")]
        value: DateTime<Utc>,
    },
    SentAfter {
        #[serde(rename = "sentAfter")]
        value: DateTime<Utc>,
    },
    InThread {
        #[serde(rename = "inThread")]
        value: String,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "property")]
pub enum Comparator {
    #[serde(rename = "receivedAt")]
    ReceivedAt,
    #[serde(rename = "size")]
    Size,
    #[serde(rename = "from")]
    From,
    #[serde(rename = "to")]
    To,
    #[serde(rename = "subject")]
    Subject,
    #[serde(rename = "sentAt")]
    SentAt,
    #[serde(rename = "hasKeyword")]
    HasKeyword { keyword: String },
    #[serde(rename = "allInThreadHaveKeyword")]
    AllInThreadHaveKeyword { keyword: String },
    #[serde(rename = "someInThreadHaveKeyword")]
    SomeInThreadHaveKeyword { keyword: String },

    // Non-standard
    #[serde(rename = "cc")]
    Cc,
}

impl Filter {
    pub fn in_mailbox(value: impl Into<String>) -> Self {
        Filter::InMailbox {
            value: value.into(),
        }
    }

    pub fn in_mailbox_other_than<U, V>(value: U) -> Self
    where
        U: IntoIterator<Item = V>,
        V: Into<String>,
    {
        Filter::InMailboxOtherThan {
            value: value.into_iter().map(|v| v.into()).collect(),
        }
    }

    pub fn before(value: i64) -> Self {
        Filter::Before {
            value: from_timestamp(value),
        }
    }

    pub fn after(value: i64) -> Self {
        Filter::After {
            value: from_timestamp(value),
        }
    }

    pub fn min_size(value: u32) -> Self {
        Filter::MinSize { value }
    }

    pub fn max_size(value: u32) -> Self {
        Filter::MaxSize { value }
    }

    pub fn all_in_thread_have_keyword(value: impl Into<String>) -> Self {
        Filter::AllInThreadHaveKeyword {
            value: value.into(),
        }
    }

    pub fn some_in_thread_have_keyword(value: impl Into<String>) -> Self {
        Filter::SomeInThreadHaveKeyword {
            value: value.into(),
        }
    }

    pub fn none_in_thread_have_keyword(value: impl Into<String>) -> Self {
        Filter::NoneInThreadHaveKeyword {
            value: value.into(),
        }
    }

    pub fn has_keyword(value: impl Into<String>) -> Self {
        Filter::HasKeyword {
            value: value.into(),
        }
    }

    pub fn not_keyword(value: impl Into<String>) -> Self {
        Filter::NotKeyword {
            value: value.into(),
        }
    }

    pub fn has_attachment(value: bool) -> Self {
        Filter::HasAttachment { value }
    }

    pub fn text(value: impl Into<String>) -> Self {
        Filter::Text {
            value: value.into(),
        }
    }

    pub fn from(value: impl Into<String>) -> Self {
        Filter::From {
            value: value.into(),
        }
    }

    pub fn to(value: impl Into<String>) -> Self {
        Filter::To {
            value: value.into(),
        }
    }

    pub fn cc(value: impl Into<String>) -> Self {
        Filter::Cc {
            value: value.into(),
        }
    }

    pub fn bcc(value: impl Into<String>) -> Self {
        Filter::Bcc {
            value: value.into(),
        }
    }

    pub fn subject(value: impl Into<String>) -> Self {
        Filter::Subject {
            value: value.into(),
        }
    }

    pub fn body(value: impl Into<String>) -> Self {
        Filter::Body {
            value: value.into(),
        }
    }

    pub fn header(header: impl Into<String>, v: Option<impl Into<String>>) -> Self {
        let mut value = Vec::with_capacity(2);
        value.push(header.into());
        if let Some(v) = v {
            value.push(v.into());
        }
        Filter::Header { value }
    }

    // Stalwart JMAP specific
    pub fn id<U, V>(value: U) -> Self
    where
        U: IntoIterator<Item = V>,
        V: Into<String>,
    {
        Filter::Id {
            value: value.into_iter().map(|v| v.into()).collect(),
        }
    }

    pub fn sent_before(value: i64) -> Self {
        Filter::SentBefore {
            value: from_timestamp(value),
        }
    }

    pub fn sent_after(value: i64) -> Self {
        Filter::SentAfter {
            value: from_timestamp(value),
        }
    }

    pub fn in_thread(value: impl Into<String>) -> Self {
        Filter::InThread {
            value: value.into(),
        }
    }
}

impl Comparator {
    pub fn received_at() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::ReceivedAt)
    }

    pub fn size() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Size)
    }

    pub fn from() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::From)
    }

    pub fn to() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::To)
    }

    pub fn cc() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Cc)
    }

    pub fn subject() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Subject)
    }

    pub fn sent_at() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::SentAt)
    }

    pub fn has_keyword(keyword: impl Into<String>) -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::HasKeyword {
            keyword: keyword.into(),
        })
    }

    pub fn all_in_thread_have_keyword(keyword: impl Into<String>) -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::AllInThreadHaveKeyword {
            keyword: keyword.into(),
        })
    }

    pub fn some_in_thread_have_keyword(
        keyword: impl Into<String>,
    ) -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::SomeInThreadHaveKeyword {
            keyword: keyword.into(),
        })
    }
}

impl QueryObject for Email<Set> {
    type QueryArguments = QueryArguments;

    type Filter = Filter;

    type Sort = Comparator;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// RFC 8621 §4.4.1 — every `FilterCondition` property is a
    /// single-member object keyed by the RFC's camelCase name. The
    /// non-standard Stalwart extensions are included so a rename cannot
    /// slip through unnoticed.
    #[test]
    fn every_filter_condition_serialises_with_its_own_key() {
        let cases: Vec<(Filter, serde_json::Value)> = vec![
            (
                Filter::in_mailbox("MB23cfa8094c0f41e6"),
                json!({ "inMailbox": "MB23cfa8094c0f41e6" }),
            ),
            (
                Filter::in_mailbox_other_than(["MB23cfa8094c0f41e6", "MB674cc24095db49ce"]),
                json!({ "inMailboxOtherThan": ["MB23cfa8094c0f41e6", "MB674cc24095db49ce"] }),
            ),
            (
                Filter::before(1_720_000_000),
                json!({ "before": "2024-07-03T09:46:40Z" }),
            ),
            (
                Filter::after(1_704_067_200),
                json!({ "after": "2024-01-01T00:00:00Z" }),
            ),
            (Filter::min_size(1024), json!({ "minSize": 1024 })),
            (Filter::max_size(4_194_304), json!({ "maxSize": 4_194_304 })),
            (
                Filter::all_in_thread_have_keyword("$seen"),
                json!({ "allInThreadHaveKeyword": "$seen" }),
            ),
            (
                Filter::some_in_thread_have_keyword("$flagged"),
                json!({ "someInThreadHaveKeyword": "$flagged" }),
            ),
            (
                Filter::none_in_thread_have_keyword("$junk"),
                json!({ "noneInThreadHaveKeyword": "$junk" }),
            ),
            (
                Filter::has_keyword("$draft"),
                json!({ "hasKeyword": "$draft" }),
            ),
            (
                Filter::not_keyword("$junk"),
                json!({ "notKeyword": "$junk" }),
            ),
            (
                Filter::has_attachment(true),
                json!({ "hasAttachment": true }),
            ),
            (
                Filter::has_attachment(false),
                json!({ "hasAttachment": false }),
            ),
            (
                Filter::text("quarterly report"),
                json!({ "text": "quarterly report" }),
            ),
            (
                Filter::from("joe@example.com"),
                json!({ "from": "joe@example.com" }),
            ),
            (
                Filter::to("alice@example.com"),
                json!({ "to": "alice@example.com" }),
            ),
            (
                Filter::cc("bob@example.com"),
                json!({ "cc": "bob@example.com" }),
            ),
            (
                Filter::bcc("carol@example.com"),
                json!({ "bcc": "carol@example.com" }),
            ),
            (
                Filter::subject("Team lunch"),
                json!({ "subject": "Team lunch" }),
            ),
            (
                Filter::body("please review"),
                json!({ "body": "please review" }),
            ),
            // Non-standard, Stalwart-specific extensions
            (
                Filter::id(["M7f6ed5bcfd7e2604d1753f6c"]),
                json!({ "id": ["M7f6ed5bcfd7e2604d1753f6c"] }),
            ),
            (
                Filter::sent_before(1_720_000_000),
                json!({ "sentBefore": "2024-07-03T09:46:40Z" }),
            ),
            (
                Filter::sent_after(1_704_067_200),
                json!({ "sentAfter": "2024-01-01T00:00:00Z" }),
            ),
            (
                Filter::in_thread("Tf14a5b2c9d3e0a7b6c5d4e3f2"),
                json!({ "inThread": "Tf14a5b2c9d3e0a7b6c5d4e3f2" }),
            ),
        ];

        // 20 RFC conditions + 4 Stalwart extensions
        assert_eq!(cases.len(), 24);
        for (filter, expected) in cases {
            let serialised = serde_json::to_value(&filter).unwrap();
            assert_eq!(
                serialised, expected,
                "{filter:?} must serialise as {expected}"
            );
            assert_eq!(
                serialised.as_object().unwrap().len(),
                1,
                "a FilterCondition carries exactly one property"
            );
        }
    }

    /// RFC 8621 §4.4.1 — `header` is a two-element array when a value is
    /// given and a one-element array when only the header name is tested.
    #[test]
    fn header_filter_arity_tracks_the_optional_value() {
        assert_eq!(
            serde_json::to_value(Filter::header("In-Reply-To", Some("<abc@example.com>"))).unwrap(),
            json!({ "header": ["In-Reply-To", "<abc@example.com>"] })
        );
        assert_eq!(
            serde_json::to_value(Filter::header("X-Spam-Result", None::<&str>)).unwrap(),
            json!({ "header": ["X-Spam-Result"] })
        );
    }

    /// RFC 8621 §4.4.2 — the Comparator `property` values the spec names,
    /// each wrapped in the generic comparator envelope that defaults
    /// `isAscending` to true and omits `collation`.
    #[test]
    fn comparator_property_names_match_the_rfc() {
        let cases: Vec<(query::Comparator<Comparator>, serde_json::Value)> = vec![
            (
                Comparator::received_at(),
                json!({ "isAscending": true, "property": "receivedAt" }),
            ),
            (
                Comparator::size(),
                json!({ "isAscending": true, "property": "size" }),
            ),
            (
                Comparator::from(),
                json!({ "isAscending": true, "property": "from" }),
            ),
            (
                Comparator::to(),
                json!({ "isAscending": true, "property": "to" }),
            ),
            (
                Comparator::subject(),
                json!({ "isAscending": true, "property": "subject" }),
            ),
            (
                Comparator::sent_at(),
                json!({ "isAscending": true, "property": "sentAt" }),
            ),
            (
                Comparator::cc(),
                json!({ "isAscending": true, "property": "cc" }),
            ),
        ];

        for (comparator, expected) in cases {
            assert_eq!(serde_json::to_value(&comparator).unwrap(), expected);
        }
    }

    /// RFC 8621 §4.4.2 — the three keyword comparators MUST also carry a
    /// `keyword` member on the Comparator object.
    #[test]
    fn keyword_comparators_carry_their_keyword() {
        assert_eq!(
            serde_json::to_value(Comparator::has_keyword("$flagged")).unwrap(),
            json!({ "isAscending": true, "property": "hasKeyword", "keyword": "$flagged" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::all_in_thread_have_keyword("$seen")).unwrap(),
            json!({
                "isAscending": true,
                "property": "allInThreadHaveKeyword",
                "keyword": "$seen"
            })
        );
        assert_eq!(
            serde_json::to_value(Comparator::some_in_thread_have_keyword("$answered")).unwrap(),
            json!({
                "isAscending": true,
                "property": "someInThreadHaveKeyword",
                "keyword": "$answered"
            })
        );
    }

    /// RFC 8621 §4.4.2 example sort — `isAscending` is flipped and the
    /// optional `collation` member (RFC 8620 §5.5.1) is emitted only when
    /// it has been set.
    #[test]
    fn comparator_ordering_and_collation_are_configurable() {
        assert_eq!(
            serde_json::to_value(Comparator::some_in_thread_have_keyword("$flagged").descending())
                .unwrap(),
            json!({
                "isAscending": false,
                "property": "someInThreadHaveKeyword",
                "keyword": "$flagged"
            })
        );
        assert_eq!(
            serde_json::to_value(Comparator::subject().descending().ascending()).unwrap(),
            json!({ "isAscending": true, "property": "subject" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::from().is_ascending(false)).unwrap(),
            json!({ "isAscending": false, "property": "from" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::subject().collation("i;unicode-casemap".to_string()))
                .unwrap(),
            json!({
                "isAscending": true,
                "collation": "i;unicode-casemap",
                "property": "subject"
            })
        );
    }
}
