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
pub mod query;
pub mod set;

use ahash::AHashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::{
    core::{changes::ChangesObject, Object},
    email::Email,
    Get, Set,
};

#[derive(Debug, Clone, Serialize, Default)]
pub struct SetArguments {
    #[serde(rename = "onSuccessUpdateEmail")]
    #[serde(skip_serializing_if = "Option::is_none")]
    on_success_update_email: Option<AHashMap<String, Email<Set>>>,
    #[serde(rename = "onSuccessDestroyEmail")]
    #[serde(skip_serializing_if = "Option::is_none")]
    on_success_destroy_email: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSubmission<State = Get> {
    #[serde(skip)]
    _create_id: Option<usize>,

    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,

    #[serde(rename = "identityId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_id: Option<String>,

    #[serde(rename = "emailId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    email_id: Option<String>,

    #[serde(rename = "threadId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,

    #[serde(rename = "envelope")]
    #[serde(skip_serializing_if = "Option::is_none")]
    envelope: Option<Envelope>,

    #[serde(rename = "sendAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    send_at: Option<DateTime<Utc>>,

    #[serde(rename = "undoStatus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    undo_status: Option<UndoStatus>,

    #[serde(rename = "deliveryStatus")]
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_status: Option<AHashMap<String, DeliveryStatus>>,

    #[serde(rename = "dsnBlobIds")]
    #[serde(skip_serializing_if = "Option::is_none")]
    dsn_blob_ids: Option<Vec<String>>,

    #[serde(rename = "mdnBlobIds")]
    #[serde(skip_serializing_if = "Option::is_none")]
    mdn_blob_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(rename = "mailFrom")]
    mail_from: Address,

    #[serde(rename = "rcptTo")]
    rcpt_to: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    email: String,
    parameters: Option<AHashMap<String, Option<String>>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UndoStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "final")]
    Final,
    #[serde(rename = "canceled")]
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryStatus {
    #[serde(rename = "smtpReply")]
    smtp_reply: String,

    #[serde(rename = "delivered")]
    delivered: Delivered,

    #[serde(rename = "displayed")]
    displayed: Displayed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Delivered {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No,
    #[serde(rename = "unknown")]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Displayed {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "yes")]
    Yes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum Property {
    #[serde(rename = "id")]
    Id,
    #[serde(rename = "identityId")]
    IdentityId,
    #[serde(rename = "emailId")]
    EmailId,
    #[serde(rename = "threadId")]
    ThreadId,
    #[serde(rename = "envelope")]
    Envelope,
    #[serde(rename = "sendAt")]
    SendAt,
    #[serde(rename = "undoStatus")]
    UndoStatus,
    #[serde(rename = "deliveryStatus")]
    DeliveryStatus,
    #[serde(rename = "dsnBlobIds")]
    DsnBlobIds,
    #[serde(rename = "mdnBlobIds")]
    MdnBlobIds,
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::IdentityId => write!(f, "identityId"),
            Property::EmailId => write!(f, "emailId"),
            Property::ThreadId => write!(f, "threadId"),
            Property::Envelope => write!(f, "envelope"),
            Property::SendAt => write!(f, "sendAt"),
            Property::UndoStatus => write!(f, "undoStatus"),
            Property::DeliveryStatus => write!(f, "deliveryStatus"),
            Property::DsnBlobIds => write!(f, "dsnBlobIds"),
            Property::MdnBlobIds => write!(f, "mdnBlobIds"),
        }
    }
}

impl Object for EmailSubmission<Set> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl Object for EmailSubmission<Get> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl ChangesObject for EmailSubmission<Set> {
    type ChangesResponse = ();
}

impl ChangesObject for EmailSubmission<Get> {
    type ChangesResponse = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure wire-name table of the submission
    //! `Property` enum and for the `Object` account-scoping flag
    //! (RFC 8621 §7.1, RFC 8620 §3.1).

    use super::*;
    use serde_json::json;

    /// RFC 8621 §7.1 — `Display` and `Serialize` must agree on the wire
    /// name of every variant, and reading that name back must land on the
    /// same variant.
    #[test]
    fn property_display_and_serde_round_trip_every_variant() {
        let cases: &[(Property, &str)] = &[
            (Property::Id, "id"),
            (Property::IdentityId, "identityId"),
            (Property::EmailId, "emailId"),
            (Property::ThreadId, "threadId"),
            (Property::Envelope, "envelope"),
            (Property::SendAt, "sendAt"),
            (Property::UndoStatus, "undoStatus"),
            (Property::DeliveryStatus, "deliveryStatus"),
            (Property::DsnBlobIds, "dsnBlobIds"),
            (Property::MdnBlobIds, "mdnBlobIds"),
        ];

        assert_eq!(cases.len(), 10, "ten submission properties");
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

    /// RFC 8620 §3.1 — `requires_account_id` is a property of the *type*,
    /// not of the operation, so a submission read through `<Get>` is
    /// scoped to an account exactly the way one written through `<Set>`
    /// is. Both arms must report `true`, or the request builders would
    /// disagree about whether to put `accountId` on the wire.
    #[test]
    fn both_submission_flavours_require_an_account_id() {
        assert!(<EmailSubmission<Set> as Object>::requires_account_id());
        assert!(<EmailSubmission<Get> as Object>::requires_account_id());
    }
}
