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

use super::{
    Email, EmailAddress, EmailAddressGroup, EmailBodyPart, EmailBodyValue, EmailHeader, Header,
    HeaderValue,
};
use crate::{
    core::{
        request::ResultReference,
        set::{from_timestamp, SetObject},
    },
    Get, Set,
};
use ahash::AHashMap;

impl Email<Set> {
    pub fn mailbox_ids<T, U>(&mut self, mailbox_ids: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.mailbox_ids = Some(mailbox_ids.into_iter().map(|s| (s.into(), true)).collect());
        self.mailbox_ids_ref = None;
        self
    }

    pub fn mailbox_ids_ref(&mut self, reference: ResultReference) -> &mut Self {
        self.mailbox_ids_ref = reference.into();
        self.mailbox_ids = None;
        self
    }

    pub fn mailbox_id(&mut self, mailbox_id: &str, set: bool) -> &mut Self {
        self.mailbox_ids = None;
        self.patch
            .get_or_insert_with(AHashMap::new)
            .insert(format!("mailboxIds/{}", mailbox_id), set);
        self
    }

    pub fn keywords<T, U>(&mut self, keywords: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.keywords = Some(keywords.into_iter().map(|s| (s.into(), true)).collect());
        self
    }

    pub fn keyword(&mut self, keyword: &str, set: bool) -> &mut Self {
        self.keywords = None;
        self.patch
            .get_or_insert_with(AHashMap::new)
            .insert(format!("keywords/{}", keyword), set);
        self
    }

