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

use crate::core::session::URLParser;

pub mod copy;
pub mod download;
pub mod helpers;
pub mod upload;

pub enum URLParameter {
    AccountId,
    BlobId,
    Name,
    Type,
}

impl URLParser for URLParameter {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "accountId" => Some(URLParameter::AccountId),
            "blobId" => Some(URLParameter::BlobId),
            "name" => Some(URLParameter::Name),
            "type" => Some(URLParameter::Type),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the blob URL-template placeholder parser. The
    //! template is published in the session document as `_blobUrl`
    //! (RFC 8620 §2) and the placeholders it may carry are fixed by
    //! RFC 8621 §4.1. No server, no socket, no async.

    use super::*;
    use std::mem::{self, Discriminant};

    /// Which variant a name parsed into, spelled so the assertion names the
    /// variant it expected instead of just comparing a `matches!` result.
    fn parsed(name: &str) -> Discriminant<URLParameter> {
        mem::discriminant(&URLParameter::parse(name).unwrap())
    }

    /// RFC 8621 §4.1 — all four placeholders a blob URL template may
    /// carry parse back into their own variant.
    #[test]
    fn the_four_blob_url_placeholders_parse() {
        for (name, want) in [
            ("accountId", mem::discriminant(&URLParameter::AccountId)),
            ("blobId", mem::discriminant(&URLParameter::BlobId)),
            ("name", mem::discriminant(&URLParameter::Name)),
            ("type", mem::discriminant(&URLParameter::Type)),
        ] {
            assert_eq!(
                parsed(name),
                want,
                "{name:?} must parse as its own URLParameter variant"
            );
        }
    }

    /// RFC 8621 §4.1 — matching is exact. The parser is case sensitive,
    /// does not strip the `{}` of the template itself, and rejects every
    /// name that is not one of the four placeholders.
    #[test]
    fn any_other_name_is_not_a_blob_placeholder() {
        for name in [
            "",
            "Type",
            "blobIds",
            "account",
            "{blobId}",
            "accountId=",
            "blobId}",
            " type",
        ] {
            assert!(
                URLParameter::parse(name).is_none(),
                "{name:?} must not parse as a blob URL placeholder"
            );
        }
    }
}
