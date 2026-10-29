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

use serde::Serialize;

use crate::{
    core::query::{self, QueryObject},
    Set,
};

use super::SieveScript;

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Filter {
    Name {
        #[serde(rename = "name")]
        value: String,
    },
    IsActive {
        #[serde(rename = "isActive")]
        value: bool,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "property")]
pub enum Comparator {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "isActive")]
    IsActive,
}

impl Filter {
    pub fn name(value: impl Into<String>) -> Self {
        Filter::Name {
            value: value.into(),
        }
    }

    pub fn is_active(value: bool) -> Self {
        Filter::IsActive { value }
    }
}

impl Comparator {
    pub fn name() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::Name)
    }

    pub fn is_active() -> query::Comparator<Comparator> {
        query::Comparator::new(Comparator::IsActive)
    }
}

impl QueryObject for SieveScript<Set> {
    type QueryArguments = ();

    type Filter = Filter;

    type Sort = Comparator;
}

#[cfg(test)]
mod tests {
    //! Unit tests for the sieve query filter and comparator shapes from
    //! draft-ietf-jmap-sieve-12 §5.3/§5.4. No server, no socket, no
    //! async.

    use super::*;
    use serde_json::json;

    /// draft-ietf-jmap-sieve-12 §5.3 — each filter variant owns exactly
    /// one key on the wire.
    #[test]
    fn filter_variants_serialise_under_their_own_key() {
        assert_eq!(
            serde_json::to_value(Filter::name("Filters")).unwrap(),
            json!({ "name": "Filters" })
        );
        assert_eq!(
            serde_json::to_value(Filter::is_active(true)).unwrap(),
            json!({ "isActive": true })
        );
        assert_eq!(
            serde_json::to_value(Filter::is_active(false)).unwrap(),
            json!({ "isActive": false })
        );
    }

    /// draft-ietf-jmap-sieve-12 §5.4 — a comparator is the `property`
    /// tag plus the shared `isAscending` / `collation` members.
    #[test]
    fn comparators_carry_their_property_and_the_shared_members() {
        assert_eq!(
            serde_json::to_value(Comparator::name()).unwrap(),
            json!({ "isAscending": true, "property": "name" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::is_active()).unwrap(),
            json!({ "isAscending": true, "property": "isActive" })
        );

        assert_eq!(
            serde_json::to_value(Comparator::name().descending()).unwrap(),
            json!({ "isAscending": false, "property": "name" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::is_active().is_ascending(false)).unwrap(),
            json!({ "isAscending": false, "property": "isActive" })
        );
        assert_eq!(
            serde_json::to_value(Comparator::name().collation("i;ascii-casemap".to_string()))
                .unwrap(),
            json!({
                "isAscending": true,
                "collation": "i;ascii-casemap",
                "property": "name"
            })
        );
        assert_eq!(
            serde_json::to_value(Comparator::name().descending().ascending()).unwrap(),
            json!({ "isAscending": true, "property": "name" })
        );
    }
}
