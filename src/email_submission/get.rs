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

use super::{Address, Delivered, DeliveryStatus, Displayed, EmailSubmission, UndoStatus};
use crate::{core::get::GetObject, Get, Set};
use ahash::AHashMap;

impl EmailSubmission<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn identity_id(&self) -> Option<&str> {
        self.identity_id.as_deref()
    }

    pub fn email_id(&self) -> Option<&str> {
        self.email_id.as_deref()
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub fn mail_from(&self) -> Option<&Address> {
        self.envelope.as_ref().map(|e| &e.mail_from)
    }

    pub fn rcpt_to(&self) -> Option<&[Address]> {
        self.envelope.as_ref().map(|e| e.rcpt_to.as_ref())
    }

    pub fn send_at(&self) -> Option<i64> {
        self.send_at.as_ref().map(|t| t.timestamp())
    }

    pub fn undo_status(&self) -> Option<&UndoStatus> {
        self.undo_status.as_ref()
    }

    pub fn delivery_status_email(&self, email: &str) -> Option<&DeliveryStatus> {
        self.delivery_status.as_ref().and_then(|ds| ds.get(email))
    }

    pub fn delivery_status(&self) -> Option<&AHashMap<String, DeliveryStatus>> {
        self.delivery_status.as_ref()
    }

    pub fn dsn_blob_ids(&self) -> Option<&[String]> {
        self.dsn_blob_ids.as_deref()
    }

    pub fn mdn_blob_ids(&self) -> Option<&[String]> {
        self.mdn_blob_ids.as_deref()
    }
}

impl Address<Get> {
    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn parameter(&self, param: &str) -> Option<&str> {
        self.parameters.as_ref()?.get(param)?.as_deref()
    }

    pub fn has_parameter(&self, param: &str) -> bool {
        self.parameters
            .as_ref()
            .map(|ps| ps.contains_key(param))
            .unwrap_or(false)
    }
}

impl DeliveryStatus {
    #[cfg(feature = "debug")]
    pub fn new(smtp_reply: impl Into<String>, delivered: Delivered, displayed: Displayed) -> Self {
        Self {
            smtp_reply: smtp_reply.into(),
            delivered,
            displayed,
        }
    }

    pub fn smtp_reply(&self) -> &str {
        &self.smtp_reply
    }

    pub fn delivered(&self) -> &Delivered {
        &self.delivered
    }

    pub fn displayed(&self) -> &Displayed {
        &self.displayed
    }
}

impl GetObject for EmailSubmission<Set> {
    type GetArguments = ();
}

impl GetObject for EmailSubmission<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `EmailSubmission<Get>` id accessors against
    //! RFC 8621 §4.4 (Email Submission objects). No server, no socket,
    //! no async.

    use super::*;
    use serde_json::json;

    fn submission(value: serde_json::Value) -> EmailSubmission<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// RFC 8621 §4.4 — `id` borrows the submission id and `take_id`
    /// moves it out, leaving the record without one.
    #[test]
    fn take_id_moves_the_id_out() {
        let mut submission = submission(json!({
            "id": "S1",
            "identityId": "I1",
            "emailId": "M1"
        }));

        assert_eq!(submission.id(), Some("S1"));
        assert_eq!(submission.take_id(), "S1");
        assert_eq!(
            submission.id(),
            None,
            "taking the id must leave the record without one"
        );
        assert_eq!(
            submission.identity_id(),
            Some("I1"),
            "taking the id must not disturb the other members"
        );
    }

    /// RFC 8621 §4.4 — a record that never had an id yields the empty
    /// string rather than panicking: the accessor is total.
    #[test]
    fn take_id_on_a_record_without_one_is_empty() {
        let mut submission = submission(json!({ "undoStatus": "final" }));

        assert_eq!(submission.id(), None);
        assert_eq!(submission.take_id(), "");
    }
}
