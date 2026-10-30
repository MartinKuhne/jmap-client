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

use super::{Mailbox, MailboxRights, Role};
use crate::{core::get::GetObject, principal::ACL, Get, Set};
use ahash::AHashMap;

impl Mailbox<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    pub fn role(&self) -> Role {
        self.role.as_ref().cloned().unwrap_or(Role::None)
    }

    pub fn sort_order(&self) -> u32 {
        self.sort_order.as_ref().copied().unwrap_or(0)
    }

    pub fn total_emails(&self) -> usize {
        self.total_emails.as_ref().copied().unwrap_or(0)
    }

    pub fn unread_emails(&self) -> usize {
        self.unread_emails.as_ref().copied().unwrap_or(0)
    }

    pub fn total_threads(&self) -> usize {
        self.total_threads.as_ref().copied().unwrap_or(0)
    }

    pub fn unread_threads(&self) -> usize {
        self.unread_threads.as_ref().copied().unwrap_or(0)
    }

    pub fn is_subscribed(&self) -> bool {
        *self.is_subscribed.as_ref().unwrap_or(&false)
    }

    pub fn my_rights(&self) -> Option<&MailboxRights> {
        self.my_rights.as_ref()
    }

    pub fn acl(&self) -> Option<&AHashMap<String, AHashMap<ACL, bool>>> {
        self.share_with.as_ref()
    }

    pub fn take_acl(&mut self) -> Option<AHashMap<String, AHashMap<ACL, bool>>> {
        self.share_with.take()
    }
}

impl MailboxRights {
    pub fn may_read_items(&self) -> bool {
        self.may_read_items
    }

    pub fn may_add_items(&self) -> bool {
        self.may_add_items
    }

    pub fn may_remove_items(&self) -> bool {
        self.may_remove_items
    }

    pub fn may_set_seen(&self) -> bool {
        self.may_set_seen
    }

    pub fn may_set_keywords(&self) -> bool {
        self.may_set_keywords
    }

    pub fn may_create_child(&self) -> bool {
        self.may_create_child
    }

    pub fn may_rename(&self) -> bool {
        self.may_rename
    }

    pub fn may_delete(&self) -> bool {
        self.may_delete
    }

    pub fn may_submit(&self) -> bool {
        self.may_submit
    }

    pub fn acl_list(&self) -> Vec<ACL> {
        let mut acl_list = Vec::new();
        for (is_set, acl) in [
            (self.may_read_items, ACL::ReadItems),
            (self.may_add_items, ACL::AddItems),
            (self.may_remove_items, ACL::RemoveItems),
            (self.may_set_seen, ACL::SetSeen),
            (self.may_set_keywords, ACL::SetKeywords),
            (self.may_create_child, ACL::CreateChild),
            (self.may_rename, ACL::Rename),
            (self.may_delete, ACL::Delete),
            (self.may_submit, ACL::Submit),
        ] {
            if is_set && !acl_list.contains(&acl) {
                acl_list.push(acl);
            }
        }
        acl_list
    }
}

impl GetObject for Mailbox<Set> {
    type GetArguments = ();
}

impl GetObject for Mailbox<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Mailbox<Get>` sharing accessors against
    //! RFC 8621 §5.1 (`shareWith`) and the RFC 4371 §3.4 right names.
    //! No server, no socket, no async.

    use super::*;
    use serde_json::json;

    fn mailbox(value: serde_json::Value) -> Mailbox<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// RFC 8621 §5.1 — `shareWith` is a map of principal id to a map of
    /// RFC 4371 §3.4 right names to booleans, and both accessors read the
    /// same field: `acl()` borrows it, `take_acl()` moves it out.
    #[test]
    fn take_acl_moves_the_share_with_map_out() {
        let mut mailbox = mailbox(json!({
            "id": "mf2",
            "name": "Shared",
            "shareWith": {
                "P2342fnddd20": { "mayReadItems": true, "mayAddItems": false }
            }
        }));

        {
            let acl = mailbox.acl().expect("shareWith must be present");
            assert_eq!(acl.len(), 1);
            let rights = &acl["P2342fnddd20"];
            assert!(rights[&ACL::ReadItems], "mayReadItems was sent true");
            assert!(
                !rights[&ACL::AddItems],
                "mayAddItems was sent false and must stay false"
            );
        }

        assert_eq!(
            mailbox.take_acl().expect("the map must move out").len(),
            1,
            "the moved map must keep every principal"
        );
        assert!(
            mailbox.acl().is_none(),
            "taking the acl must leave the record unshared"
        );
    }

    /// RFC 8621 §5.1 — `shareWith` is optional, so a mailbox nobody has
    /// been granted rights on reads as `None` and takes as `None`.
    #[test]
    fn take_acl_on_an_unshared_mailbox_is_none() {
        let mut mailbox = mailbox(json!({ "id": "mf1", "name": "Inbox" }));

        assert!(mailbox.acl().is_none());
        assert!(mailbox.take_acl().is_none());
    }
}
