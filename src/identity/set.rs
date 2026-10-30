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

use crate::{core::set::SetObject, email::EmailAddress, Get, Set};

use super::Identity;

impl Identity<Set> {
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn email(&mut self, email: impl Into<String>) -> &mut Self {
        self.email = Some(email.into());
        self
    }

    pub fn bcc<T, U>(&mut self, bcc: Option<T>) -> &mut Self
    where
        T: Iterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.bcc = bcc.map(|s| s.map(|s| s.into()).collect());
        self
    }

    pub fn reply_to<T, U>(&mut self, reply_to: Option<T>) -> &mut Self
    where
        T: Iterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.reply_to = reply_to.map(|s| s.map(|s| s.into()).collect());
        self
    }

    pub fn text_signature(&mut self, text_signature: impl Into<String>) -> &mut Self {
        self.text_signature = Some(text_signature.into());
        self
    }

    pub fn html_signature(&mut self, html_signature: impl Into<String>) -> &mut Self {
        self.html_signature = Some(html_signature.into());
        self
    }
}

impl SetObject for Identity<Set> {
    type SetArguments = ();

    fn new(_create_id: Option<usize>) -> Self {
        Identity {
            _create_id,
            _state: Default::default(),
            id: None,
            name: None,
            email: None,
            reply_to: Vec::with_capacity(0).into(),
            bcc: Vec::with_capacity(0).into(),
            text_signature: None,
            html_signature: None,
            may_delete: None,
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetObject for Identity<Get> {
    type SetArguments = ();

    fn new(_create_id: Option<usize>) -> Self {
        unimplemented!()
    }

    fn create_id(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `<Get>` arm of `SetObject` on `Identity`
    //! (RFC 8621 §2). A fetched record is not a patch, and the trait
    //! arm encodes that: it can never mint a creation id, and it refuses
    //! to construct itself.

    use super::*;
    use serde_json::json;

    /// RFC 8621 §2 — a fetched identity has no creation id. The generic
    /// `SetRequest` machinery uses this to decide that the object cannot
    /// take part in `create`, and the answer must be `None` rather than a
    /// bare counter that would serialise as a bogus `c0` key.
    #[test]
    fn a_fetched_identity_has_no_creation_id() {
        let identity: Identity<Get> = serde_json::from_value(json!({
            "id": "I1",
            "name": "Philip Jagger"
        }))
        .unwrap();

        assert_eq!(identity.create_id(), None);
    }

    /// RFC 8621 §2 — asking a `<Get>` flavour to construct itself is a
    /// type-level mistake, and the arm says so loudly rather than handing
    /// back an empty patch that would silently serialise to `{}`.
    #[test]
    #[should_panic(expected = "not implemented")]
    fn a_fetched_identity_refuses_to_construct_itself() {
        let _ = <Identity<Get> as SetObject>::new(Some(0));
    }
}
