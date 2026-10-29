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

use super::{ACLPatch, Mailbox, Role, SetArguments};
use crate::{core::set::SetObject, principal::ACL, Get, Set};
use ahash::AHashMap;

impl Mailbox<Set> {
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn parent_id(&mut self, parent_id: Option<impl Into<String>>) -> &mut Self {
        self.parent_id = parent_id.map(|s| s.into());
        self
    }

    pub fn parent_id_ref(&mut self, parent_id_ref: &str) -> &mut Self {
        self.parent_id = format!("#{}", parent_id_ref).into();
        self
    }

    pub fn role(&mut self, role: Role) -> &mut Self {
        if !matches!(role, Role::None) {
            self.role = Some(role);
        } else {
            self.role = None;
        }
        self
    }

    pub fn sort_order(&mut self, sort_order: u32) -> &mut Self {
        self.sort_order = sort_order.into();
        self
    }

    pub fn is_subscribed(&mut self, is_subscribed: bool) -> &mut Self {
        self.is_subscribed = is_subscribed.into();
        self
    }

    pub fn acls<T, U, V>(&mut self, acls: T) -> &mut Self
    where
        T: IntoIterator<Item = (U, V)>,
        U: Into<String>,
        V: IntoIterator<Item = ACL>,
    {
        self.share_with = Some(
            acls.into_iter()
                .map(|(id, acls)| (id.into(), acls.into_iter().map(|acl| (acl, true)).collect()))
                .collect(),
        );
        self
    }

    pub fn acl(&mut self, id: &str, acl: impl IntoIterator<Item = ACL>) -> &mut Self {
        self.acl_patch.get_or_insert_with(AHashMap::new).insert(
            format!("shareWith/{}", id),
            ACLPatch::Replace(acl.into_iter().map(|acl| (acl, true)).collect()),
        );
        self
    }

    pub fn acl_set(&mut self, id: &str, acl: ACL, set: bool) -> &mut Self {
        self.acl_patch
            .get_or_insert_with(AHashMap::new)
            .insert(format!("shareWith/{}/{}", id, acl), ACLPatch::Set(set));
        self
    }
}

pub fn role_not_set(role: &Option<Role>) -> bool {
    matches!(role, Some(Role::None))
}

impl SetObject for Mailbox<Set> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        Mailbox {
            _create_id,
            _state: Default::default(),
            id: None,
            name: None,
            parent_id: "".to_string().into(),
            role: Role::None.into(),
            sort_order: None,
            total_emails: None,
            unread_emails: None,
            total_threads: None,
            unread_threads: None,
            my_rights: None,
            is_subscribed: None,
            share_with: AHashMap::with_capacity(0).into(),
            acl_patch: None,
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetObject for Mailbox<Get> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        unimplemented!()
    }

    fn create_id(&self) -> Option<String> {
        None
    }
}

