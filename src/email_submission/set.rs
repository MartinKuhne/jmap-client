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

use super::{Address, EmailSubmission, Envelope, SetArguments, UndoStatus};
use crate::{core::set::SetObject, email::Email, Get, Set};
use ahash::AHashMap;

impl EmailSubmission<Set> {
    pub fn identity_id(&mut self, identity_id: impl Into<String>) -> &mut Self {
        self.identity_id = Some(identity_id.into());
        self
    }

    pub fn email_id(&mut self, email_id: impl Into<String>) -> &mut Self {
        self.email_id = Some(email_id.into());
        self
    }

    pub fn envelope<S, T, U>(&mut self, mail_from: S, rcpt_to: T) -> &mut Self
    where
        S: Into<Address>,
        T: IntoIterator<Item = U>,
        U: Into<Address>,
    {
        self.envelope = Some(Envelope::new(mail_from, rcpt_to));
        self
    }

    pub fn undo_status(&mut self, undo_status: UndoStatus) -> &mut Self {
        self.undo_status = Some(undo_status);
        self
    }
}

impl SetObject for EmailSubmission<Set> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        EmailSubmission {
            _create_id,
            _state: Default::default(),
            id: None,
            identity_id: None,
            email_id: None,
            thread_id: None,
            envelope: None,
            send_at: None,
            undo_status: None,
            delivery_status: None,
            dsn_blob_ids: None,
            mdn_blob_ids: None,
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetObject for EmailSubmission<Get> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        unimplemented!()
    }

    fn create_id(&self) -> Option<String> {
        None
    }
}

impl Envelope {
    pub fn new<S, T, U>(mail_from: S, rcpt_to: T) -> Envelope
    where
        S: Into<Address>,
        T: IntoIterator<Item = U>,
        U: Into<Address>,
    {
        Envelope {
            mail_from: mail_from.into(),
            rcpt_to: rcpt_to.into_iter().map(|s| s.into()).collect(),
        }
    }
}

impl Address<Set> {
    pub fn new(email: impl Into<String>) -> Address<Set> {
        Address {
            _state: Default::default(),
            email: email.into(),
            parameters: None,
        }
    }

    pub fn parameter(
        mut self,
        parameter: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Self {
        self.parameters
            .get_or_insert_with(AHashMap::new)
            .insert(parameter.into(), value.map(|s| s.into()));
        self
    }
}

impl From<String> for Address {
    fn from(email: String) -> Self {
        Address {
            _state: Default::default(),
            email,
            parameters: None,
        }
    }
}

impl From<&str> for Address {
    fn from(email: &str) -> Self {
        Address {
            _state: Default::default(),
            email: email.to_string(),
            parameters: None,
        }
    }
}

impl From<Address<Set>> for Address<Get> {
    fn from(addr: Address<Set>) -> Self {
        Address {
            _state: Default::default(),
            email: addr.email,
            parameters: addr.parameters,
        }
    }
}

impl From<Address<Get>> for Address<Set> {
    fn from(addr: Address<Get>) -> Self {
        Address {
            _state: Default::default(),
            email: addr.email,
            parameters: addr.parameters,
        }
    }
}

impl SetArguments {
    pub fn on_success_update_email(&mut self, id: impl Into<String>) -> &mut Email<Set> {
        self.on_success_update_email_(format!("#{}", id.into()))
    }

    pub fn on_success_update_email_id(&mut self, id: impl Into<String>) -> &mut Email<Set> {
        self.on_success_update_email_(id)
    }

    fn on_success_update_email_(&mut self, id: impl Into<String>) -> &mut Email<Set> {
        let id = id.into();
        self.on_success_update_email
            .get_or_insert_with(AHashMap::new)
            .insert(id.clone(), Email::new(None));
        self.on_success_update_email
            .as_mut()
            .unwrap()
            .get_mut(&id)
            .unwrap()
    }

    pub fn on_success_destroy_email(&mut self, id: impl Into<String>) -> &mut Self {
        self.on_success_destroy_email
            .get_or_insert_with(Vec::new)
            .push(format!("#{}", id.into()));
        self
    }

