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

use crate::core::set::{list_not_set, map_not_set, string_not_set};
use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Display;

use crate::{
    core::{changes::ChangesObject, Object},
    Get, Set,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal<State = Get> {
    #[serde(skip)]
    _create_id: Option<usize>,

    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,

    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    ptype: Option<Type>,

    #[serde(skip_serializing_if = "string_not_set")]
    name: Option<String>,

    #[serde(skip_serializing_if = "string_not_set")]
    description: Option<String>,

    #[serde(skip_serializing_if = "string_not_set")]
    email: Option<String>,

    /// RFC 9670 §2 — the property is spelled `timeZone`, capital `Z`.
    #[serde(rename = "timeZone")]
    #[serde(skip_serializing_if = "string_not_set")]
    timezone: Option<String>,

    /// RFC 9670 §2 — `capabilities` is `String[Object]`: a map of JMAP
    /// capability URI to the domain-specific information that capability's
    /// specification defines for a Principal. The value is kept as raw JSON
    /// because every capability has its own shape.
    #[serde(skip_serializing_if = "map_not_set")]
    capabilities: Option<AHashMap<String, Value>>,

    #[serde(skip_serializing_if = "list_not_set")]
    aliases: Option<Vec<String>>,

    #[serde(skip_serializing_if = "string_not_set")]
    secret: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    dkim: Option<DKIM>,

    #[serde(skip_serializing_if = "Option::is_none")]
    quota: Option<u32>,

    #[serde(skip_serializing_if = "string_not_set")]
    picture: Option<String>,

    #[serde(skip_serializing_if = "list_not_set")]
    members: Option<Vec<String>>,

    /// RFC 9670 §4 names the sharing property `shareWith`: a map of Principal
    /// id to the rights given to that Principal. The Rust side keeps the short
    /// `acl` name, but the wire name must be the RFC's or the data the server
    /// answers under `shareWith` is silently dropped.
    #[serde(rename = "shareWith")]
    #[serde(skip_serializing_if = "map_not_set")]
    acl: Option<AHashMap<String, Vec<ACL>>>,

    #[serde(flatten)]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    property_patch: Option<AHashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum Property {
    #[serde(rename = "id")]
    Id = 0,
    #[serde(rename = "type")]
    Type = 1,
    #[serde(rename = "name")]
    Name = 2,
    #[serde(rename = "description")]
    Description = 3,
    #[serde(rename = "email")]
    Email = 4,
    #[serde(rename = "timeZone")]
    Timezone = 5,
    #[serde(rename = "capabilities")]
    Capabilities = 6,
    #[serde(rename = "aliases")]
    Aliases = 7,
    #[serde(rename = "secret")]
    Secret = 8,
    #[serde(rename = "dkim")]
    DKIM = 9,
    #[serde(rename = "quota")]
    Quota = 10,
    #[serde(rename = "picture")]
    Picture = 11,
    #[serde(rename = "members")]
    Members = 12,
    #[serde(rename = "shareWith")]
    ShareWith = 13,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Copy)]
pub enum ACL {
    #[serde(rename = "mayRename")]
    Rename = 1,
    #[serde(rename = "mayDelete")]
    Delete = 2,
    #[serde(rename = "mayReadItems")]
    ReadItems = 3,
    #[serde(rename = "mayAddItems")]
    AddItems = 4,
    #[serde(rename = "maySetKeywords")]
    SetKeywords = 5,
    #[serde(rename = "mayRemoveItems")]
    RemoveItems = 6,
    #[serde(rename = "mayCreateChild")]
    CreateChild = 7,
    #[serde(rename = "mayShare")]
    Administer = 8,
    #[serde(rename = "maySubmit")]
    Submit = 10,
    #[serde(rename = "maySetSeen")]
    SetSeen = 11,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    #[serde(rename = "individual")]
    Individual,
    #[serde(rename = "group")]
    Group,
    #[serde(rename = "resource")]
    Resource,
    #[serde(rename = "location")]
    Location,
    #[serde(rename = "domain")]
    Domain,
    #[serde(rename = "list")]
    List,
    #[serde(rename = "other")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DKIM {
    #[serde(rename = "dkimSelector")]
    dkim_selector: Option<String>,
    #[serde(rename = "dkimExpiration")]
    dkim_expiration: Option<i64>,
}

impl DKIM {
    pub fn new(dkim_selector: Option<impl Into<String>>, dkim_expiration: Option<i64>) -> DKIM {
        DKIM {
            dkim_selector: dkim_selector.map(Into::into),
            dkim_expiration,
        }
    }

    pub fn selector(&self) -> Option<&str> {
        self.dkim_selector.as_deref()
    }

    pub fn expiration(&self) -> Option<i64> {
        self.dkim_expiration
    }
}

impl Display for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::Type => write!(f, "type"),
            Property::Name => write!(f, "name"),
            Property::Description => write!(f, "description"),
            Property::Email => write!(f, "email"),
            Property::Timezone => write!(f, "timeZone"),
            Property::Capabilities => write!(f, "capabilities"),
            Property::Aliases => write!(f, "aliases"),
            Property::Secret => write!(f, "secret"),
            Property::DKIM => write!(f, "dkim"),
            Property::Quota => write!(f, "quota"),
            Property::Picture => write!(f, "picture"),
            Property::Members => write!(f, "members"),
            Property::ShareWith => write!(f, "shareWith"),
        }
    }
}

