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
use crate::{
    core::set::{from_timestamp, SetObject},
    email_submission::SetArguments,
    DataType, Get, Set,
};
use encodify::base64::URL_SAFE;

impl PushSubscription<Set> {
    pub fn device_client_id(&mut self, device_client_id: impl Into<String>) -> &mut Self {
        self.device_client_id = Some(device_client_id.into());
        self
    }

    pub fn url(&mut self, url: impl Into<String>) -> &mut Self {
        self.url = Some(url.into());
        self
    }

    pub fn verification_code(&mut self, verification_code: impl Into<String>) -> &mut Self {
        self.verification_code = Some(verification_code.into());
        self
    }

    pub fn keys(&mut self, keys: Keys) -> &mut Self {
        self.keys = Some(keys);
        self
    }

    pub fn expires(&mut self, expires: i64) -> &mut Self {
        self.expires = Some(from_timestamp(expires));
        self
    }

    pub fn types(&mut self, types: Option<impl IntoIterator<Item = DataType>>) -> &mut Self {
        self.types = types.map(|s| s.into_iter().collect());
        self
    }
}

impl SetObject for PushSubscription<Set> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        PushSubscription {
            _create_id,
            _state: Default::default(),
            id: None,
            device_client_id: None,
            url: None,
            keys: None,
            verification_code: None,
            expires: None,
            types: Vec::with_capacity(0).into(),
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }
}

impl SetObject for PushSubscription<Get> {
    type SetArguments = SetArguments;

    fn new(_create_id: Option<usize>) -> Self {
        unimplemented!()
    }

    fn create_id(&self) -> Option<String> {
        None
    }
}

impl Keys {
    pub fn new(p256dh: &[u8], auth: &[u8]) -> Self {
        Keys {
            p256dh: URL_SAFE.encode(p256dh),
            auth: URL_SAFE.encode(auth),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `<Get>` arm of `SetObject` on
    //! `PushSubscription` (RFC 8620 §7.3). A fetched subscription is
    //! not a patch, and the trait arm encodes that.

    use super::*;
    use serde_json::json;

    /// RFC 8620 §7.3 — a fetched subscription has no creation id, so the
    /// generic `SetRequest` machinery cannot accidentally treat it as a
    /// new subscription.
    #[test]
    fn a_fetched_subscription_has_no_creation_id() {
        let subscription: PushSubscription<Get> = serde_json::from_value(json!({
            "id": "P1",
            "deviceClientId": "a031-9af8-bec4-3d4f",
            "url": "https://push.example.com/context/1234"
        }))
        .unwrap();

        assert_eq!(subscription.create_id(), None);
    }

    /// RFC 8620 §7.3 — asking a `<Get>` flavour to construct itself is a
    /// type-level mistake, and the arm refuses loudly rather than handing
    /// back an empty patch.
    #[test]
    #[should_panic(expected = "not implemented")]
    fn a_fetched_subscription_refuses_to_construct_itself() {
        let _ = <PushSubscription<Get> as SetObject>::new(Some(0));
    }
}
