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

use super::{Principal, Type, ACL, DKIM};
use crate::{core::get::GetObject, Get, Set};
use ahash::AHashMap;
use serde_json::Value;

impl Principal<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn ptype(&self) -> Option<&Type> {
        self.ptype.as_ref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn timezone(&self) -> Option<&str> {
        self.timezone.as_deref()
    }

    pub fn secret(&self) -> Option<&str> {
        self.secret.as_deref()
    }

    pub fn picture(&self) -> Option<&str> {
        self.picture.as_deref()
    }

    pub fn quota(&self) -> Option<u32> {
        self.quota
    }

    /// RFC 9670 §2 — the `capabilities` map: JMAP capability URI to the
    /// domain-specific object that capability defines for a Principal.
    pub fn capabilities(&self) -> Option<&AHashMap<String, Value>> {
        self.capabilities.as_ref()
    }

    pub fn aliases(&self) -> Option<&[String]> {
        self.aliases.as_deref()
    }

    pub fn members(&self) -> Option<&[String]> {
        self.members.as_deref()
    }

    pub fn dkim(&self) -> Option<&DKIM> {
        self.dkim.as_ref()
    }

    pub fn acl(&self) -> Option<&AHashMap<String, Vec<ACL>>> {
        self.acl.as_ref()
    }
}

impl GetObject for Principal<Set> {
    type GetArguments = ();
}

impl GetObject for Principal<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Principal<Get>` id accessors against
    //! RFC 4371 §3.3 (Principal objects). No server, no socket, no async.

    use super::*;
    use serde_json::json;

    fn principal(value: serde_json::Value) -> Principal<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// RFC 4371 §3.3 — `id` borrows the principal id and `take_id`
    /// moves it out, leaving the record without one.
    #[test]
    fn take_id_moves_the_id_out() {
        let mut principal = principal(json!({
            "id": "P2342fnddd20",
            "type": "individual",
            "name": "Philip Jagger"
        }));

        assert_eq!(principal.id(), Some("P2342fnddd20"));
        assert_eq!(principal.take_id(), "P2342fnddd20");
        assert_eq!(
            principal.id(),
            None,
            "taking the id must leave the record without one"
        );
        assert_eq!(
            principal.name(),
            Some("Philip Jagger"),
            "taking the id must not disturb the other members"
        );
    }

    /// RFC 4371 §3.3 — a record that never had an id yields the empty
    /// string rather than panicking: the accessor is total.
    #[test]
    fn take_id_on_a_record_without_one_is_empty() {
        let mut principal = principal(json!({ "type": "group" }));

        assert_eq!(principal.id(), None);
        assert_eq!(principal.take_id(), "");
    }
}
