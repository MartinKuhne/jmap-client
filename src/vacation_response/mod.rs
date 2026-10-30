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

pub mod get;
pub mod helpers;
pub mod set;

use std::fmt::Display;

use crate::core::changes::ChangesObject;
use crate::core::set::date_not_set;
use crate::core::set::string_not_set;
use crate::core::Object;
use crate::Get;
use crate::Set;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacationResponse<State = Get> {
    #[serde(skip)]
    _create_id: Option<usize>,

    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,

    #[serde(rename = "isEnabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    is_enabled: Option<bool>,

    #[serde(rename = "fromDate")]
    #[serde(skip_serializing_if = "date_not_set")]
    from_date: Option<DateTime<Utc>>,

    #[serde(rename = "toDate")]
    #[serde(skip_serializing_if = "date_not_set")]
    to_date: Option<DateTime<Utc>>,

    #[serde(rename = "subject")]
    #[serde(skip_serializing_if = "string_not_set")]
    subject: Option<String>,

    #[serde(rename = "textBody")]
    #[serde(skip_serializing_if = "string_not_set")]
    text_body: Option<String>,

    #[serde(rename = "htmlBody")]
    #[serde(skip_serializing_if = "string_not_set")]
    html_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum Property {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "isEnabled")]
    IsEnabled,
    #[serde(rename = "fromDate")]
    FromDate,
    #[serde(rename = "toDate")]
    ToDate,
    #[serde(rename = "subject")]
    Subject,
    #[serde(rename = "textBody")]
    TextBody,
    #[serde(rename = "htmlBody")]
    HtmlBody,
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::IsEnabled => write!(f, "isEnabled"),
            Property::FromDate => write!(f, "fromDate"),
            Property::ToDate => write!(f, "toDate"),
            Property::Subject => write!(f, "subject"),
            Property::TextBody => write!(f, "textBody"),
            Property::HtmlBody => write!(f, "htmlBody"),
        }
    }
}

impl Object for VacationResponse<Set> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl Object for VacationResponse<Get> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl ChangesObject for VacationResponse<Set> {
    type ChangesResponse = ();
}

impl ChangesObject for VacationResponse<Get> {
    type ChangesResponse = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure wire-name table of the vacation-response
    //! `Property` enum and for the `Object` account-scoping flag
    //! (RFC 8621 §8.1, RFC 8620 §3.1).

    use super::*;
    use serde_json::json;

    /// RFC 8621 §8.1 — `Display` and `Serialize` must agree on the wire
    /// name of every variant, and reading that name back must land on the
    /// same variant.
    #[test]
    fn property_display_and_serde_round_trip_every_variant() {
        let cases: &[(Property, &str)] = &[
            (Property::Id, "id"),
            (Property::IsEnabled, "isEnabled"),
            (Property::FromDate, "fromDate"),
            (Property::ToDate, "toDate"),
            (Property::Subject, "subject"),
            (Property::TextBody, "textBody"),
            (Property::HtmlBody, "htmlBody"),
        ];

        assert_eq!(cases.len(), 7, "seven vacation-response properties");
        for (property, wire) in cases {
            assert_eq!(property.to_string(), *wire, "{property:?} Display");
            assert_eq!(serde_json::to_value(property).unwrap(), json!(*wire));
            assert_eq!(
                serde_json::from_value::<Property>(json!(*wire)).unwrap(),
                *property,
                "{wire} must round-trip back to the variant it came from"
            );
        }
    }

    /// RFC 8621 §8 — the vacation response is a per-account singleton,
    /// so it is account scoped whichever flavour the caller holds.
    #[test]
    fn both_vacation_response_flavours_require_an_account_id() {
        assert!(<VacationResponse<Set> as Object>::requires_account_id());
        assert!(<VacationResponse<Get> as Object>::requires_account_id());
    }
}