    pub fn message_id<T, U>(&mut self, message_id: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.message_id = Some(message_id.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn in_reply_to<T, U>(&mut self, in_reply_to: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.in_reply_to = Some(in_reply_to.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn references<T, U>(&mut self, references: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.references = Some(references.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn sender<T, U>(&mut self, sender: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.sender = Some(sender.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn from<T, U>(&mut self, from: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.from = Some(from.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn to<T, U>(&mut self, to: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.to = Some(to.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn cc<T, U>(&mut self, cc: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.cc = Some(cc.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn bcc<T, U>(&mut self, bcc: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.bcc = Some(bcc.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn reply_to<T, U>(&mut self, reply_to: T) -> &mut Self
    where
        T: IntoIterator<Item = U>,
        U: Into<EmailAddress>,
    {
        self.reply_to = Some(reply_to.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn subject(&mut self, subject: impl Into<String>) -> &mut Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn sent_at(&mut self, sent_at: i64) -> &mut Self {
        self.sent_at = Some(from_timestamp(sent_at));
        self
    }

    pub fn body_structure(&mut self, body_structure: EmailBodyPart) -> &mut Self {
        self.body_structure = Some(body_structure.into());
        self
    }

    pub fn body_value(&mut self, id: String, body_value: impl Into<EmailBodyValue>) -> &mut Self {
        self.body_values
            .get_or_insert_with(AHashMap::new)
            .insert(id, body_value.into());
        self
    }

    pub fn text_body(&mut self, text_body: impl Into<EmailBodyPart<Get>>) -> &mut Self {
        self.text_body
            .get_or_insert_with(Vec::new)
            .push(text_body.into());
        self
    }

    pub fn html_body(&mut self, html_body: impl Into<EmailBodyPart<Get>>) -> &mut Self {
        self.html_body
            .get_or_insert_with(Vec::new)
            .push(html_body.into());
        self
    }

    pub fn attachment(&mut self, attachment: impl Into<EmailBodyPart<Get>>) -> &mut Self {
        self.attachments
            .get_or_insert_with(Vec::new)
            .push(attachment.into());
        self
    }

    pub fn header(&mut self, header: Header, value: impl Into<HeaderValue>) -> &mut Self {
        self.headers.insert(header, Some(value.into()));
        self
    }

    pub fn received_at(&mut self, received_at: i64) -> &mut Self {
        self.received_at = Some(from_timestamp(received_at));
        self
    }
}

impl SetObject for Email<Set> {
    type SetArguments = ();

    fn new(_create_id: Option<usize>) -> Email<Set> {
        Email {
            _create_id,
            _state: Default::default(),
            id: Default::default(),
            blob_id: Default::default(),
            thread_id: Default::default(),
            mailbox_ids: Default::default(),
            mailbox_ids_ref: Default::default(),
            keywords: Default::default(),
            size: Default::default(),
            received_at: Default::default(),
            message_id: Default::default(),
            in_reply_to: Default::default(),
            references: Default::default(),
            sender: Default::default(),
            from: Default::default(),
            to: Default::default(),
            cc: Default::default(),
            bcc: Default::default(),
            reply_to: Default::default(),
            subject: Default::default(),
            sent_at: Default::default(),
            body_structure: Default::default(),
            body_values: Default::default(),
            text_body: Default::default(),
            html_body: Default::default(),
            attachments: Default::default(),
            has_attachment: Default::default(),
            preview: Default::default(),
            headers: Default::default(),
            patch: Default::default(),
        }
    }

    fn create_id(&self) -> Option<String> {
        self._create_id.map(|id| format!("c{}", id))
    }

    fn set_id(&mut self, id: Option<String>) {
        self.id = id;
    }
}

impl SetObject for Email<Get> {
    type SetArguments = ();

    fn new(_create_id: Option<usize>) -> Email<Get> {
        unimplemented!()
    }

    fn create_id(&self) -> Option<String> {
        None
    }
}

impl EmailBodyPart {
    pub fn new() -> EmailBodyPart<Set> {
        EmailBodyPart {
            part_id: None,
            blob_id: None,
            size: None,
            headers: None,
            name: None,
            type_: None,
            charset: None,
            disposition: None,
            cid: None,
            language: None,
            location: None,
            sub_parts: None,
            header: None,
            _state: Default::default(),
        }
    }
}

impl From<EmailBodyPart<Set>> for EmailBodyPart<Get> {
    fn from(part: EmailBodyPart<Set>) -> Self {
        EmailBodyPart {
            part_id: part.part_id,
            blob_id: part.blob_id,
            size: part.size,
            headers: part.headers,
            name: part.name,
            type_: part.type_,
            charset: part.charset,
            disposition: part.disposition,
            cid: part.cid,
            language: part.language,
            location: part.location,
            sub_parts: part.sub_parts,
            header: part.header,
            _state: Default::default(),
        }
    }
}

impl EmailBodyPart<Set> {
    pub fn part_id(mut self, part_id: impl Into<String>) -> Self {
        self.part_id = Some(part_id.into());
        self
    }

    pub fn blob_id(mut self, blob_id: impl Into<String>) -> Self {
        self.blob_id = Some(blob_id.into());
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.type_ = Some(content_type.into());
        self
    }

    pub fn content_id(mut self, content_id: impl Into<String>) -> Self {
        self.cid = Some(content_id.into());
        self
    }

    pub fn content_language<T, U>(mut self, content_language: T) -> Self
    where
        T: IntoIterator<Item = U>,
        U: Into<String>,
    {
        self.language = Some(content_language.into_iter().map(|v| v.into()).collect());
        self
    }

    pub fn content_location(mut self, content_location: impl Into<String>) -> Self {
        self.location = Some(content_location.into());
        self
    }

    pub fn sub_part(mut self, sub_part: EmailBodyPart) -> Self {
        self.sub_parts.get_or_insert_with(Vec::new).push(sub_part);
        self
    }
}

impl From<String> for EmailBodyValue {
    fn from(value: String) -> Self {
        EmailBodyValue {
            value,
            is_encoding_problem: None,
            is_truncated: None,
            _state: Default::default(),
        }
    }
}

impl From<&str> for EmailBodyValue {
    fn from(value: &str) -> Self {
        EmailBodyValue {
            value: value.to_string(),
            is_encoding_problem: None,
            is_truncated: None,
            _state: Default::default(),
        }
    }
}

impl EmailAddress {
    pub fn new(email: String) -> EmailAddress<Set> {
        EmailAddress {
            _state: Default::default(),
            name: None,
            email,
        }
    }
}

impl EmailAddress<Set> {
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}

impl From<String> for EmailAddress {
    fn from(email: String) -> Self {
        EmailAddress {
            _state: Default::default(),
            name: None,
            email,
        }
    }
}

impl From<(String, String)> for EmailAddress {
    fn from(parts: (String, String)) -> Self {
        EmailAddress {
            _state: Default::default(),
            name: parts.0.into(),
            email: parts.1,
        }
    }
}

impl From<&str> for EmailAddress {
    fn from(email: &str) -> Self {
        EmailAddress {
            _state: Default::default(),
            name: None,
            email: email.to_string(),
        }
    }
}

impl From<(&str, &str)> for EmailAddress {
    fn from(parts: (&str, &str)) -> Self {
        EmailAddress {
            _state: Default::default(),
            name: parts.0.to_string().into(),
            email: parts.1.to_string(),
        }
    }
}

impl EmailAddressGroup {
    pub fn new() -> EmailAddressGroup<Set> {
        EmailAddressGroup {
            _state: Default::default(),
            name: None,
            addresses: Vec::new(),
        }
    }
}

impl EmailAddressGroup<Set> {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn address(mut self, address: impl Into<EmailAddress>) -> Self {
        self.addresses.push(address.into());
        self
    }
}

impl EmailHeader {
    pub fn new(name: String, value: String) -> EmailHeader<Set> {
        EmailHeader {
            _state: Default::default(),
            name,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Email<Set>` patch builders (RFC 8621 §5.2 on
    //! top of RFC 8620 §5.3), the `EmailBodyPart` value builder, and the
    //! `From` conversions that let callers write addresses and body
    //! values without naming the types. No server, no socket, no async.

    use super::*;
    use crate::core::set::SetObject;
    use crate::email::HeaderForm;
    use crate::Method;
    use serde_json::json;

    fn fresh() -> Email<Set> {
        Email::<Set>::new(None)
    }

    /// An untouched patch contributes nothing to the request body.
    #[test]
    fn a_fresh_email_patch_serialises_as_an_empty_object() {
        assert_eq!(serde_json::to_value(fresh()).unwrap(), json!({}));
        assert_eq!(
            serde_json::to_value(Email::<Set>::new(Some(3))).unwrap(),
            json!({})
        );
    }

    /// RFC 8620 §5.3 — a single-mailbox / single-keyword change is sent as
    /// a dotted path inside the patch, never as a whole-value member.
    #[test]
    fn mailbox_id_and_keyword_write_dotted_patch_paths() {
        let mut email = fresh();
        email.mailbox_id("mb-1", true).keyword("$seen", true);
        email.keyword("$junk", false);

        assert_eq!(
            serde_json::to_value(&email).unwrap(),
            json!({
                "mailboxIds/mb-1": true,
                "keywords/$seen": true,
                "keywords/$junk": false
            })
        );
        // The dotted form and the whole-value form never coexist.
        assert!(email.mailbox_ids.is_none());
        assert!(email.keywords.is_none());
    }

    /// RFC 8621 §5.2 — `mailboxIds` may be given as a whole value or as a
    /// result reference; each form clears the other.
    #[test]
    fn whole_value_mailbox_ids_and_the_reference_are_exclusive() {
        let mut email = fresh();
        email.mailbox_ids(["mb-1", "mb-2"]);
        let value = serde_json::to_value(&email).unwrap();
        let mut ids = value["mailboxIds"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            ids,
            vec![
                ("mb-1".to_string(), json!(true)),
                ("mb-2".to_string(), json!(true))
            ]
        );
        assert!(email.mailbox_ids_ref.is_none());

        email.mailbox_ids_ref(ResultReference::new(Method::QueryMailbox, 1, "/ids"));
        let value = serde_json::to_value(&email).unwrap();
        assert!(value.get("mailboxIds").is_none());
        assert_eq!(
            value["#mailboxIds"],
            json!({ "resultOf": "s1", "name": "Mailbox/query", "path": "/ids" })
        );

        email.mailbox_ids(["mb-3"]);
        assert!(serde_json::to_value(&email)
            .unwrap()
            .get("#mailboxIds")
            .is_none());
    }

    /// RFC 8621 §5.2 — the scalar and message-id-list builders land under
    /// their own wire names.
    #[test]
    fn scalar_and_list_builders_land_under_their_wire_names() {
        let mut email = fresh();
        email
            .subject("Hello, World!")
            .sent_at(1_720_000_000)
            .received_at(1_720_000_001)
            .message_id(["<abc123@example.com>"])
            .in_reply_to(["<other@example.com>"])
            .references(["<a@example.com>", "<b@example.com>"]);

        assert_eq!(
            serde_json::to_value(&email).unwrap(),
            json!({
                "subject": "Hello, World!",
                "sentAt": "2024-07-03T09:46:40Z",
                "receivedAt": "2024-07-03T09:46:41Z",
                "messageId": ["<abc123@example.com>"],
                "inReplyTo": ["<other@example.com>"],
                "references": ["<a@example.com>", "<b@example.com>"]
            })
        );
    }

    /// Every address-list builder accepts the `EmailAddress` `From` shapes,
    /// so callers can mix bare addresses and named ones without naming the
    /// type.
    #[test]
    fn address_list_builders_accept_every_from_shape() {
        let mut email = fresh();
        email
            .sender([EmailAddress::from("sender@example.com")])
            .from([EmailAddress::from((
                "From Name".to_string(),
                "from@example.com".to_string(),
            ))])
            .to([
                EmailAddress::from(("Alice".to_string(), "alice@example.com".to_string())),
                EmailAddress::from(("Bob", "bob@example.com")),
                EmailAddress::from("bare@example.com"),
            ])
            .cc(["cc@example.com"])
            .bcc([String::from("bcc@example.com")])
            .reply_to([("Reply", "reply@example.com")]);

        let value = serde_json::to_value(&email).unwrap();
        assert_eq!(
            value["sender"],
            json!([{ "name": null, "email": "sender@example.com" }])
        );
        assert_eq!(
            value["from"],
            json!([{ "name": "From Name", "email": "from@example.com" }])
        );
        assert_eq!(
            value["to"],
            json!([
                { "name": "Alice", "email": "alice@example.com" },
                { "name": "Bob", "email": "bob@example.com" },
                { "name": null, "email": "bare@example.com" }
            ])
        );
        assert_eq!(
            value["cc"],
            json!([{ "name": null, "email": "cc@example.com" }])
        );
        assert_eq!(
            value["bcc"],
            json!([{ "name": null, "email": "bcc@example.com" }])
        );
        assert_eq!(
            value["replyTo"],
            json!([{ "name": "Reply", "email": "reply@example.com" }])
        );
    }

    /// RFC 8621 §5.2 — `bodyValues` is keyed by part id and accepts both
    /// owned and borrowed text.
    #[test]
    fn body_values_are_keyed_by_part_id() {
        let mut email = fresh();
        email
            .body_value("1".to_string(), "plain text")
            .body_value("2".to_string(), String::from("html text"));

        let value = serde_json::to_value(&email).unwrap();
        assert_eq!(
            value["bodyValues"],
            json!({
                "1": { "value": "plain text" },
                "2": { "value": "html text" }
            })
        );
    }

    /// The `EmailBodyPart` builder produces the RFC 8621 §4.1.4 member
    /// names, and a `<Set>` part converts into the `<Get>` slot the
    /// whole-value builders expect.
    #[test]
    fn body_part_builder_produces_the_rfc_member_names() {
        let part = EmailBodyPart::new()
            .part_id("2")
            .blob_id("c2f9ff3a-123e-1234-80a1-371358b4bce8")
            .name("attachment.png")
            .content_type("image/png")
            .content_id("c2f9ff3a-123e-1234-80a1-371358b4bce8")
            .content_language(["en"])
            .content_location("https://example.com/attachment.png")
            .sub_part(
                EmailBodyPart::new()
                    .part_id("2.1")
                    .content_type("text/plain")
                    .into(),
            );

        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            json!({
                "partId": "2",
                "blobId": "c2f9ff3a-123e-1234-80a1-371358b4bce8",
                "name": "attachment.png",
                "type": "image/png",
                "cid": "c2f9ff3a-123e-1234-80a1-371358b4bce8",
                "language": ["en"],
                "location": "https://example.com/attachment.png",
                "subParts": [
                    { "partId": "2.1", "type": "text/plain" }
                ]
            })
        );
    }

    /// The three list-shaped body builders append rather than replace.
    #[test]
    fn body_list_builders_append() {
        let mut email = fresh();
        email
            .body_structure(EmailBodyPart::new().content_type("multipart/mixed").into())
            .text_body(EmailBodyPart::new().part_id("1").content_type("text/plain"))
            .text_body(EmailBodyPart::new().part_id("2").content_type("text/html"))
            .attachment(EmailBodyPart::new().name("file.png"));

        let value = serde_json::to_value(&email).unwrap();
        assert_eq!(value["bodyStructure"]["type"], json!("multipart/mixed"));
        assert_eq!(value["textBody"].as_array().unwrap().len(), 2);
        assert_eq!(value["textBody"][0]["partId"], json!("1"));
        assert_eq!(value["textBody"][1]["partId"], json!("2"));
        assert_eq!(value["attachments"][0]["name"], json!("file.png"));
    }

    /// The flattened `header:<name>:as<Form>` arm of the record is written
    /// through `header()`.
    #[test]
    fn header_builder_writes_the_flattened_header_map() {
        let mut email = fresh();
        email.header(
            Header {
                name: "X-Mailing-List".to_string(),
                form: HeaderForm::Text,
                all: false,
            },
            HeaderValue::AsText("list@example.com".to_string()),
        );

        assert_eq!(
            serde_json::to_value(&email).unwrap(),
            json!({ "header:X-Mailing-List:asText": "list@example.com" })
        );
    }

    /// RFC 8621 §4.1 — a grouped address is built up one member at a time.
    #[test]
    fn email_address_group_builder() {
        let group = EmailAddressGroup::new()
            .name("Team")
            .address(EmailAddress::from(("Alice", "alice@example.com")))
            .address("bob@example.com");

        assert_eq!(
            serde_json::to_value(&group).unwrap(),
            json!({
                "name": "Team",
                "addresses": [
                    { "name": "Alice", "email": "alice@example.com" },
                    { "name": null, "email": "bob@example.com" }
                ]
            })
        );
    }

    /// A body-part header entry is a plain name/value pair.
    #[test]
    fn email_header_new_is_a_plain_pair() {
        let header = EmailHeader::new("Content-Type".to_string(), "image/png".to_string());
        assert_eq!(header.name, "Content-Type");
        assert_eq!(header.value, "image/png");
    }

    /// The named constructor is the `<Set>` flavour: `new` takes the
    /// address, `.name` adds the display name.
    #[test]
    fn email_address_new_then_name() {
        assert_eq!(
            serde_json::to_value(
                EmailAddress::new("alice@example.com".to_string()).name("Alice".to_string())
            )
            .unwrap(),
            json!({ "name": "Alice", "email": "alice@example.com" })
        );
        assert_eq!(
            serde_json::to_value(EmailAddress::new("bob@example.com".to_string())).unwrap(),
            json!({ "name": null, "email": "bob@example.com" })
        );
    }

    /// RFC 8620 §5.3 — the create id is `c<n>`, and `set_id` writes the
    /// real id back once the server has answered.
    #[test]
    fn create_id_and_set_id() {
        assert_eq!(Email::<Set>::new(None).create_id(), None);
        assert_eq!(
            Email::<Set>::new(Some(0)).create_id(),
            Some("c0".to_string())
        );
        assert_eq!(
            Email::<Set>::new(Some(12)).create_id(),
            Some("c12".to_string())
        );

        let mut email = Email::<Set>::new(Some(1));
        email.set_id(Some("M8504".to_string()));
        assert_eq!(email.id, Some("M8504".to_string()));
        email.set_id(None);
        assert!(email.id.is_none());
    }

    /// The `<Get>` flavour can never report a create id: it is not a patch.
    #[test]
    fn a_get_flavoured_email_never_reports_a_create_id() {
        let email: Email<Get> = serde_json::from_value(json!({ "id": "M8504" })).unwrap();
        assert_eq!(email.create_id(), None);
    }
}
