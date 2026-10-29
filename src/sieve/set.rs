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

use crate::{core::set::SetObject, Get, Set};

use super::{SetArguments, SieveScript};

impl SieveScript<Set> {
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn blob_id(&mut self, blob_id: impl Into<String>) -> &mut Self {
        self.blob_id = Some(blob_id.into());
        self
    }
}

impl SetObject for SieveScript<Set> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        SieveScript {
            _create_id,
            _state: Default::default(),
            id: None,
            name: None,
            blob_id: None,
            is_active: None,
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetArguments {
    pub fn on_success_activate_script(&mut self, id: impl Into<String>) -> &mut Self {
        self.on_success_activate_script = Some(format!("#{}", id.into()));
        self
    }

    pub fn on_success_activate_script_id(&mut self, id: impl Into<String>) -> &mut Self {
        self.on_success_activate_script = Some(id.into());
        self
    }

    pub fn on_success_deactivate_script(&mut self, value: bool) -> &mut Self {
        self.on_success_deactivate_script = Some(value);
        self
    }
}

impl SetObject for SieveScript<Get> {
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
    //! Unit tests for the SieveScript patch builders and for the
    //! `<Get>` arm of `SetObject`
    //! (draft-ietf-jmap-sieve-12 §2.2).

    use super::*;
    use serde_json::json;

    /// draft-ietf-jmap-sieve-12 §2.2 — a create patch carries the
    /// script `name` and the `blobId` of the uploaded text, and the
    /// creation id the generic `Set` machinery assigned is rendered with
    /// the `c` prefix.
    #[test]
    fn create_patch_carries_name_and_blob_id() {
        let mut script = <SieveScript<Set> as SetObject>::new(Some(2));
        script.name("filter").blob_id("BlobId1");

        assert_eq!(script.create_id().as_deref(), Some("c2"));
        assert_eq!(
            serde_json::to_value(&script).unwrap(),
            json!({ "name": "filter", "blobId": "BlobId1" }),
            "`isActive` is not part of a create patch"
        );
    }

    /// draft-ietf-jmap-sieve-12 §2.2 — `onSuccessActivateScript` is a
    /// *result reference* (`#c0`) when it points at a script being
    /// created in the same request and a plain id when it points at one
    /// that already exists. The two setters differ only in that prefix.
    #[test]
    fn set_arguments_reference_or_name_the_script_to_activate() {
        let mut args = SetArguments::default();
        args.on_success_activate_script("c0");
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({ "onSuccessActivateScript": "#c0" })
        );

        let mut args = SetArguments::default();
        args.on_success_activate_script_id("script-42");
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({ "onSuccessActivateScript": "script-42" }),
            "the `_id` form adds no `#`"
        );

        let mut args = SetArguments::default();
        args.on_success_deactivate_script(true);
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({ "onSuccessDeactivateScript": true })
        );

        assert_eq!(
            serde_json::to_value(SetArguments::default()).unwrap(),
            json!({}),
            "an untouched argument block writes nothing"
        );
    }

    /// A fetched script is not a create: it has no creation id, whatever
    /// the record itself says.
    #[test]
    fn a_fetched_script_has_no_creation_id() {
        let script: SieveScript<Get> = serde_json::from_value(json!({
            "id": "s1",
            "name": "filter",
            "isActive": true,
            "blobId": "BlobId1"
        }))
        .unwrap();

        assert_eq!(script.create_id(), None);
    }

    /// The `<Get>` arm of `SetObject::new` is a deliberate
    /// `unimplemented!()`: nothing in the generic set machinery ever asks
    /// a fetched record to construct itself, so the only way to reach the
    /// arm is to ask for it directly — and it panics. Narrowing
    /// Finding #94: the arm is reachable, under `#[should_panic]`.
    #[test]
    #[should_panic(expected = "not implemented")]
    fn a_fetched_script_refuses_to_construct_itself() {
        let _ = <SieveScript<Get> as SetObject>::new(None);
    }
}