impl Display for ACL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ACL::Rename => write!(f, "rename"),
            ACL::Delete => write!(f, "delete"),
            ACL::ReadItems => write!(f, "readItems"),
            ACL::AddItems => write!(f, "addItems"),
            ACL::SetKeywords => write!(f, "setKeywords"),
            ACL::RemoveItems => write!(f, "removeItems"),
            ACL::CreateChild => write!(f, "createChild"),
            ACL::Administer => write!(f, "administer"),
            ACL::Submit => write!(f, "submit"),
            ACL::SetSeen => write!(f, "setSeen"),
        }
    }
}

impl Object for Principal<Set> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl Object for Principal<Get> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl ChangesObject for Principal<Set> {
    type ChangesResponse = ();
}

impl ChangesObject for Principal<Get> {
    type ChangesResponse = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Object` contract on both flavours of
    //! `Principal` (RFC 4371 §4). No server, no socket, no async.

    use super::*;

    /// RFC 4371 §4.1 - §4.3 — every Principal method takes an
    /// `accountId`, so both the patch and the fetched flavour report that
    /// one is required. The `<Get>` arm is the one the generic request
    /// machinery consults when it is handed a fetched record rather than
    /// a patch, and it must agree with `<Set>` or the two paths would
    /// disagree about whether to put `accountId` on the wire.
    #[test]
    fn both_principal_flavours_require_an_account_id() {
        assert!(
            <Principal<Set> as Object>::requires_account_id(),
            "RFC 4371 §4.1 gives Principal/get an accountId argument"
        );
        assert!(
            <Principal<Get> as Object>::requires_account_id(),
            "the fetched flavour must agree with the patch flavour"
        );
    }

    /// RFC 9670 §2 / §4 — the property enum is the `Property` associated type
    /// on both flavours, and its wire names are the ones the RFC spells:
    /// `timeZone` with a capital `Z`, and `shareWith` for the sharing map.
    #[test]
    fn the_property_type_is_shared_and_names_the_rfc_members() {
        assert_eq!(Property::ShareWith.to_string(), "shareWith");
        assert_eq!(Property::DKIM.to_string(), "dkim");
        assert_eq!(
            serde_json::to_value(Property::Timezone).unwrap(),
            serde_json::json!("timeZone")
        );
        assert_eq!(Property::Timezone.to_string(), "timeZone");
        assert_eq!(
            serde_json::from_value::<Property>(serde_json::json!("timeZone")).unwrap(),
            Property::Timezone,
            "a server that answers with the RFC name must land on the variant"
        );
    }

    /// RFC 9670 §2 Figure 3 — a Principal record written exactly the way the
    /// RFC writes it parses, and every accessor returns the data.
    ///
    /// Before the wire names were corrected this record lost three members:
    /// `timeZone` was read as `timezone`, `capabilities` was typed as a flat
    /// `String[]` so the RFC's map failed the parse outright, and the sharing
    /// map was read as `acl` while the client itself asked for `shareWith`.
    #[test]
    fn an_rfc_9670_shaped_principal_record_parses() {
        let principal: Principal<Get> = serde_json::from_value(serde_json::json!({
            "id": "P2342fnddd20",
            "type": "individual",
            "name": "Joe Bloggs",
            "description": null,
            "email": "joe.bloggs@example.com",
            "timeZone": "Australia/Melbourne",
            "capabilities": {
                "urn:com.example:jmap:todo": {
                    "accountId": null,
                    "mayShareWith": true
                }
            },
            "shareWith": {
                "P674pp24095qo49pr": ["mayReadItems", "mayAddItems"]
            }
        }))
        .expect("an RFC 9670 record must deserialize");

        assert_eq!(principal.timezone(), Some("Australia/Melbourne"));
        let capabilities = principal
            .capabilities()
            .expect("capabilities must be present");
        assert_eq!(capabilities.len(), 1);
        assert_eq!(
            capabilities["urn:com.example:jmap:todo"]["mayShareWith"],
            Value::Bool(true)
        );
        assert_eq!(
            principal.acl().expect("shareWith must be present")["P674pp24095qo49pr"],
            vec![ACL::ReadItems, ACL::AddItems]
        );

        // And the same three members go back out under the RFC names.
        let patch = serde_json::to_value({
            let mut principal = <Principal<Set> as crate::core::set::SetObject>::new(None);
            principal.timezone(Some("Australia/Melbourne"));
            principal.acl(Some(AHashMap::from([(
                "P674pp24095qo49pr".to_string(),
                vec![ACL::ReadItems, ACL::AddItems],
            )])));
            principal
        })
        .unwrap();
        assert_eq!(
            patch,
            serde_json::json!({
                "timeZone": "Australia/Melbourne",
                "shareWith": {
                    "P674pp24095qo49pr": ["mayReadItems", "mayAddItems"]
                }
            })
        );
    }
}
