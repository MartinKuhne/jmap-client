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

use crate::{core::get::GetObject, Get, Set};

use super::SieveScript;

impl SieveScript<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn blob_id(&self) -> Option<&str> {
        self.blob_id.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.is_active.unwrap_or(false)
    }
}

impl GetObject for SieveScript<Set> {
    type GetArguments = ();
}

impl GetObject for SieveScript<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `SieveScript<Get>` accessors against the
    //! draft-ietf-jmap-sieve-12 §5.1 record shape. No server, no socket,
    //! no async.

    use super::*;
    use serde_json::json;

    fn script(value: serde_json::Value) -> SieveScript<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// draft-ietf-jmap-sieve-12 §5.1 — every member reads back under its
    /// own name.
    #[test]
    fn accessors_read_the_full_record() {
        let script = script(json!({
            "id": "1",
            "name": "Filters",
            "blobId": "0",
            "isActive": true
        }));

        assert_eq!(script.id(), Some("1"));
        assert_eq!(script.name(), Some("Filters"));
        assert_eq!(script.blob_id(), Some("0"));
        assert!(script.is_active());
    }

    /// draft-ietf-jmap-sieve-12 §5.1 — `isActive` is optional and reads
    /// as `false` when the server omits it.
    #[test]
    fn is_active_defaults_to_false() {
        let script = script(json!({ "id": "2", "name": "Draft" }));
        assert!(!script.is_active());
        assert!(script.blob_id().is_none());
    }

    /// `take_id` hands the id out and leaves the field empty, so a second
    /// call yields the empty string rather than panicking.
    #[test]
    fn take_id_drains_the_field() {
        let mut script = script(json!({ "id": "1", "name": "Filters" }));
        assert_eq!(script.take_id(), "1");
        assert!(script.id().is_none());
        assert_eq!(script.take_id(), "");
    }
}