    pub fn on_success_destroy_email_id(&mut self, id: impl Into<String>) -> &mut Self {
        self.on_success_destroy_email
            .get_or_insert_with(Vec::new)
            .push(id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `EmailSubmission<Set>` patch builders and the
    //! RFC 8621 §6.1 envelope/address shapes, including the two spellings
    //! of the `onSuccess*` back-references (`#c1` create-id reference vs
    //! a literal id). No server, no socket, no async.

    use super::*;
    use serde_json::json;

    fn fresh() -> EmailSubmission<Set> {
        EmailSubmission::<Set>::new(None)
    }

    /// An untouched submission patch contributes nothing to the request.
    #[test]
    fn a_fresh_submission_patch_serialises_as_an_empty_object() {
        assert_eq!(serde_json::to_value(fresh()).unwrap(), json!({}));
    }

    /// RFC 8621 §6.1 — the four buildable members land under their own
    /// wire names.
    #[test]
    fn submission_patch_builders_land_under_their_wire_names() {
        let mut submission = fresh();
        submission
            .identity_id("id-1")
            .email_id("#c1")
            .envelope(
                "sender@example.com",
                ["rcpt1@example.com", "rcpt2@example.com"],
            )
            .undo_status(UndoStatus::Pending);

        assert_eq!(
            serde_json::to_value(&submission).unwrap(),
            json!({
                "identityId": "id-1",
                "emailId": "#c1",
                "envelope": {
                    "mailFrom": { "email": "sender@example.com", "parameters": null },
                    "rcptTo": [
                        { "email": "rcpt1@example.com", "parameters": null },
                        { "email": "rcpt2@example.com", "parameters": null }
                    ]
                },
                "undoStatus": "pending"
            })
        );
    }

    /// RFC 8621 §6.1 — every `UndoStatus` round-trips through its wire name.
    #[test]
    fn undo_status_round_trips() {
        for (status, wire) in [
            (UndoStatus::Pending, "pending"),
            (UndoStatus::Final, "final"),
            (UndoStatus::Canceled, "canceled"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<UndoStatus>(json!(wire)).unwrap(),
                status
            );
        }
    }

    /// `Address` has no `skip_serializing_if` on `parameters`, so the
    /// member is always present on the wire even when empty.
    #[test]
    fn address_always_serialises_its_parameters_member() {
        assert_eq!(
            serde_json::to_value(Address::new("a@example.com")).unwrap(),
            json!({ "email": "a@example.com", "parameters": null })
        );

        let with_params = Address::new("a@example.com")
            .parameter("size", Some("1000"))
            .parameter("notify", None::<String>);
        let value = serde_json::to_value(&with_params).unwrap();
        assert_eq!(value["email"], json!("a@example.com"));
        let params = value["parameters"].as_object().unwrap();
        assert_eq!(params["size"], json!("1000"));
        assert!(params.contains_key("notify"));
        assert!(params["notify"].is_null());
    }

    /// The `Address` `From` impls and the `<Set>` ↔ `<Get>` conversions
    /// carry both the address and its parameters across.
    #[test]
    fn address_from_impls_and_state_conversion() {
        assert_eq!(
            serde_json::to_value(Address::from("owned@example.com".to_string())).unwrap(),
            json!({ "email": "owned@example.com", "parameters": null })
        );
        assert_eq!(
            serde_json::to_value(Address::from("borrowed@example.com")).unwrap(),
            json!({ "email": "borrowed@example.com", "parameters": null })
        );

        let set_addr = Address::new("x@example.com").parameter("key", Some("value"));
        let get_addr: Address<Get> = set_addr.into();
        assert_eq!(
            serde_json::to_value(&get_addr).unwrap(),
            json!({
                "email": "x@example.com",
                "parameters": { "key": "value" }
            })
        );

        let back: Address<Set> = get_addr.into();
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            json!({
                "email": "x@example.com",
                "parameters": { "key": "value" }
            })
        );
    }

    /// RFC 8621 §6.1 — `onSuccessUpdateEmail`/`onSuccessDestroyEmail`
    /// key on a `#`-prefixed create-id reference by default; the `_id`
    /// variants send the literal string instead.
    #[test]
    fn on_success_back_references_have_two_spellings() {
        let mut args = SetArguments::default();
        args.on_success_update_email("c1").subject("Rewritten");
        args.on_success_update_email_id("M8504").subject("Patched");

        let value = serde_json::to_value(&args).unwrap();
        let updated = value["onSuccessUpdateEmail"].as_object().unwrap();
        assert_eq!(updated["#c1"]["subject"], json!("Rewritten"));
        assert_eq!(updated["M8504"]["subject"], json!("Patched"));

        let mut args = SetArguments::default();
        args.on_success_destroy_email("c1")
            .on_success_destroy_email_id("M9000");
        let value = serde_json::to_value(&args).unwrap();
        assert_eq!(value["onSuccessDestroyEmail"], json!(["#c1", "M9000"]));
    }

    /// RFC 8620 §5.3 — the create id is `c<n>` for the patch flavour and
    /// always `None` for the `<Get>` flavour.
    #[test]
    fn create_id_differs_between_the_two_flavours() {
        assert_eq!(EmailSubmission::<Set>::new(None).create_id(), None);
        assert_eq!(
            EmailSubmission::<Set>::new(Some(2)).create_id(),
            Some("c2".to_string())
        );

        let got: EmailSubmission<Get> =
            serde_json::from_value(json!({ "id": "S1", "identityId": "id-1" })).unwrap();
        assert_eq!(got.create_id(), None);
    }
}