impl SetArguments {
    pub fn on_destroy_remove_emails(&mut self, value: bool) -> &mut Self {
        self.on_destroy_remove_emails = value.into();
        self
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Mailbox<Set>` patch builders
    //! (RFC 8621 §2.5). Each builder is driven through the fluent API
    //! and the resulting JSON object is asserted as a whole, so a renamed
    //! or dropped member shows up immediately.

    use super::*;
    use serde_json::json;

    fn mailbox() -> Mailbox<Set> {
        <Mailbox<Set> as SetObject>::new(Some(0))
    }

    /// A freshly built patch object writes nothing at all: the `""` /
    /// `Role::None` / empty-map seeds `SetObject::new` installs are all
    /// suppressed by the `*_not_set` guards.
    #[test]
    fn a_fresh_patch_object_is_empty() {
        assert_eq!(serde_json::to_value(mailbox()).unwrap(), json!({}));
    }

    /// RFC 8621 §2.5 — the five plain members, spelled as the RFC names
    /// them, with `role` going through `Role`'s own serialisation.
    #[test]
    fn builders_serialise_the_rfc_property_names() {
        let mut mailbox = mailbox();
        mailbox
            .name("Projects")
            .parent_id(Some("MB23cfa8094c0f41e6"))
            .role(Role::Important)
            .sort_order(42)
            .is_subscribed(true);

        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({
                "name": "Projects",
                "parentId": "MB23cfa8094c0f41e6",
                "role": "important",
                "sortOrder": 42,
                "isSubscribed": true
            })
        );
    }

    /// RFC 8620 §3.6 / RFC 8621 §2.5 — a creation reference is spelled
    /// `#<createId>`, which is how a create can point at another create
    /// in the same request.
    #[test]
    fn parent_id_ref_writes_the_hash_form() {
        let mut mailbox = mailbox();
        mailbox.parent_id_ref("c1");
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({ "parentId": "#c1" })
        );

        // the reference is just a string: nothing stops a caller naming a
        // create id that is not `c<n>`
        mailbox.parent_id_ref("special");
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({ "parentId": "#special" })
        );
    }

    /// `role(Role::None)` clears the field to `None`, which the guard no
    /// longer suppresses — the member rides the wire as an explicit
    /// `null`, the RFC 8620 §5.3 "clear this property" patch
    /// (Finding #39).
    #[test]
    fn role_none_clears_the_member_to_null() {
        let mut mailbox = mailbox();
        assert!(role_not_set(&mailbox.role), "the seed is suppressed");

        mailbox.role(Role::None);
        assert_eq!(mailbox.role, None);
        assert!(!role_not_set(&mailbox.role));
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({ "role": null }),
            "a cleared role is sent as null, not omitted"
        );
    }

    /// `parent_id(None)` clears `parentId` the same way, which is how a
    /// mailbox is moved to the top level.
    #[test]
    fn parent_id_none_clears_to_null() {
        let mut mailbox = mailbox();
        mailbox.parent_id(Some("MB23cfa8094c0f41e6"));
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({ "parentId": "MB23cfa8094c0f41e6" })
        );

        mailbox.parent_id(None::<String>);
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({ "parentId": null })
        );
    }

    /// RFC 8621 §2.1 — `acls` replaces `shareWith` with a
    /// principal → {right: true} map, built by `into_iter/collect`.
    #[test]
    fn acls_collect_principals_into_rights_maps() {
        let mut mailbox = mailbox();
        mailbox.acls([
            ("alice@example.com", vec![ACL::ReadItems, ACL::AddItems]),
            ("bob@example.com", vec![ACL::Administer]),
        ]);

        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({
                "shareWith": {
                    "alice@example.com": {
                        "mayReadItems": true,
                        "mayAddItems": true
                    },
                    "bob@example.com": { "mayShare": true }
                }
            }),
            "rights go out under their serde `mayXxx` names, not the Display form"
        );

        // An empty iterator installs an empty map, which the guard
        // suppresses — so the member disappears rather than clearing.
        mailbox.acls(Vec::<(&str, [ACL; 0])>::new());
        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({}),
            "an empty shareWith map is suppressed by the guard"
        );
    }

    /// RFC 8621 §2.5 — the dotted patch keys `shareWith/<principal>`
    /// (replace a principal's whole right set) and
    /// `shareWith/<principal>/<right>` (set one right).
    #[test]
    fn acl_patch_uses_dotted_keys() {
        let mut mailbox = mailbox();
        mailbox
            .acl("carol@example.com", [ACL::Rename, ACL::Delete])
            .acl_set("dave@example.com", ACL::Submit, false);

        assert_eq!(
            serde_json::to_value(&mailbox).unwrap(),
            json!({
                "shareWith/carol@example.com": {
                    "mayRename": true,
                    "mayDelete": true
                },
                "shareWith/dave@example.com/submit": false
            }),
            "the per-right key uses the ACL's Display name"
        );
    }

    /// `SetObject::create_id` spells the `c<n>` creation id for the Set
    /// flavour; the Get flavour never carries one.
    #[test]
    fn create_id_is_the_c_prefixed_counter() {
        assert_eq!(
            mailbox().create_id(),
            Some("c0".to_string()),
            "the create id is the counter `SetObject::new` was given"
        );
        assert_eq!(
            <Mailbox<Set> as SetObject>::new(None).create_id(),
            None,
            "an update patch has no create id"
        );
        assert_eq!(
            serde_json::from_value::<Mailbox<Get>>(json!({}))
                .unwrap()
                .create_id(),
            None,
            "a Get-side object never has a create id"
        );
    }

    /// RFC 8621 §2.5 — the one `Mailbox/set` argument.
    #[test]
    fn set_argument_on_destroy_remove_emails() {
        let mut args = SetArguments::default();
        assert_eq!(serde_json::to_value(&args).unwrap(), json!({}));

        args.on_destroy_remove_emails(true);
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({ "onDestroyRemoveEmails": true })
        );

        args.on_destroy_remove_emails(false);
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({ "onDestroyRemoveEmails": false }),
            "an explicit false is still sent"
        );
    }
}
