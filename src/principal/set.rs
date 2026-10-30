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

use super::{Principal, Property, Type, ACL, DKIM};
use crate::{core::set::SetObject, Get, Set};
use ahash::AHashMap;
use serde_json::Value;

impl Principal<Set> {
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = name.into().into();
        self
    }

    pub fn description(&mut self, description: Option<impl Into<String>>) -> &mut Self {
        self.description = description.map(|s| s.into());
        self
    }

    pub fn email(&mut self, email: impl Into<String>) -> &mut Self {
        self.email = email.into().into();
        self
    }

    pub fn secret(&mut self, secret: impl Into<String>) -> &mut Self {
        self.secret = secret.into().into();
        self
    }

    pub fn timezone(&mut self, timezone: Option<impl Into<String>>) -> &mut Self {
        self.timezone = timezone.map(|s| s.into());
        self
    }

    pub fn picture(&mut self, picture: Option<impl Into<String>>) -> &mut Self {
        self.picture = picture.map(|s| s.into());
        self
    }

    pub fn quota(&mut self, quota: Option<u32>) -> &mut Self {
        self.quota = quota;
        self
    }

    pub fn ptype(&mut self, ptype: Type) -> &mut Self {
        self.ptype = ptype.into();
        self
    }

    pub fn dkim(&mut self, dkim: DKIM) -> &mut Self {
        self.dkim = dkim.into();
        self
    }

    /// RFC 9670 §4 — the sharing map, Principal id → the rights given to that
    /// Principal. It rides the wire under the RFC's `shareWith` name.
    pub fn acl(&mut self, acl: Option<AHashMap<String, Vec<ACL>>>) -> &mut Self {
        self.acl = acl;
        self
    }

    pub fn aliases<T, U>(&mut self, aliases: Option<T>) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.aliases = aliases.map(|l| l.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn alias(&mut self, alias: &str, set: bool) -> &mut Self {
        self.property_patch
            .get_or_insert_with(AHashMap::new)
            .insert(format!("{}/{}", Property::Aliases, alias), set);
        self
    }

    /// RFC 9670 §2 — `capabilities` is `String[Object]`: a map of JMAP
    /// capability URI to the domain-specific object that capability defines
    /// for a Principal. Anything that yields `(uri, value)` pairs is accepted,
    /// so both an `AHashMap<String, Value>` and a slice of pairs work.
    pub fn capabilities<T, U>(&mut self, capabilities: Option<T>) -> &mut Self
    where
        T: IntoIterator<Item = (U, Value)>,
        U: Into<String>,
    {
        self.capabilities =
            capabilities.map(|l| l.into_iter().map(|(k, v)| (k.into(), v)).collect());
        self
    }

    pub fn members<T, U>(&mut self, members: Option<T>) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.members = members.map(|l| l.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn member(&mut self, member: &str, set: bool) -> &mut Self {
        self.property_patch
            .get_or_insert_with(AHashMap::new)
            .insert(format!("{}/{}", Property::Members, member), set);
        self
    }
}

impl SetObject for Principal<Set> {
    type SetArguments = ();

    fn new(_create_id: Option<usize>) -> Self {
        Principal {
            _create_id,
            _state: Default::default(),
            id: None,
            ptype: None,
            name: "".to_string().into(),
            description: "".to_string().into(),
            email: "".to_string().into(),
            timezone: "".to_string().into(),
            capabilities: AHashMap::with_capacity(0).into(),
            aliases: Vec::with_capacity(0).into(),
            secret: "".to_string().into(),
            dkim: None,
            quota: None,
            picture: "".to_string().into(),
            members: Vec::with_capacity(0).into(),
            acl: AHashMap::with_capacity(0).into(),
            property_patch: None,
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetObject for Principal<Get> {
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
    //! Unit tests for the `Principal<Set>` patch builders
    //! (RFC 9670 §4). Every setter is driven through the fluent API and
    //! the resulting JSON object is asserted as a whole, so a renamed or
    //! dropped member shows up immediately.

    use super::*;
    use serde_json::json;

    fn principal() -> Principal<Set> {
        <Principal<Set> as SetObject>::new(Some(0))
    }

    /// A fresh patch object writes nothing at all: the `""` / empty-list /
    /// empty-map seeds `SetObject::new` installs are all suppressed by
    /// the `*_not_set` guards.
    #[test]
    fn a_fresh_patch_object_is_empty() {
        assert_eq!(serde_json::to_value(principal()).unwrap(), json!({}));
    }

    /// RFC 9670 §4 — the scalar members, with `type` going through
    /// `Type`'s serde rename.
    #[test]
    fn scalar_builders_serialise_the_rfc_names() {
        let mut principal = principal();
        principal
            .name("Alice Anderson")
            .description(Some("Team lead"))
            .email("alice@example.org")
            .secret("s3cr3t")
            .timezone(Some("Europe/Berlin"))
            .picture(Some("https://example.org/alice.png"))
            .quota(Some(1_073_741_824))
            .ptype(Type::Individual);

        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "type": "individual",
                "name": "Alice Anderson",
                "description": "Team lead",
                "email": "alice@example.org",
                "timeZone": "Europe/Berlin",
                "secret": "s3cr3t",
                "quota": 1_073_741_824,
                "picture": "https://example.org/alice.png"
            })
        );
    }

    /// RFC 9670 §4 — the principal types all round-trip through the
    /// `ptype` builder under their serde names.
    #[test]
    fn ptype_covers_every_principal_type() {
        for (ptype, wire) in [
            (Type::Individual, "individual"),
            (Type::Group, "group"),
            (Type::Resource, "resource"),
            (Type::Location, "location"),
            (Type::Domain, "domain"),
            (Type::List, "list"),
            (Type::Other, "other"),
        ] {
            let mut principal = principal();
            principal.ptype(ptype.clone());
            assert_eq!(
                serde_json::to_value(&principal).unwrap(),
                json!({ "type": wire }),
                "{ptype:?} must serialise as {wire}"
            );
        }
    }

    /// RFC 9670 §4 — `dkim` carries the selector and an expiration, and
    /// because the struct has no `skip_serializing_if` both `None` members
    /// ride the wire as explicit `null` (Finding #71).
    #[test]
    fn dkim_builder_and_its_null_members() {
        let mut principal = principal();
        principal.dkim(DKIM::new(Some("sel1"), Some(1_735_689_600_i64)));
        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "dkim": {
                    "dkimSelector": "sel1",
                    "dkimExpiration": 1_735_689_600_i64
                }
            })
        );

        let dkim = DKIM::new(None::<String>, None);
        assert!(dkim.selector().is_none());
        assert_eq!(dkim.expiration(), None);
        principal.dkim(dkim);
        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "dkim": { "dkimSelector": null, "dkimExpiration": null }
            }),
            "an absent selector and an absent expiration are indistinguishable from cleared ones"
        );
    }

    /// RFC 9670 §2 — the two list members, `aliases` and `members`.
    #[test]
    fn list_builders_collect_their_items() {
        let mut principal = principal();
        principal
            .aliases(Some(["alice@example.org", "a@example.org"]))
            .members(Some(["P1", "P2"]));

        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "aliases": ["alice@example.org", "a@example.org"],
                "members": ["P1", "P2"]
            })
        );
    }

    /// RFC 9670 §2 — `capabilities` is `String[Object]`, a map of capability
    /// URI to that capability's own object, not a flat list of URIs.
    #[test]
    fn capabilities_is_a_map_of_uri_to_domain_object() {
        let mut principal = principal();
        principal.capabilities(Some([(
            "urn:com.example:jmap:todo",
            json!({ "accountId": null, "mayShareWith": true }),
        )]));

        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "capabilities": {
                    "urn:com.example:jmap:todo": {
                        "accountId": null,
                        "mayShareWith": true
                    }
                }
            })
        );

        // A capability whose registered value is an empty object is legal too
        // (RFC 9670 §1.5.1 gives `urn:ietf:params:jmap:principals` one), and a
        // ready-made map is accepted as well as an iterator of pairs.
        let mut shared = <Principal<Set> as SetObject>::new(Some(0));
        shared.capabilities(Some(AHashMap::from([(
            "urn:ietf:params:jmap:principal".to_string(),
            json!({}),
        )])));
        assert_eq!(
            serde_json::to_value(&shared).unwrap(),
            json!({ "capabilities": { "urn:ietf:params:jmap:principal": {} } })
        );
    }

    /// Passing `None` to a list builder clears the field to `None`, which
    /// the guard no longer suppresses — the patch sends `null`
    /// (Finding #39).
    #[test]
    fn list_builders_clear_to_null() {
        let mut principal = principal();
        principal.aliases(None::<[&str; 0]>);
        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({ "aliases": null })
        );

        principal.members(None::<[&str; 0]>);
        principal.capabilities(None::<[(String, Value); 0]>);
        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "aliases": null,
                "members": null,
                "capabilities": null
            })
        );
    }

    /// RFC 9670 §4 — the sharing map is principal → list of rights, and
    /// the rights go out under their serde `mayXxx` names, not the
    /// `Display` form (Finding #72). The map itself rides the wire under
    /// the RFC's `shareWith` name, which is also the name `Property::ShareWith`
    /// asks the server for.
    #[test]
    fn acl_map_builder() {
        let mut principal = principal();
        let mut acls = AHashMap::new();
        acls.insert("P2".to_string(), vec![ACL::ReadItems, ACL::Administer]);
        principal.acl(Some(acls));

        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({ "shareWith": { "P2": ["mayReadItems", "mayShare"] } })
        );

        principal.acl(None);
        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({ "shareWith": null }),
            "a cleared shareWith is sent as null, not omitted"
        );
    }

    /// RFC 9670 §4.4 — the dotted patch keys `aliases/<value>` and
    /// `members/<value>`, built from `Property::Aliases`/
    /// `Property::Members`; the value may be a `#<createId>` reference.
    #[test]
    fn alias_and_member_dotted_keys() {
        let mut principal = principal();
        principal
            .alias("a@example.org", true)
            .alias("old@example.org", false)
            .member("P2", true)
            .member("#c1", false);

        assert_eq!(
            serde_json::to_value(&principal).unwrap(),
            json!({
                "aliases/a@example.org": true,
                "aliases/old@example.org": false,
                "members/P2": true,
                "members/#c1": false
            }),
            "the key prefix is the property name, so `#c1` is a creation reference"
        );
    }

    /// `SetObject::create_id` spells the `c<n>` creation id for the Set
    /// flavour; the Get flavour never carries one.
    #[test]
    fn create_id_is_the_c_prefixed_counter() {
        assert_eq!(principal().create_id(), Some("c0".to_string()));
        assert_eq!(
            <Principal<Set> as SetObject>::new(Some(7)).create_id(),
            Some("c7".to_string())
        );
        assert_eq!(
            <Principal<Set> as SetObject>::new(None).create_id(),
            None,
            "an update patch has no create id"
        );
        assert_eq!(
            serde_json::from_value::<Principal<Get>>(json!({ "id": "P1" }))
                .unwrap()
                .create_id(),
            None
        );
    }
}
