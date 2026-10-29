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
use crate::core::set::list_not_set;
use crate::core::Object;
use crate::Set;
use crate::{email::EmailAddress, Get};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity<State = Get> {
    #[serde(skip)]
    _create_id: Option<usize>,

    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(rename = "email")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(rename = "replyTo")]
    #[serde(skip_serializing_if = "list_not_set")]
    pub reply_to: Option<Vec<EmailAddress>>,

    #[serde(rename = "bcc")]
    #[serde(skip_serializing_if = "list_not_set")]
    pub bcc: Option<Vec<EmailAddress>>,

    #[serde(rename = "textSignature")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,

    #[serde(rename = "htmlSignature")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_signature: Option<String>,

    #[serde(rename = "mayDelete")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub may_delete: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum Property {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "replyTo")]
    ReplyTo,
    #[serde(rename = "bcc")]
    Bcc,
    #[serde(rename = "textSignature")]
    TextSignature,
    #[serde(rename = "htmlSignature")]
    HtmlSignature,
    #[serde(rename = "mayDelete")]
    MayDelete,
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::Name => write!(f, "name"),
            Property::Email => write!(f, "email"),
            Property::ReplyTo => write!(f, "replyTo"),
            Property::Bcc => write!(f, "bcc"),
            Property::TextSignature => write!(f, "textSignature"),
            Property::HtmlSignature => write!(f, "htmlSignature"),
            Property::MayDelete => write!(f, "mayDelete"),
        }
    }
}

impl Object for Identity<Set> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl Object for Identity<Get> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl ChangesObject for Identity<Set> {
    type ChangesResponse = ();
}

impl ChangesObject for Identity<Get> {
    type ChangesResponse = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure wire-name table of the identity
    //! `Property` enum and for the `Object` account-scoping flag
    //! (RFC 8621 §6.1, RFC 8620 §3.1).

    use super::*;
    use serde_json::json;

    /// RFC 8621 §6.1 — `Display` and `Serialize` must agree on the wire
    /// name of every variant, and reading that name back must land on the
    /// same variant.
    #[test]
    fn property_display_and_serde_round_trip_every_variant() {
        let cases: &[(Property, &str)] = &[
            (Property::Id, "id"),
            (Property::Name, "name"),
            (Property::Email, "email"),
            (Property::ReplyTo, "replyTo"),
            (Property::Bcc, "bcc"),
            (Property::TextSignature, "textSignature"),
            (Property::HtmlSignature, "htmlSignature"),
            (Property::MayDelete, "mayDelete"),
        ];

        assert_eq!(cases.len(), 8, "eight identity properties");
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

    /// RFC 8621 §6 — identities live in an account, and the flag is
    /// carried by the type rather than the call, so `<Get>` and `<Set>`
    /// must both report `true`.
    #[test]
    fn both_identity_flavours_require_an_account_id() {
        assert!(<Identity<Set> as Object>::requires_account_id());
        assert!(<Identity<Get> as Object>::requires_account_id());
    }
}
