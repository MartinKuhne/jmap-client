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

use super::{
    Email, EmailAddress, EmailAddressGroup, EmailBodyPart, EmailBodyValue, EmailHeader,
    GetArguments, Header, HeaderValue,
};

impl Email<Get> {
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn take_id(&mut self) -> String {
        self.id.take().unwrap_or_default()
    }

    pub fn blob_id(&self) -> Option<&str> {
        self.blob_id.as_deref()
    }

    pub fn take_blob_id(&mut self) -> String {
        self.blob_id.take().unwrap_or_default()
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub fn take_thread_id(&mut self) -> Option<String> {
        self.thread_id.take()
    }

    pub fn mailbox_ids(&self) -> Vec<&str> {
        self.mailbox_ids
            .as_ref()
            .map(|m| {
                m.iter()
                    .filter(|(_, v)| **v)
                    .map(|(k, _)| k.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn keywords(&self) -> Vec<&str> {
        self.keywords
            .as_ref()
            .map(|k| {
                k.iter()
                    .filter(|(_, v)| **v)
                    .map(|(k, _)| k.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn size(&self) -> usize {
        self.size.unwrap_or(0)
    }

    pub fn received_at(&self) -> Option<i64> {
        self.received_at.as_ref().map(|r| r.timestamp())
    }

    pub fn message_id(&self) -> Option<&[String]> {
        self.message_id.as_deref()
    }

    pub fn in_reply_to(&self) -> Option<&[String]> {
        self.in_reply_to.as_deref()
    }

    pub fn references(&self) -> Option<&[String]> {
        self.references.as_deref()
    }

    pub fn sender(&self) -> Option<&[EmailAddress]> {
        self.sender.as_deref()
    }

    pub fn take_sender(&mut self) -> Option<Vec<EmailAddress>> {
        self.sender.take()
    }

    pub fn from(&self) -> Option<&[EmailAddress]> {
        self.from.as_deref()
    }

    pub fn take_from(&mut self) -> Option<Vec<EmailAddress>> {
        self.from.take()
    }

    pub fn reply_to(&self) -> Option<&[EmailAddress]> {
        self.reply_to.as_deref()
    }

    pub fn take_reply_to(&mut self) -> Option<Vec<EmailAddress>> {
        self.reply_to.take()
    }

    pub fn to(&self) -> Option<&[EmailAddress]> {
        self.to.as_deref()
    }

    pub fn take_to(&mut self) -> Option<Vec<EmailAddress>> {
        self.to.take()
    }

    pub fn cc(&self) -> Option<&[EmailAddress]> {
        self.cc.as_deref()
    }

    pub fn take_cc(&mut self) -> Option<Vec<EmailAddress>> {
        self.cc.take()
    }

    pub fn bcc(&self) -> Option<&[EmailAddress]> {
        self.bcc.as_deref()
    }

    pub fn take_bcc(&mut self) -> Option<Vec<EmailAddress>> {
        self.bcc.take()
    }

    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    pub fn take_subject(&mut self) -> Option<String> {
        self.subject.take()
    }

    pub fn sent_at(&self) -> Option<i64> {
        self.sent_at.as_ref().map(|v| v.timestamp())
    }

    pub fn body_structure(&self) -> Option<&EmailBodyPart> {
        self.body_structure.as_deref()
    }

    pub fn body_value(&self, id: &str) -> Option<&EmailBodyValue> {
        self.body_values.as_ref().and_then(|v| v.get(id))
    }

    pub fn text_body(&self) -> Option<&[EmailBodyPart]> {
        self.text_body.as_deref()
    }

    pub fn html_body(&self) -> Option<&[EmailBodyPart]> {
        self.html_body.as_deref()
    }

    pub fn attachments(&self) -> Option<&[EmailBodyPart]> {
        self.attachments.as_deref()
    }

    pub fn has_attachment(&self) -> bool {
        *self.has_attachment.as_ref().unwrap_or(&false)
    }

    pub fn header(&self, id: &Header) -> Option<&HeaderValue> {
        self.headers.get(id).and_then(|v| v.as_ref())
    }

    pub fn has_header(&self, id: &Header) -> bool {
        self.headers.contains_key(id)
    }

    pub fn preview(&self) -> Option<&str> {
        self.preview.as_deref()
    }

    pub fn take_preview(&mut self) -> Option<String> {
        self.preview.take()
    }

    #[cfg(feature = "debug")]
    pub fn into_test(self) -> super::TestEmail {
        self.into()
    }
}

impl EmailBodyPart<Get> {
    pub fn part_id(&self) -> Option<&str> {
        self.part_id.as_deref()
    }

    pub fn blob_id(&self) -> Option<&str> {
        self.blob_id.as_deref()
    }

    pub fn size(&self) -> usize {
        *self.size.as_ref().unwrap_or(&0)
    }

    pub fn headers(&self) -> Option<&[EmailHeader]> {
        self.headers.as_deref()
    }

    pub fn header(&self, id: &Header) -> Option<&HeaderValue> {
        self.header.as_ref().and_then(|v| v.get(id))
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn charset(&self) -> Option<&str> {
        self.charset.as_deref()
    }

    pub fn content_type(&self) -> Option<&str> {
        self.type_.as_deref()
    }

    pub fn content_disposition(&self) -> Option<&str> {
        self.disposition.as_deref()
    }

    pub fn content_id(&self) -> Option<&str> {
        self.cid.as_deref()
    }

    pub fn content_language(&self) -> Option<&[String]> {
        self.language.as_deref()
    }

    pub fn content_location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    pub fn sub_parts(&self) -> Option<&[EmailBodyPart]> {
        self.sub_parts.as_deref()
    }
}

impl EmailBodyValue<Get> {
    pub fn value(&self) -> &str {
        self.value.as_str()
    }

    pub fn is_encoding_problem(&self) -> bool {
        self.is_encoding_problem.unwrap_or(false)
    }

    pub fn is_truncated(&self) -> bool {
        self.is_truncated.unwrap_or(false)
    }
}

impl EmailAddress<Get> {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn email(&self) -> &str {
        self.email.as_str()
    }

    pub fn unwrap(self) -> (String, Option<String>) {
        (self.email, self.name)
    }
}

impl EmailAddressGroup<Get> {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn addresses(&self) -> &[EmailAddress] {
        self.addresses.as_ref()
    }
}

impl EmailHeader<Get> {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn value(&self) -> &str {
        self.value.as_str()
    }
}

impl GetObject for Email<Set> {
    type GetArguments = GetArguments;
}

impl GetObject for Email<Get> {
    type GetArguments = GetArguments;
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Email<Get>` read surface: the borrowing
    //! accessors, the draining `take_*` family, the two `true`-filtering
    //! map accessors and the body/address/header helper types.
    //! Fixtures follow RFC 8621 §4.1.2 (the `Email/get` example) and
    //! §4.1.4 (body structure). Deserialised in place — no server, no
    //! socket, no async.

    use super::*;
    use crate::email::HeaderForm;
    use serde_json::{json, Value};

    /// RFC 8621 §4.1.2 — a full `Email` record as it arrives on the wire.
    fn rfc_email() -> Value {
        json!({
            "id": "M8504",
            "blobId": "a2f9ff3a-123e-1234-80a1-371358b4bce8",
            "threadId": "Jf9ms0g1AAA4NcQ",
            "mailboxIds": {
                "f4da56b1-5ec1-4bb9-9d97-0c46e4fbdb01": true
            },
            "keywords": { "$seen": true, "$flagged": true },
            "size": 4456,
            "receivedAt": "2018-10-30T13:13:20Z",
            "messageId": ["<abc123@example.com>"],
            "inReplyTo": ["<other@example.com>"],
            "references": ["<a@example.com>", "<b@example.com>"],
            "sender": [{ "name": "Alice", "email": "alice@example.com" }],
            "from": [{ "name": "Alice", "email": "alice@example.com" }],
            "to": [
                { "name": "Bob", "email": "bob@example.com" },
                { "email": "carol@example.com" }
            ],
            "cc": [{ "name": "Dan", "email": "dan@example.com" }],
            "bcc": [{ "name": "Eve", "email": "eve@example.com" }],
            "replyTo": [{ "name": "Alice", "email": "alice@example.com" }],
            "subject": "Hello, World!",
            "sentAt": "2018-10-30T13:13:19Z",
            "bodyStructure": {
                "blobId": "a2f9ff3a-123e-1234-80a1-371358b4bce8",
                "size": 4456,
                "type": "multipart/mixed",
                "subParts": [
                    {
                        "partId": "1",
                        "blobId": "b2f9ff3a-123e-1234-80a1-371358b4bce8",
                        "size": 458,
                        "type": "text/plain",
                        "charset": "utf-8"
                    },
                    {
                        "partId": "2",
                        "blobId": "c2f9ff3a-123e-1234-80a1-371358b4bce8",
                        "size": 12458,
                        "name": "attachment.png",
                        "type": "image/png",
                        "disposition": "attachment",
                        "cid": "c2f9ff3a-123e-1234-80a1-371358b4bce8",
                        "language": ["en"],
                        "location": "https://example.com/attachment.png",
                        "headers": [
                            { "name": "Content-Type", "value": "image/png" }
                        ]
                    }
                ]
            },
            "bodyValues": {
                "1": {
                    "value": "Hi, this is the plain text body.",
                    "isEncodingProblem": false,
                    "isTruncated": false
                },
                "2": {
                    "value": "<p>Truncated…",
                    "isEncodingProblem": true,
                    "isTruncated": true
                }
            },
            "textBody": [
                {
                    "partId": "1",
                    "blobId": "b2f9ff3a-123e-1234-80a1-371358b4bce8",
                    "size": 458,
                    "type": "text/plain",
                    "charset": "utf-8"
                }
            ],
            "htmlBody": [
                {
                    "partId": "3",
                    "blobId": "d2f9ff3a-123e-1234-80a1-371358b4bce8",
                    "size": 902,
                    "type": "text/html",
                    "charset": "utf-8"
                }
            ],
            "attachments": [
                {
                    "partId": "2",
                    "blobId": "c2f9ff3a-123e-1234-80a1-371358b4bce8",
                    "size": 12458,
                    "name": "attachment.png",
                    "type": "image/png",
                    "disposition": "attachment"
                }
            ],
            "hasAttachment": true,
            "preview": "Hi, this is the plain text body.",
            "header:Message-ID:asText": "<abc123@example.com>"
        })
    }

    fn get(value: Value) -> Email<Get> {
        serde_json::from_value(value).unwrap()
    }

    /// Every borrowing accessor must read back exactly what the fixture
    /// carries, under the RFC 8621 §4.1.2 names.
    #[test]
    fn email_get_borrowing_accessors_read_the_rfc_record() {
        let email = get(rfc_email());

        assert_eq!(email.id(), Some("M8504"));
        assert_eq!(
            email.blob_id(),
            Some("a2f9ff3a-123e-1234-80a1-371358b4bce8")
        );
        assert_eq!(email.thread_id(), Some("Jf9ms0g1AAA4NcQ"));
        assert_eq!(email.size(), 4456);
        // 2018-10-30T13:13:20Z
        assert_eq!(email.received_at(), Some(1_540_905_200));
        // 2018-10-30T13:13:19Z
        assert_eq!(email.sent_at(), Some(1_540_905_199));
        assert_eq!(email.subject(), Some("Hello, World!"));
        assert_eq!(email.preview(), Some("Hi, this is the plain text body."));
        assert!(email.has_attachment());

        assert_eq!(
            email.message_id(),
            Some(&["<abc123@example.com>".to_string()][..])
        );
        assert_eq!(
            email.in_reply_to(),
            Some(&["<other@example.com>".to_string()][..])
        );
        assert_eq!(
            email.references(),
            Some(&["<a@example.com>".to_string(), "<b@example.com>".to_string()][..])
        );

        assert_eq!(email.sender().unwrap().len(), 1);
        assert_eq!(email.sender().unwrap()[0].email(), "alice@example.com");
        assert_eq!(email.sender().unwrap()[0].name(), Some("Alice"));
        assert_eq!(email.from().unwrap()[0].email(), "alice@example.com");
        assert_eq!(email.reply_to().unwrap()[0].email(), "alice@example.com");
        assert_eq!(email.to().unwrap().len(), 2);
        assert_eq!(email.to().unwrap()[1].name(), None);
        assert_eq!(email.to().unwrap()[1].email(), "carol@example.com");
        assert_eq!(email.cc().unwrap()[0].email(), "dan@example.com");
        assert_eq!(email.bcc().unwrap()[0].email(), "eve@example.com");

        assert_eq!(email.text_body().unwrap().len(), 1);
        assert_eq!(email.text_body().unwrap()[0].part_id(), Some("1"));
        assert_eq!(email.html_body().unwrap().len(), 1);
        assert_eq!(
            email.html_body().unwrap()[0].content_type(),
            Some("text/html")
        );
        assert_eq!(email.attachments().unwrap().len(), 1);
        assert_eq!(
            email.attachments().unwrap()[0].name(),
            Some("attachment.png")
        );
    }

    /// RFC 8621 §4.1.2 — the flattened `header:<name>:as<Form>` arm of the
    /// record is reachable through `header`/`has_header`, and an absent
    /// header is `None` but still distinguishable from a present-but-null one.
    #[test]
    fn header_lookup_uses_the_flattened_header_map() {
        let email = get(rfc_email());
        let present = Header {
            name: "Message-ID".to_string(),
            form: HeaderForm::Text,
            all: false,
        };
        let absent = Header {
            name: "X-Nope".to_string(),
            form: HeaderForm::Text,
            all: false,
        };

        assert!(email.has_header(&present));
        assert!(!email.has_header(&absent));
        // `HeaderValue` has no `PartialEq`, so compare on the wire form.
        assert_eq!(
            serde_json::to_value(email.header(&present).unwrap()).unwrap(),
            json!("<abc123@example.com>")
        );
        assert!(email.header(&absent).is_none());
    }

    /// `mailbox_ids`/`keywords` are *membership* views: only entries whose
    /// value is `true` come back, and the borrow dies with the record.
    #[test]
    fn mailbox_ids_and_keywords_only_yield_true_entries() {
        let email = get(json!({
            "id": "M1",
            "mailboxIds": { "mb-yes": true, "mb-no": false },
            "keywords": { "$seen": true, "$junk": false, "$flagged": true }
        }));

        let mut boxes = email.mailbox_ids();
        boxes.sort_unstable();
        assert_eq!(boxes, vec!["mb-yes"]);

        let mut words = email.keywords();
        words.sort_unstable();
        assert_eq!(words, vec!["$flagged", "$seen"]);
    }

    /// The draining `take_*` family must hand the value out *and* leave the
    /// field empty, so a second call sees the default.
    #[test]
    fn take_accessors_drain_the_underlying_field() {
        let mut email = get(rfc_email());

        assert_eq!(email.take_id(), "M8504");
        assert_eq!(email.take_blob_id(), "a2f9ff3a-123e-1234-80a1-371358b4bce8");
        assert_eq!(email.take_thread_id(), Some("Jf9ms0g1AAA4NcQ".to_string()));
        assert_eq!(email.take_subject(), Some("Hello, World!".to_string()));
        assert_eq!(
            email.take_preview(),
            Some("Hi, this is the plain text body.".to_string())
        );

        assert_eq!(email.take_sender().unwrap().len(), 1);
        assert_eq!(email.take_from().unwrap().len(), 1);
        assert_eq!(email.take_reply_to().unwrap().len(), 1);
        assert_eq!(email.take_to().unwrap().len(), 2);
        assert_eq!(email.take_cc().unwrap().len(), 1);
        assert_eq!(email.take_bcc().unwrap().len(), 1);

        // Everything drained: the borrowing views now see nothing.
        assert_eq!(email.id(), None);
        assert_eq!(email.take_id(), "");
        assert_eq!(email.blob_id(), None);
        assert_eq!(email.take_blob_id(), "");
        assert_eq!(email.thread_id(), None);
        assert_eq!(email.take_thread_id(), None);
        assert_eq!(email.subject(), None);
        assert_eq!(email.take_subject(), None);
        assert_eq!(email.preview(), None);
        assert_eq!(email.take_preview(), None);
        assert!(email.sender().is_none());
        assert!(email.from().is_none());
        assert!(email.reply_to().is_none());
        assert!(email.to().is_none());
        assert!(email.cc().is_none());
        assert!(email.bcc().is_none());
    }

    /// A record that omits every optional member must still read cleanly,
    /// falling back to the documented zero values rather than panicking.
    #[test]
    fn absent_optionals_fall_back_to_zero_values() {
        let email = get(json!({ "id": "M0" }));

        assert_eq!(email.size(), 0);
        assert!(!email.has_attachment());
        assert!(email.received_at().is_none());
        assert!(email.sent_at().is_none());
        assert!(email.message_id().is_none());
        assert!(email.in_reply_to().is_none());
        assert!(email.references().is_none());
        assert!(email.sender().is_none());
        assert!(email.subject().is_none());
        assert!(email.body_structure().is_none());
        assert!(email.text_body().is_none());
        assert!(email.html_body().is_none());
        assert!(email.attachments().is_none());
        assert!(email.body_value("1").is_none());
        assert_eq!(email.mailbox_ids(), Vec::<&str>::new());
        assert_eq!(email.keywords(), Vec::<&str>::new());
    }

    /// RFC 8621 §4.1.4 — the body-structure tree exposes every member
    /// under its own accessor, and the nested `subParts` keep their type.
    #[test]
    fn body_part_accessors_walk_the_structure() {
        let email = get(rfc_email());
        let root = email.body_structure().unwrap();

        assert_eq!(root.part_id(), None);
        assert_eq!(root.blob_id(), Some("a2f9ff3a-123e-1234-80a1-371358b4bce8"));
        assert_eq!(root.size(), 4456);
        assert_eq!(root.content_type(), Some("multipart/mixed"));
        assert!(root.name().is_none());
        assert!(root.charset().is_none());
        assert!(root.content_disposition().is_none());
        assert!(root.content_id().is_none());
        assert!(root.content_language().is_none());
        assert!(root.content_location().is_none());
        assert!(root.headers().is_none());
        assert!(root
            .header(&Header {
                name: "Content-Type".to_string(),
                form: HeaderForm::Text,
                all: false,
            })
            .is_none());

        let parts = root.sub_parts().unwrap();
        assert_eq!(parts.len(), 2);

        let leaf = &parts[1];
        assert_eq!(leaf.part_id(), Some("2"));
        assert_eq!(leaf.name(), Some("attachment.png"));
        assert_eq!(leaf.content_type(), Some("image/png"));
        assert_eq!(leaf.content_disposition(), Some("attachment"));
        assert_eq!(
            leaf.content_id(),
            Some("c2f9ff3a-123e-1234-80a1-371358b4bce8")
        );
        assert_eq!(leaf.content_language(), Some(&["en".to_string()][..]));
        assert_eq!(
            leaf.content_location(),
            Some("https://example.com/attachment.png")
        );
        assert_eq!(leaf.size(), 12458);

        let headers = leaf.headers().unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name(), "Content-Type");
        assert_eq!(headers[0].value(), "image/png");
    }

    /// A bare `EmailBodyPart` with nothing set reads as zero/None, not a panic.
    #[test]
    fn empty_body_part_reads_as_zero() {
        let part: EmailBodyPart<Get> = serde_json::from_value(json!({})).unwrap();
        assert_eq!(part.size(), 0);
        assert!(part.part_id().is_none());
        assert!(part.blob_id().is_none());
        assert!(part.sub_parts().is_none());
    }

    /// RFC 8621 §4.1.3 — `bodyValues` entries carry the text plus the two
    /// optional problem flags, defaulting to `false` when omitted.
    #[test]
    fn body_value_accessors_report_flags_and_text() {
        let email = get(rfc_email());

        let clean = email.body_value("1").unwrap();
        assert_eq!(clean.value(), "Hi, this is the plain text body.");
        assert!(!clean.is_encoding_problem());
        assert!(!clean.is_truncated());

        let broken = email.body_value("2").unwrap();
        assert_eq!(broken.value(), "<p>Truncated…");
        assert!(broken.is_encoding_problem());
        assert!(broken.is_truncated());

        assert!(email.body_value("3").is_none());
    }

    /// RFC 8621 §4.1 — an address is `{ name?, email }`; `unwrap` hands
    /// back the `(email, name)` pair by value.
    #[test]
    fn email_address_accessors_and_unwrap() {
        let named: EmailAddress<Get> =
            serde_json::from_value(json!({ "name": "Alice", "email": "alice@example.com" }))
                .unwrap();
        assert_eq!(named.name(), Some("Alice"));
        assert_eq!(named.email(), "alice@example.com");
        let (email, name) = named.unwrap();
        assert_eq!(email, "alice@example.com");
        assert_eq!(name, Some("Alice".to_string()));

        let bare: EmailAddress<Get> =
            serde_json::from_value(json!({ "email": "carol@example.com" })).unwrap();
        assert_eq!(bare.name(), None);
        assert_eq!(bare.email(), "carol@example.com");
        let (email, name) = bare.unwrap();
        assert_eq!(email, "carol@example.com");
        assert_eq!(name, None);
    }

    /// RFC 8621 §4.1 — a grouped address (`asGroupedAddresses`) is a
    /// display name plus the member addresses.
    #[test]
    fn email_address_group_accessors() {
        let group: EmailAddressGroup<Get> = serde_json::from_value(json!({
            "name": "Team",
            "addresses": [
                { "name": "Alice", "email": "alice@example.com" },
                { "email": "bob@example.com" }
            ]
        }))
        .unwrap();

        assert_eq!(group.name(), Some("Team"));
        assert_eq!(group.addresses().len(), 2);
        assert_eq!(group.addresses()[0].name(), Some("Alice"));
        assert_eq!(group.addresses()[1].name(), None);
        assert_eq!(group.addresses()[1].email(), "bob@example.com");
    }

    /// The `headers` array of a body part is a flat `{ name, value }` pair.
    #[test]
    fn email_header_accessors() {
        let header: EmailHeader<Get> =
            serde_json::from_value(json!({ "name": "Content-Type", "value": "image/png" }))
                .unwrap();
        assert_eq!(header.name(), "Content-Type");
        assert_eq!(header.value(), "image/png");
    }
}
