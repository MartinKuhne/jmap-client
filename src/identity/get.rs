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

use crate::{core::get::GetObject, email::EmailAddress, Get, Set};

use super::Identity;

impl Identity<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn reply_to(&self) -> Option<&[EmailAddress]> {
        self.reply_to.as_deref()
    }

    pub fn bcc(&self) -> Option<&[EmailAddress]> {
        self.bcc.as_deref()
    }

    pub fn text_signature(&self) -> Option<&str> {
        self.text_signature.as_deref()
    }

    pub fn html_signature(&self) -> Option<&str> {
        self.html_signature.as_deref()
    }

    pub fn may_delete(&self) -> bool {
        self.may_delete.unwrap_or(false)
    }
}

impl GetObject for Identity<Set> {
    type GetArguments = ();
}

impl GetObject for Identity<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Identity<Get>` identifier accessors
    //! (RFC 8621 §6.1).

    use super::*;
    use serde_json::json;

    /// RFC 8621 §6.1 — `take_id` moves the identifier out and leaves an
    /// empty string behind, so the second call yields `""` rather than
    /// the id it already gave up.
    #[test]
    fn take_id_drains_the_identifier() {
        let mut identity: Identity<Get> = serde_json::from_value(json!({
            "id": "id1",
            "name": "Joe Bloggs",
            "email": "joe@bloggs.com",
            "textSignature": "-- Joe Bloggs",
            "mayDelete": true
        }))
        .unwrap();

        assert_eq!(identity.id(), Some("id1"));
        assert_eq!(identity.take_id(), "id1");
        assert_eq!(identity.id(), None, "the field has been moved out of");
        assert_eq!(
            identity.take_id(),
            String::new(),
            "and there is nothing left"
        );
        assert!(identity.may_delete());
    }

    /// A record with no `id` member takes out the empty string, which is
    /// how the crate spells "this identity was never identified" without
    /// an `Option` at the call site.
    #[test]
    fn take_id_on_an_unidentified_record_yields_the_empty_string() {
        let mut identity: Identity<Get> = serde_json::from_value(json!({
            "name": "Wildcard",
            "email": "*@example.org"
        }))
        .unwrap();

        assert_eq!(identity.id(), None);
        assert_eq!(identity.take_id(), String::new());
        assert!(
            !identity.may_delete(),
            "a missing `mayDelete` reads as false"
        );
    }
}
