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

use super::{Keys, PushSubscription};
use crate::{core::get::GetObject, DataType, Get, Set};
use encodify::base64::URL_SAFE;

impl PushSubscription<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn device_client_id(&self) -> Option<&str> {
        self.device_client_id.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub fn keys(&self) -> Option<&Keys> {
        self.keys.as_ref()
    }

    pub fn verification_code(&self) -> Option<&str> {
        self.verification_code.as_deref()
    }

    pub fn expires(&self) -> Option<i64> {
        self.expires.map(|v| v.timestamp())
    }

    pub fn types(&self) -> Option<&[DataType]> {
        self.types.as_deref()
    }
}

impl Keys {
    pub fn p256dh(&self) -> Option<Vec<u8>> {
        URL_SAFE.decode(&self.p256dh).ok()
    }

    pub fn auth(&self) -> Option<Vec<u8>> {
        URL_SAFE.decode(&self.auth).ok()
    }
}

impl GetObject for PushSubscription<Set> {
    type GetArguments = ();
}

impl GetObject for PushSubscription<Get> {
    type GetArguments = ();
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `PushSubscription<Get>` id accessors against
    //! RFC 8620 §7.3 (Push Subscription objects). No server, no socket,
    //! no async.

    use super::*;
    use serde_json::json;

    fn subscription(value: serde_json::Value) -> PushSubscription<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// RFC 8620 §7.3 — `id` borrows the subscription id and `take_id`
    /// moves it out, leaving the record without one.
    #[test]
    fn take_id_moves_the_id_out() {
        let mut sub = subscription(json!({ "id": "P1" }));

        assert_eq!(sub.id(), Some("P1"));
        assert_eq!(sub.take_id(), "P1");
        assert_eq!(
            sub.id(),
            None,
            "taking the id must leave the record without one"
        );
    }

    /// RFC 8620 §7.3 — a record that never had an id yields the empty
    /// string rather than panicking: the accessor is total.
    #[test]
    fn take_id_on_a_record_without_one_is_empty() {
        let mut sub = subscription(json!({ "url": "https://example.com/push" }));

        assert_eq!(sub.id(), None);
        assert_eq!(sub.take_id(), "");
    }
}
