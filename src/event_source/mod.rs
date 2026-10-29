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

pub mod parser;
pub mod stream;

use crate::{core::session::URLParser, CalendarAlert, DataType};
use ahash::AHashMap;
use serde::{Deserialize, Serialize};

pub enum URLParameter {
    Types,
    CloseAfter,
    Ping,
}

impl URLParser for URLParameter {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "types" => Some(URLParameter::Types),
            "closeafter" => Some(URLParameter::CloseAfter),
            "ping" => Some(URLParameter::Ping),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the event-source URL-template placeholder parser.
    //! The template is published in the session document as
    //! `_eventSourceUrl` (RFC 8620 §2) and the query parameters it
    //! accepts are fixed by RFC 8620 §7.4. No server, no socket, no async.

    use super::*;
    use std::mem::{self, Discriminant};

    /// Which variant a name parsed into, spelled so the assertion names
    /// the variant it expected instead of just comparing a `matches!`
    /// result.
    fn parsed(name: &str) -> Discriminant<URLParameter> {
        mem::discriminant(&URLParameter::parse(name).unwrap())
    }

    /// RFC 8620 §7.4 — `types`, `closeafter` and `ping` all parse back
    /// into their own variant.
    #[test]
    fn the_three_event_source_placeholders_parse() {
        for (name, want) in [
            ("types", mem::discriminant(&URLParameter::Types)),
            ("closeafter", mem::discriminant(&URLParameter::CloseAfter)),
            ("ping", mem::discriminant(&URLParameter::Ping)),
        ] {
            assert_eq!(
                parsed(name),
                want,
                "{name:?} must parse as its own URLParameter variant"
            );
        }
    }

    /// RFC 8620 §7.4 — the parameter names are lower case on the wire, so
    /// the camel-case spellings a client might guess are rejected rather
    /// than silently accepted.
    #[test]
    fn camel_case_and_unknown_names_are_rejected() {
        for name in ["", "Types", "closeAfter", "CloseAfter", "types[]", "state", " ping"] {
            assert!(
                URLParameter::parse(name).is_none(),
                "{name:?} must not parse as an event-source placeholder"
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushNotification {
    StateChange(Changes),
    CalendarAlert(CalendarAlert),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    id: Option<String>,
    changes: AHashMap<String, AHashMap<DataType, String>>,
}

impl Changes {
    pub fn new(id: Option<String>, changes: AHashMap<String, AHashMap<DataType, String>>) -> Self {
        Self { id, changes }
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn account_changes(&mut self, account_id: &str) -> Option<AHashMap<DataType, String>> {
        self.changes.remove(account_id)
    }

    pub fn changed_accounts(&self) -> impl Iterator<Item = &String> {
        self.changes.keys()
    }

    pub fn changes(&self, account_id: &str) -> Option<impl Iterator<Item = (&DataType, &String)>> {
        self.changes.get(account_id).map(|changes| changes.iter())
    }

    pub fn has_type(&self, type_: DataType) -> bool {
        self.changes
            .values()
            .any(|changes| changes.contains_key(&type_))
    }

    pub fn into_inner(self) -> AHashMap<String, AHashMap<DataType, String>> {
        self.changes
    }

    pub fn is_empty(&self) -> bool {
        !self.changes.values().any(|changes| !changes.is_empty())
    }
}
