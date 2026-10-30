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
pub mod import;
pub mod parse;
pub mod query;
pub mod search_snippet;
pub mod set;
use ahash::AHashMap;
use chrono::{DateTime, Utc};
use serde::{de::Visitor, Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

use crate::{
    core::{changes::ChangesObject, request::ResultReference, Object},
    Get, Set,
};

impl Object for Email<Set> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl Object for Email<Get> {
    type Property = Property;

    fn requires_account_id() -> bool {
        true
    }
}

impl ChangesObject for Email<Set> {
    type ChangesResponse = ();
}

impl ChangesObject for Email<Get> {
    type ChangesResponse = ();
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Email<State = Get> {
    #[serde(skip)]
    _create_id: Option<usize>,

    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,

    #[serde(rename = "blobId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    blob_id: Option<String>,

    #[serde(rename = "threadId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<String>,

    #[serde(rename = "mailboxIds")]
    #[serde(skip_serializing_if = "Option::is_none")]
    mailbox_ids: Option<AHashMap<String, bool>>,

    #[serde(rename = "#mailboxIds")]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    mailbox_ids_ref: Option<ResultReference>,

    #[serde(rename = "keywords")]
    #[serde(skip_serializing_if = "Option::is_none")]
    keywords: Option<AHashMap<String, bool>>,

    #[serde(rename = "size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<usize>,

    #[serde(rename = "receivedAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    received_at: Option<DateTime<Utc>>,

    #[cfg_attr(
        not(feature = "debug"),
        serde(alias = "header:Message-ID:asMessageIds")
    )]
    #[serde(rename = "messageId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<Vec<String>>,

    #[serde(rename = "inReplyTo")]
    #[cfg_attr(
        not(feature = "debug"),
        serde(alias = "header:In-Reply-To:asMessageIds")
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    in_reply_to: Option<Vec<String>>,

    #[serde(rename = "references")]
    #[cfg_attr(
        not(feature = "debug"),
        serde(alias = "header:References:asMessageIds")
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    references: Option<Vec<String>>,

    #[serde(rename = "sender")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Sender:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    sender: Option<Vec<EmailAddress>>,

    #[serde(rename = "from")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:From:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<Vec<EmailAddress>>,

    #[serde(rename = "to")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:To:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<Vec<EmailAddress>>,

    #[serde(rename = "cc")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Cc:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    cc: Option<Vec<EmailAddress>>,

    #[serde(rename = "bcc")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Bcc:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    bcc: Option<Vec<EmailAddress>>,

    #[serde(rename = "replyTo")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Reply-To:asAddresses"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<Vec<EmailAddress>>,

    #[serde(rename = "subject")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Subject:asText"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,

    #[serde(rename = "sentAt")]
    #[cfg_attr(not(feature = "debug"), serde(alias = "header:Date:asDate"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    sent_at: Option<DateTime<Utc>>,

    #[serde(rename = "bodyStructure")]
    #[serde(skip_serializing_if = "Option::is_none")]
    body_structure: Option<Box<EmailBodyPart>>,

    #[serde(rename = "bodyValues")]
    #[serde(skip_serializing_if = "Option::is_none")]
    body_values: Option<AHashMap<String, EmailBodyValue>>,

    #[serde(rename = "textBody")]
    #[serde(skip_serializing_if = "Option::is_none")]
    text_body: Option<Vec<EmailBodyPart>>,

    #[serde(rename = "htmlBody")]
    #[serde(skip_serializing_if = "Option::is_none")]
    html_body: Option<Vec<EmailBodyPart>>,

    #[serde(rename = "attachments")]
    #[serde(skip_serializing_if = "Option::is_none")]
    attachments: Option<Vec<EmailBodyPart>>,

    #[serde(rename = "hasAttachment")]
    #[serde(skip_serializing_if = "Option::is_none")]
    has_attachment: Option<bool>,

    #[serde(rename = "preview")]
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<String>,

    #[serde(flatten)]
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    headers: AHashMap<Header, Option<HeaderValue>>,

    #[serde(flatten)]
    #[serde(skip_deserializing)]
    #[serde(skip_serializing_if = "Option::is_none")]
    patch: Option<AHashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBodyPart<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "partId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    part_id: Option<String>,

    #[serde(rename = "blobId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    blob_id: Option<String>,

    #[serde(rename = "size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<usize>,

    #[serde(rename = "headers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<Vec<EmailHeader>>,

    #[serde(rename = "name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,

    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    type_: Option<String>,

    #[serde(rename = "charset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    charset: Option<String>,

    #[serde(rename = "disposition")]
    #[serde(skip_serializing_if = "Option::is_none")]
    disposition: Option<String>,

    #[serde(rename = "cid")]
    #[serde(skip_serializing_if = "Option::is_none")]
    cid: Option<String>,

    #[serde(rename = "language")]
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<Vec<String>>,

    #[serde(rename = "location")]
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,

    #[serde(rename = "subParts")]
    #[serde(skip_serializing_if = "Option::is_none")]
    sub_parts: Option<Vec<EmailBodyPart>>,

    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<AHashMap<Header, HeaderValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBodyValue<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    #[serde(rename = "value")]
    value: String,

    #[serde(rename = "isEncodingProblem")]
    #[serde(skip_serializing_if = "Option::is_none")]
    is_encoding_problem: Option<bool>,

    #[serde(rename = "isTruncated")]
    #[serde(skip_serializing_if = "Option::is_none")]
    is_truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    name: Option<String>,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddressGroup<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    name: Option<String>,
    addresses: Vec<EmailAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailHeader<State = Get> {
    #[serde(skip)]
    _state: std::marker::PhantomData<State>,

    name: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Property {
    Id,
    BlobId,
    ThreadId,
    MailboxIds,
    Keywords,
    Size,
    ReceivedAt,
    MessageId,
    InReplyTo,
    References,
    Sender,
    From,
    To,
    Cc,
    Bcc,
    ReplyTo,
    Subject,
    SentAt,
    BodyStructure,
    BodyValues,
    TextBody,
    HtmlBody,
    Attachments,
    HasAttachment,
    Preview,
    Header(Header),
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HeaderValue {
    AsDate(DateTime<Utc>),
    AsDateAll(Vec<DateTime<Utc>>),
    AsText(String),
    AsTextAll(Vec<String>),
    AsTextListAll(Vec<Vec<String>>),
    AsAddressesAll(Vec<Vec<EmailAddress>>),
    AsAddresses(Vec<EmailAddress>),
    AsGroupedAddressesAll(Vec<Vec<EmailAddressGroup>>),
    AsGroupedAddresses(Vec<EmailAddressGroup>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone)]
pub struct Header {
    pub name: String,
    pub form: HeaderForm,
    pub all: bool,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, PartialOrd, Ord)]
pub enum HeaderForm {
    Raw,
    Text,
    Addresses,
    GroupedAddresses,
    MessageIds,
    Date,
    URLs,
}

impl Property {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "id" => Some(Property::Id),
            "blobId" => Some(Property::BlobId),
            "threadId" => Some(Property::ThreadId),
            "mailboxIds" => Some(Property::MailboxIds),
            "keywords" => Some(Property::Keywords),
            "size" => Some(Property::Size),
            "receivedAt" => Some(Property::ReceivedAt),
            "messageId" => Some(Property::MessageId),
            "inReplyTo" => Some(Property::InReplyTo),
            "references" => Some(Property::References),
            "sender" => Some(Property::Sender),
            "from" => Some(Property::From),
            "to" => Some(Property::To),
            "cc" => Some(Property::Cc),
            "bcc" => Some(Property::Bcc),
            "replyTo" => Some(Property::ReplyTo),
            "subject" => Some(Property::Subject),
            "sentAt" => Some(Property::SentAt),
            "hasAttachment" => Some(Property::HasAttachment),
            "preview" => Some(Property::Preview),
            "bodyValues" => Some(Property::BodyValues),
            "textBody" => Some(Property::TextBody),
            "htmlBody" => Some(Property::HtmlBody),
            "attachments" => Some(Property::Attachments),
            "bodyStructure" => Some(Property::BodyStructure),
            _ if value.starts_with("header:") => Some(Property::Header(Header::parse(value)?)),
            _ => Some(Property::Other(value.to_string())),
        }
    }
}

impl Display for Property {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Property::Id => write!(f, "id"),
            Property::BlobId => write!(f, "blobId"),
            Property::ThreadId => write!(f, "threadId"),
            Property::MailboxIds => write!(f, "mailboxIds"),
            Property::Keywords => write!(f, "keywords"),
            Property::Size => write!(f, "size"),
            Property::ReceivedAt => write!(f, "receivedAt"),
            Property::MessageId => write!(f, "messageId"),
            Property::InReplyTo => write!(f, "inReplyTo"),
            Property::References => write!(f, "references"),
            Property::Sender => write!(f, "sender"),
            Property::From => write!(f, "from"),
            Property::To => write!(f, "to"),
            Property::Cc => write!(f, "cc"),
            Property::Bcc => write!(f, "bcc"),
            Property::ReplyTo => write!(f, "replyTo"),
            Property::Subject => write!(f, "subject"),
            Property::SentAt => write!(f, "sentAt"),
            Property::BodyStructure => write!(f, "bodyStructure"),
            Property::BodyValues => write!(f, "bodyValues"),
            Property::TextBody => write!(f, "textBody"),
            Property::HtmlBody => write!(f, "htmlBody"),
            Property::Attachments => write!(f, "attachments"),
            Property::HasAttachment => write!(f, "hasAttachment"),
            Property::Preview => write!(f, "preview"),
            Property::Header(header) => header.fmt(f),
            Property::Other(other) => write!(f, "{}", other),
        }
    }
}

impl Serialize for Property {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct PropertyVisitor;

impl<'de> Visitor<'de> for PropertyVisitor {
    type Value = Property;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP e-mail property")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Property::parse(v).ok_or_else(|| {
            serde::de::Error::custom(format!("Failed to parse JMAP property '{}'", v))
        })
    }
}

impl<'de> Deserialize<'de> for Property {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(PropertyVisitor)
    }
}

impl Serialize for Header {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct HeaderVisitor;

impl<'de> Visitor<'de> for HeaderVisitor {
    type Value = Header;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP header")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Header::parse(v)
            .ok_or_else(|| serde::de::Error::custom(format!("Failed to parse JMAP header '{}'", v)))
    }
}

impl<'de> Deserialize<'de> for Header {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(HeaderVisitor)
    }
}

impl HeaderForm {
    pub fn parse(value: &str) -> Option<HeaderForm> {
        match value {
            "asText" => Some(HeaderForm::Text),
            "asAddresses" => Some(HeaderForm::Addresses),
            "asGroupedAddresses" => Some(HeaderForm::GroupedAddresses),
            "asMessageIds" => Some(HeaderForm::MessageIds),
            "asDate" => Some(HeaderForm::Date),
            "asURLs" => Some(HeaderForm::URLs),
            _ => None,
        }
    }
}

impl Header {
    pub fn as_raw(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::Raw,
            all,
        }
    }

    pub fn as_text(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::Text,
            all,
        }
    }

    pub fn as_addresses(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::Addresses,
            all,
        }
    }

    pub fn as_grouped_addresses(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::GroupedAddresses,
            all,
        }
    }

    pub fn as_message_ids(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::MessageIds,
            all,
        }
    }

    pub fn as_date(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::Date,
            all,
        }
    }

    pub fn as_urls(name: impl Into<String>, all: bool) -> Header {
        Header {
            name: name.into(),
            form: HeaderForm::URLs,
            all,
        }
    }

    pub fn parse(value: &str) -> Option<Header> {
        let mut all = false;
        let mut form = HeaderForm::Raw;
        let mut header = None;
        for (pos, part) in value.split(':').enumerate() {
            match pos {
                0 if part == "header" => (),
                1 => {
                    header = part.into();
                }
                2 | 3 if part == "all" => all = true,
                2 => {
                    form = HeaderForm::parse(part)?;
                }
                _ => return None,
            }
        }
        Header {
            name: header?.to_string(),
            form,
            all,
        }
        .into()
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "header:")?;
        self.name.fmt(f)?;
        self.form.fmt(f)?;
        if self.all {
            write!(f, ":all")
        } else {
            Ok(())
        }
    }
}

impl Display for HeaderForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeaderForm::Raw => Ok(()),
            HeaderForm::Text => write!(f, ":asText"),
            HeaderForm::Addresses => write!(f, ":asAddresses"),
            HeaderForm::GroupedAddresses => write!(f, ":asGroupedAddresses"),
            HeaderForm::MessageIds => write!(f, ":asMessageIds"),
            HeaderForm::Date => write!(f, ":asDate"),
            HeaderForm::URLs => write!(f, ":asURLs"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BodyProperty {
    PartId,
    BlobId,
    Size,
    Headers,
    Name,
    Type,
    Charset,
    Disposition,
    Cid,
    Language,
    Location,
    SubParts,
    Header(Header),
}

impl BodyProperty {
    fn parse(value: &str) -> Option<BodyProperty> {
        match value {
            "partId" => Some(BodyProperty::PartId),
            "blobId" => Some(BodyProperty::BlobId),
            "size" => Some(BodyProperty::Size),
            "name" => Some(BodyProperty::Name),
            "type" => Some(BodyProperty::Type),
            "charset" => Some(BodyProperty::Charset),
            "headers" => Some(BodyProperty::Headers),
            "disposition" => Some(BodyProperty::Disposition),
            "cid" => Some(BodyProperty::Cid),
            "language" => Some(BodyProperty::Language),
            "location" => Some(BodyProperty::Location),
            "subParts" => Some(BodyProperty::SubParts),
            _ if value.starts_with("header:") => Some(BodyProperty::Header(Header::parse(value)?)),
            _ => None,
        }
    }
}

impl Display for BodyProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyProperty::PartId => write!(f, "partId"),
            BodyProperty::BlobId => write!(f, "blobId"),
            BodyProperty::Size => write!(f, "size"),
            BodyProperty::Name => write!(f, "name"),
            BodyProperty::Type => write!(f, "type"),
            BodyProperty::Charset => write!(f, "charset"),
            BodyProperty::Header(header) => header.fmt(f),
            BodyProperty::Headers => write!(f, "headers"),
            BodyProperty::Disposition => write!(f, "disposition"),
            BodyProperty::Cid => write!(f, "cid"),
            BodyProperty::Language => write!(f, "language"),
            BodyProperty::Location => write!(f, "location"),
            BodyProperty::SubParts => write!(f, "subParts"),
        }
    }
}

impl Serialize for BodyProperty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct BodyPropertyVisitor;

impl<'de> Visitor<'de> for BodyPropertyVisitor {
    type Value = BodyProperty;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP body property")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        BodyProperty::parse(v).ok_or_else(|| {
            serde::de::Error::custom(format!("Failed to parse JMAP body property '{}'", v))
        })
    }
}

impl<'de> Deserialize<'de> for BodyProperty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(BodyPropertyVisitor)
    }
}

/// The `urn:ietf:params:jmap:mail` capability block (RFC 8621 §1.3.1).
///
/// RFC 8621 §1.3.1 says the value of this capability in the *session*
/// `capabilities` property is an **empty object**, and that the limits below
/// live in an account's `accountCapabilities`. Read them through
/// [`crate::core::session::Account::mail_capabilities`];
/// [`crate::core::session::Session::mail_capabilities`] only finds something
/// when a server deviates from §1.3.1 and repeats the block at session level.
///
/// Every property RFC 8621 §1.3.1 types as `UnsignedInt|null` is an `Option`,
/// so a conforming `null` ("no limit") parses instead of failing the whole
/// capability. `maxSizeMailboxName` stays required: §1.3.1 types it as a plain
/// `UnsignedInt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailCapabilities {
    /// `UnsignedInt|null` (RFC 8621 §1.3.1); `None` means no limit.
    #[serde(rename = "maxMailboxesPerEmail")]
    max_mailboxes_per_email: Option<usize>,

    /// `UnsignedInt|null` (RFC 8621 §1.3.1); `None` means no limit.
    #[serde(rename = "maxMailboxDepth")]
    max_mailbox_depth: Option<usize>,

    /// `UnsignedInt` (RFC 8621 §1.3.1), always present.
    #[serde(rename = "maxSizeMailboxName")]
    max_size_mailbox_name: usize,

    /// RFC 8621 §1.3.1 types this as a plain `UnsignedInt`, so a conforming
    /// server always sends a number. It is modelled as an `Option` anyway, as a
    /// deliberate deviation: because `Capabilities` is matched on shape, a
    /// single unparseable member drops the whole block to
    /// `Capabilities::Other` and every mail limit is lost, which is a worse
    /// outcome than reading one limit as unknown.
    #[serde(rename = "maxSizeAttachmentsPerEmail")]
    max_size_attachments_per_email: Option<usize>,

    /// `String[]` (RFC 8621 §1.3.1), always present.
    #[serde(rename = "emailQuerySortOptions")]
    email_query_sort_options: Vec<String>,

    /// `Boolean` (RFC 8621 §1.3.1), always present.
    #[serde(rename = "mayCreateTopLevelMailbox")]
    may_create_top_level_mailbox: bool,
}

/// The `urn:ietf:params:jmap:submission` capability block (RFC 8621 §1.3.2).
///
/// RFC 8621 §1.3.2 says the value of this capability in the *session*
/// `capabilities` property is an **empty object**, and that `maxDelayedSend`
/// and `submissionExtensions` live in an account's `accountCapabilities`. Read
/// them through
/// [`crate::core::session::Account::submission_capabilities`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionCapabilities {
    /// `UnsignedInt` (RFC 8621 §1.3.2).
    #[serde(rename = "maxDelayedSend")]
    max_delayed_send: usize,

    /// RFC 8621 §1.3.2 types `submissionExtensions` as `String[String[]]`: a
    /// map whose keys are the SMTP extension "ehlo-name" and whose values are
    /// that extension's "ehlo-args", for example
    /// `{"SIZE": ["1229782938"], "DSN": [], "UTF8ONLY": []}`.
    #[serde(rename = "submissionExtensions")]
    submission_extensions: AHashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct QueryArguments {
    #[serde(rename = "collapseThreads")]
    #[serde(skip_serializing_if = "Option::is_none")]
    collapse_threads: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GetArguments {
    #[serde(rename = "bodyProperties")]
    #[serde(skip_serializing_if = "Option::is_none")]
    body_properties: Option<Vec<BodyProperty>>,

    #[serde(rename = "fetchTextBodyValues")]
    #[serde(skip_serializing_if = "Option::is_none")]
    fetch_text_body_values: Option<bool>,

    #[serde(rename = "fetchHTMLBodyValues")]
    #[serde(skip_serializing_if = "Option::is_none")]
    fetch_html_body_values: Option<bool>,

    #[serde(rename = "fetchAllBodyValues")]
    #[serde(skip_serializing_if = "Option::is_none")]
    fetch_all_body_values: Option<bool>,

    #[serde(rename = "maxBodyValueBytes")]
    #[serde(skip_serializing_if = "Option::is_none")]
    max_body_value_bytes: Option<usize>,
}

impl QueryArguments {
    pub fn collapse_threads(&mut self, collapse_threads: bool) {
        self.collapse_threads = collapse_threads.into();
    }
}

impl GetArguments {
    pub fn body_properties(
        &mut self,
        body_properties: impl IntoIterator<Item = BodyProperty>,
    ) -> &mut Self {
        self.body_properties = Some(body_properties.into_iter().collect());
        self
    }

    pub fn fetch_text_body_values(&mut self, fetch_text_body_values: bool) -> &mut Self {
        self.fetch_text_body_values = fetch_text_body_values.into();
        self
    }

    pub fn fetch_html_body_values(&mut self, fetch_html_body_values: bool) -> &mut Self {
        self.fetch_html_body_values = fetch_html_body_values.into();
        self
    }

    pub fn fetch_all_body_values(&mut self, fetch_all_body_values: bool) -> &mut Self {
        self.fetch_all_body_values = fetch_all_body_values.into();
        self
    }

    pub fn max_body_value_bytes(&mut self, max_body_value_bytes: usize) -> &mut Self {
        self.max_body_value_bytes = max_body_value_bytes.into();
        self
    }
}

impl MailCapabilities {
    /// `None` when the server sent `null`, i.e. there is no limit
    /// (RFC 8621 §1.3.1).
    pub fn max_mailboxes_per_email(&self) -> Option<usize> {
        self.max_mailboxes_per_email
    }

    /// `None` when the server sent `null`, i.e. there is no limit
    /// (RFC 8621 §1.3.1).
    pub fn max_mailbox_depth(&self) -> Option<usize> {
        self.max_mailbox_depth
    }

    pub fn max_size_mailbox_name(&self) -> usize {
        self.max_size_mailbox_name
    }

    /// `None` only if the server sent `null`, which RFC 8621 §1.3.1 does not
    /// allow for this property; see the field's note on why that is tolerated.
    pub fn max_size_attachments_per_email(&self) -> Option<usize> {
        self.max_size_attachments_per_email
    }

    pub fn email_query_sort_options(&self) -> &[String] {
        &self.email_query_sort_options
    }

    pub fn may_create_top_level_mailbox(&self) -> bool {
        self.may_create_top_level_mailbox
    }
}

impl SubmissionCapabilities {
    pub fn max_delayed_send(&self) -> usize {
        self.max_delayed_send
    }

    /// The `submissionExtensions` map (RFC 8621 §1.3.2): each key is an SMTP
    /// submission extension's "ehlo-name", each value that extension's
    /// "ehlo-args" (empty when it takes none).
    pub fn submission_extensions(&self) -> &AHashMap<String, Vec<String>> {
        &self.submission_extensions
    }
}

#[cfg(feature = "debug")]
use std::collections::BTreeMap;

#[cfg(feature = "debug")]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TestEmail {
    #[serde(rename = "mailboxIds")]
    pub mailbox_ids: Option<BTreeMap<String, bool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<BTreeMap<String, bool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,

    #[serde(rename = "receivedAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<DateTime<Utc>>,

    #[serde(rename = "messageId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Vec<String>>,

    #[serde(rename = "inReplyTo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<Vec<EmailAddress>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Vec<EmailAddress>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<EmailAddress>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Vec<EmailAddress>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcc: Option<Vec<EmailAddress>>,

    #[serde(rename = "replyTo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<EmailAddress>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    #[serde(rename = "sentAt")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<DateTime<Utc>>,

    #[serde(rename = "bodyStructure")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_structure: Option<Box<EmailBodyPart>>,

    #[serde(rename = "bodyValues")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_values: Option<BTreeMap<String, EmailBodyValue>>,

    #[serde(rename = "textBody")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<Vec<EmailBodyPart>>,

    #[serde(rename = "htmlBody")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<Vec<EmailBodyPart>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<EmailBodyPart>>,

    #[serde(rename = "hasAttachment")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,

    #[serde(flatten)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<Header, Option<HeaderValue>>,
}

#[cfg(feature = "debug")]
impl From<Email> for TestEmail {
    fn from(email: Email) -> Self {
        TestEmail {
            mailbox_ids: email.mailbox_ids.map(|ids| ids.into_iter().collect()),
            keywords: email
                .keywords
                .map(|keywords| keywords.into_iter().collect()),
            size: email.size,
            received_at: email.received_at,
            message_id: email.message_id,
            in_reply_to: email.in_reply_to,
            references: email.references,
            sender: email.sender,
            from: email.from,
            to: email.to,
            cc: email.cc,
            bcc: email.bcc,
            reply_to: email.reply_to,
            subject: email.subject,
            sent_at: email.sent_at,
            body_structure: email.body_structure,
            body_values: email
                .body_values
                .map(|body_values| body_values.into_iter().collect()),
            text_body: email.text_body,
            html_body: email.html_body,
            attachments: email.attachments,
            has_attachment: email.has_attachment,
            preview: email.preview,
            headers: email.headers.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure property logic of the mail module:
    //! `Property` (RFC 8621 §4.1), `Header`/`HeaderForm`
    //! (§4.1.2/§4.1.3), `BodyProperty` (§4.1.4), the `HeaderValue`
    //! forms and the two session capability records (§2). Every fixture
    //! is JSON deserialised in place — no server, no socket, no async.

    use super::*;
    use serde_json::json;

    /// RFC 8621 §4.1 — `Display` and `Serialize` must agree on the wire
    /// name of every variant, and reading that name back must land on the
    /// same variant.
    #[test]
    fn property_display_and_serde_round_trip_every_variant() {
        let cases: Vec<(Property, &str)> = vec![
            (Property::Id, "id"),
            (Property::BlobId, "blobId"),
            (Property::ThreadId, "threadId"),
            (Property::MailboxIds, "mailboxIds"),
            (Property::Keywords, "keywords"),
            (Property::Size, "size"),
            (Property::ReceivedAt, "receivedAt"),
            (Property::MessageId, "messageId"),
            (Property::InReplyTo, "inReplyTo"),
            (Property::References, "references"),
            (Property::Sender, "sender"),
            (Property::From, "from"),
            (Property::To, "to"),
            (Property::Cc, "cc"),
            (Property::Bcc, "bcc"),
            (Property::ReplyTo, "replyTo"),
            (Property::Subject, "subject"),
            (Property::SentAt, "sentAt"),
            (Property::BodyStructure, "bodyStructure"),
            (Property::BodyValues, "bodyValues"),
            (Property::TextBody, "textBody"),
            (Property::HtmlBody, "htmlBody"),
            (Property::Attachments, "attachments"),
            (Property::HasAttachment, "hasAttachment"),
            (Property::Preview, "preview"),
            (
                Property::Header(Header::as_addresses("X-Mailing-List", true)),
                "header:X-Mailing-List:asAddresses:all",
            ),
            (
                Property::Other("x-vendor-property".to_string()),
                "x-vendor-property",
            ),
        ];

        // 25 named properties + the `header:` arm + the `Other` catch-all
        assert_eq!(cases.len(), 27);
        for (property, wire) in cases {
            assert_eq!(property.to_string(), wire, "{property:?} Display");
            assert_eq!(serde_json::to_value(&property).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<Property>(json!(wire)).unwrap(),
                property,
                "{wire} must round-trip back to the variant it came from"
            );
        }
    }

    /// RFC 8621 §4.1 — `Property::parse` maps the known names, hands the
    /// `header:` prefix to `Header::parse`, and falls through to `Other`
    /// for anything else. The prefix test is on `"header:"`, not
    /// `"header"`, so `headerX` is an `Other`, not a header.
    #[test]
    fn property_parse_covers_names_header_arm_and_fallback() {
        assert_eq!(Property::parse("mailboxIds"), Some(Property::MailboxIds));
        assert_eq!(Property::parse("keywords"), Some(Property::Keywords));
        assert_eq!(
            Property::parse("bodyStructure"),
            Some(Property::BodyStructure)
        );
        assert_eq!(
            Property::parse("header:Subject:asText"),
            Some(Property::Header(Header::as_text("Subject", false)))
        );
        assert_eq!(
            Property::parse("header:Received:all"),
            Some(Property::Header(Header::as_raw("Received", true)))
        );
        assert_eq!(
            Property::parse("x-vendor"),
            Some(Property::Other("x-vendor".to_string()))
        );
        assert_eq!(
            Property::parse("headerX"),
            Some(Property::Other("headerX".to_string())),
            "only `header:` (with the colon) routes to the header arm"
        );
    }

    /// The `PropertyVisitor` failure arm: a `header:` property whose form
    /// is not one of the six the RFC defines makes `parse` return `None`,
    /// and the visitor turns that into a serde error naming the string.
    #[test]
    fn property_deserialise_reports_an_unparsable_header_form() {
        assert_eq!(Property::parse("header:Subject:notAForm"), None);
        let error = serde_json::from_value::<Property>(json!("header:Subject:notAForm"))
            .expect_err("an unknown header form must not deserialise");
        assert_eq!(
            error.to_string(),
            "Failed to parse JMAP property 'header:Subject:notAForm'"
        );
    }

    /// RFC 8621 §4.1.2 — the six header forms, their `asXxx` tokens and
    /// the `:asXxx` suffix each renders. `Raw` is the absence of a form
    /// and therefore renders as the empty string.
    #[test]
    fn header_form_parse_and_display() {
        let cases: &[(&str, HeaderForm, &str)] = &[
            ("asText", HeaderForm::Text, ":asText"),
            ("asAddresses", HeaderForm::Addresses, ":asAddresses"),
            (
                "asGroupedAddresses",
                HeaderForm::GroupedAddresses,
                ":asGroupedAddresses",
            ),
            ("asMessageIds", HeaderForm::MessageIds, ":asMessageIds"),
            ("asDate", HeaderForm::Date, ":asDate"),
            ("asURLs", HeaderForm::URLs, ":asURLs"),
        ];

        assert_eq!(cases.len(), 6, "the RFC defines exactly six forms");
        for (token, form, suffix) in cases {
            assert_eq!(HeaderForm::parse(token), Some(form.clone()), "{token}");
            assert_eq!(form.to_string(), *suffix, "{form:?} Display");
        }

        assert_eq!(HeaderForm::Raw.to_string(), "", "Raw adds no suffix");
        assert_eq!(HeaderForm::parse("asRaw"), None);
        assert_eq!(HeaderForm::parse("text"), None);
        assert_eq!(HeaderForm::parse(""), None);
    }

    /// RFC 8621 §4.1.2 — the seven constructors build exactly the header
    /// their `Display` string spells back out, and `Header::parse` is the
    /// inverse of that string for every one of them.
    #[test]
    fn header_constructors_round_trip_through_display_and_parse() {
        let built: Vec<(Header, &str)> = vec![
            (Header::as_raw("X-Raw", false), "header:X-Raw"),
            (Header::as_raw("X-Raw", true), "header:X-Raw:all"),
            (Header::as_text("X-Text", false), "header:X-Text:asText"),
            (Header::as_text("X-Text", true), "header:X-Text:asText:all"),
            (Header::as_addresses("To", false), "header:To:asAddresses"),
            (
                Header::as_grouped_addresses("To", true),
                "header:To:asGroupedAddresses:all",
            ),
            (
                Header::as_message_ids("Message-Id", false),
                "header:Message-Id:asMessageIds",
            ),
            (Header::as_date("Date", true), "header:Date:asDate:all"),
            (
                Header::as_urls("List-Subscribe", false),
                "header:List-Subscribe:asURLs",
            ),
        ];

        for (header, wire) in built {
            assert_eq!(header.to_string(), wire, "{header:?} Display");
            assert_eq!(serde_json::to_value(&header).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<Header>(json!(wire)).unwrap(),
                header,
                "{wire} must round-trip back to the same header"
            );
            assert_eq!(Header::parse(wire), Some(header.clone()));
        }
    }

    /// RFC 8621 §4.1.2 — the shape `Header::parse` accepts is
    /// `header:<name>[:as<Form>][:all]`. Anything outside it is `None`:
    /// the `all` flag is only recognised in the third and fourth position
    /// and only as the literal `all`, so `header:To:all:asText` fails.
    #[test]
    fn header_parse_accepts_the_grammar_and_rejects_everything_else() {
        assert_eq!(
            Header::parse("header:Subject"),
            Some(Header {
                name: "Subject".to_string(),
                form: HeaderForm::Raw,
                all: false,
            })
        );
        assert_eq!(
            Header::parse("header:To:asAddresses:all"),
            Some(Header {
                name: "To".to_string(),
                form: HeaderForm::Addresses,
                all: true,
            })
        );
        // A `header:` with no name at all parses, to an empty name.
        assert_eq!(
            Header::parse("header:"),
            Some(Header {
                name: String::new(),
                form: HeaderForm::Raw,
                all: false,
            })
        );

        assert_eq!(Header::parse("header"), None, "a name is required");
        assert_eq!(Header::parse(""), None);
        assert_eq!(Header::parse("Subject"), None, "the prefix is required");
        assert_eq!(Header::parse("header:Subject:bogus"), None);
        assert_eq!(Header::parse("header:To:all:asText"), None);
        assert_eq!(Header::parse("header:To:asText:all:extra"), None);
    }

    /// The `HeaderVisitor` failure arm, reached through `Deserialize`
    /// rather than a direct `parse` call.
    #[test]
    fn header_deserialise_reports_an_unparsable_header() {
        let error = serde_json::from_value::<Header>(json!("X-Raw"))
            .expect_err("a bare header name is not a JMAP header property");
        assert_eq!(error.to_string(), "Failed to parse JMAP header 'X-Raw'");
    }

    /// RFC 8621 §4.1.4 — the twelve body-structure property names plus
    /// the `header:` arm, driven through `Display`, `Serialize` and
    /// `Deserialize`.
    #[test]
    fn body_property_display_and_serde_round_trip_every_variant() {
        let cases: Vec<(BodyProperty, &str)> = vec![
            (BodyProperty::PartId, "partId"),
            (BodyProperty::BlobId, "blobId"),
            (BodyProperty::Size, "size"),
            (BodyProperty::Headers, "headers"),
            (BodyProperty::Name, "name"),
            (BodyProperty::Type, "type"),
            (BodyProperty::Charset, "charset"),
            (BodyProperty::Disposition, "disposition"),
            (BodyProperty::Cid, "cid"),
            (BodyProperty::Language, "language"),
            (BodyProperty::Location, "location"),
            (BodyProperty::SubParts, "subParts"),
            (
                BodyProperty::Header(Header::as_text("Content-Type", false)),
                "header:Content-Type:asText",
            ),
        ];

        assert_eq!(cases.len(), 13, "twelve members plus the header arm");
        for (property, wire) in cases {
            assert_eq!(property.to_string(), wire, "{property:?} Display");
            assert_eq!(serde_json::to_value(&property).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<BodyProperty>(json!(wire)).unwrap(),
                property,
                "{wire} must round-trip back to the variant it came from"
            );
        }
    }

    /// Unlike `email::Property`, `BodyProperty` has no `Other` catch-all:
    /// an unknown name is a parse failure, and the visitor reports it.
    #[test]
    fn body_property_parse_rejects_unknown_names_and_bad_header_forms() {
        assert_eq!(
            BodyProperty::parse("header:Cid:asText"),
            Some(BodyProperty::Header(Header::as_text("Cid", false)))
        );
        assert_eq!(BodyProperty::parse("header:Cid:bogus"), None);
        assert_eq!(BodyProperty::parse("notAProperty"), None);
        assert_eq!(BodyProperty::parse(""), None);

        let error = serde_json::from_value::<BodyProperty>(json!("nope"))
            .expect_err("an unknown body property must not deserialise");
        assert_eq!(
            error.to_string(),
            "Failed to parse JMAP body property 'nope'"
        );
    }

    /// RFC 8621 §4.1.2 — the header value forms are untagged, so the
    /// JSON shape alone picks the variant. `HeaderValue` does not
    /// implement `PartialEq`, so the round trip is checked by comparing
    /// the re-serialised wire form.
    #[test]
    fn header_value_forms_survive_a_wire_round_trip() {
        let cases: Vec<(HeaderValue, serde_json::Value)> = vec![
            (
                HeaderValue::AsText("Team lunch".into()),
                json!("Team lunch"),
            ),
            (
                HeaderValue::AsTextAll(vec!["a".into(), "b".into()]),
                json!(["a", "b"]),
            ),
            (
                HeaderValue::AsDate("2024-07-03T09:46:40Z".parse().unwrap()),
                json!("2024-07-03T09:46:40Z"),
            ),
            (
                HeaderValue::AsDateAll(vec!["2024-07-03T09:46:40Z".parse().unwrap()]),
                json!(["2024-07-03T09:46:40Z"]),
            ),
            (
                HeaderValue::AsAddresses(vec![EmailAddress::from((
                    "Joe".to_string(),
                    "joe@example.com".to_string(),
                ))]),
                json!([{ "name": "Joe", "email": "joe@example.com" }]),
            ),
            (
                HeaderValue::AsAddressesAll(vec![vec![EmailAddress::from("joe@example.com")]]),
                json!([[{ "name": null, "email": "joe@example.com" }]]),
            ),
            (
                HeaderValue::AsGroupedAddresses(vec![serde_json::from_value(json!({
                    "name": "Team",
                    "addresses": [{ "name": null, "email": "joe@example.com" }]
                }))
                .unwrap()]),
                json!([{
                    "name": "Team",
                    "addresses": [{ "name": null, "email": "joe@example.com" }]
                }]),
            ),
        ];

        for (value, wire) in cases {
            let serialised = serde_json::to_value(&value).unwrap();
            assert_eq!(serialised, wire, "{value:?} must serialise as {wire}");
            let back: HeaderValue = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serialised,
                "{wire} must round-trip back to the same form"
            );
        }
    }

    /// RFC 8621 §1.3.1 — the `urn:ietf:params:jmap:mail` capability block.
    /// `maxMailboxesPerEmail` and `maxMailboxDepth` are `UnsignedInt|null`, so
    /// `null` is a conforming value meaning "no limit", not a parse error.
    #[test]
    fn mail_capability_accessors_read_the_session_block() {
        let caps: MailCapabilities = serde_json::from_value(json!({
            "maxMailboxesPerEmail": null,
            "maxMailboxDepth": 10,
            "maxSizeMailboxName": 255,
            "maxSizeAttachmentsPerEmail": 33554432,
            "emailQuerySortOptions": [
                "receivedAt", "size", "subject", "displayTo", "threadId"
            ],
            "mayCreateTopLevelMailbox": true
        }))
        .unwrap();

        assert_eq!(caps.max_mailboxes_per_email(), None);
        assert_eq!(caps.max_mailbox_depth(), Some(10));
        assert_eq!(caps.max_size_mailbox_name(), 255);
        assert_eq!(caps.max_size_attachments_per_email(), Some(33_554_432));
        assert_eq!(
            caps.email_query_sort_options(),
            ["receivedAt", "size", "subject", "displayTo", "threadId"]
        );
        assert!(caps.may_create_top_level_mailbox());

        let limited: MailCapabilities = serde_json::from_value(json!({
            "maxMailboxesPerEmail": 1,
            "maxMailboxDepth": 1,
            "maxSizeMailboxName": 30,
            "maxSizeAttachmentsPerEmail": 1024,
            "emailQuerySortOptions": [],
            "mayCreateTopLevelMailbox": false
        }))
        .unwrap();
        assert_eq!(limited.max_mailboxes_per_email(), Some(1));
        assert_eq!(limited.max_mailbox_depth(), Some(1));
        assert_eq!(limited.max_size_attachments_per_email(), Some(1024));
        assert!(!limited.may_create_top_level_mailbox());
        assert!(limited.email_query_sort_options().is_empty());
    }

    /// RFC 8621 §1.3.1 — `maxMailboxDepth` is `UnsignedInt|null`: a server with
    /// no limit on the mailbox hierarchy depth sends `null`, and that must
    /// parse rather than push the whole capability into `Capabilities::Other`.
    #[test]
    fn mail_capability_accepts_a_null_depth_and_attachment_limit() {
        let caps: MailCapabilities = serde_json::from_value(json!({
            "maxMailboxesPerEmail": null,
            "maxMailboxDepth": null,
            "maxSizeMailboxName": 255,
            "maxSizeAttachmentsPerEmail": null,
            "emailQuerySortOptions": ["receivedAt"],
            "mayCreateTopLevelMailbox": true
        }))
        .expect("RFC 8621 §1.3.1 allows null for maxMailboxDepth");

        assert_eq!(caps.max_mailbox_depth(), None);
        assert_eq!(caps.max_size_attachments_per_email(), None);
        assert_eq!(caps.max_size_mailbox_name(), 255);
    }

    /// RFC 8621 §1.3.2 — the `urn:ietf:params:jmap:submission` capability
    /// block. `submissionExtensions` is `String[String[]]`: an object keyed by
    /// the extension's ehlo-name whose value is that extension's ehlo-args.
    #[test]
    fn submission_capability_accessors_read_the_session_block() {
        let caps: SubmissionCapabilities = serde_json::from_value(json!({
            "maxDelayedSend": 1209600,
            "submissionExtensions": {
                "SIZE": ["1229782938"],
                "DSN": [],
                "UTF8ONLY": []
            }
        }))
        .expect("RFC 8621 §1.3.2 submissionExtensions is a map of name to args");

        assert_eq!(caps.max_delayed_send(), 1_209_600);
        let extensions = caps.submission_extensions();
        assert_eq!(extensions.len(), 3);
        assert_eq!(
            extensions.get("SIZE").map(Vec::as_slice),
            Some(&["1229782938".to_string()][..]),
            "the ehlo-args survive the round trip through the map"
        );
        assert_eq!(extensions.get("DSN").map(Vec::as_slice), Some(&[][..]));
        assert_eq!(extensions.get("UTF8ONLY").map(Vec::as_slice), Some(&[][..]));
        assert_eq!(extensions.get("PIPELINING"), None);

        let bare: SubmissionCapabilities = serde_json::from_value(json!(
            { "maxDelayedSend": 0, "submissionExtensions": {} }
        ))
        .unwrap();
        assert_eq!(bare.max_delayed_send(), 0);
        assert!(bare.submission_extensions().is_empty());
    }

    /// RFC 8621 §4.1 — the `Email/get` argument builders, including the
    /// default (nothing set serialises to an empty object).
    #[test]
    fn get_and_query_argument_builders() {
        let args = GetArguments::default();
        assert_eq!(serde_json::to_value(&args).unwrap(), json!({}));

        let mut args = GetArguments::default();
        args.body_properties([
            BodyProperty::PartId,
            BodyProperty::Header(Header::as_text("Content-Type", false)),
        ])
        .fetch_text_body_values(true)
        .fetch_html_body_values(false)
        .fetch_all_body_values(true)
        .max_body_value_bytes(4096);
        assert_eq!(
            serde_json::to_value(&args).unwrap(),
            json!({
                "bodyProperties": ["partId", "header:Content-Type:asText"],
                "fetchTextBodyValues": true,
                "fetchHTMLBodyValues": false,
                "fetchAllBodyValues": true,
                "maxBodyValueBytes": 4096
            })
        );

        let mut query = QueryArguments::default();
        assert_eq!(serde_json::to_value(&query).unwrap(), json!({}));
        query.collapse_threads(true);
        assert_eq!(
            serde_json::to_value(&query).unwrap(),
            json!({ "collapseThreads": true })
        );
    }

    /// RFC 8621 §2 — both `Email` flavours are account-scoped objects.
    #[test]
    fn both_email_flavours_require_an_account_id() {
        assert!(<Email<Set> as Object>::requires_account_id());
        assert!(<Email<Get> as Object>::requires_account_id());
    }

    /// A non-string input makes serde fall back to each visitor's
    /// `expecting()` text, so the three custom property visitors must name
    /// what they wanted.
    #[test]
    fn visitors_report_what_they_expected_on_a_non_string_input() {
        let property = serde_json::from_value::<Property>(json!(42))
            .unwrap_err()
            .to_string();
        assert!(
            property.contains("a valid JMAP e-mail property"),
            "Property visitor: {property}"
        );

        let header = serde_json::from_value::<Header>(json!(42))
            .unwrap_err()
            .to_string();
        assert!(
            header.contains("a valid JMAP header"),
            "Header visitor: {header}"
        );

        let body = serde_json::from_value::<BodyProperty>(json!(42))
            .unwrap_err()
            .to_string();
        assert!(
            body.contains("a valid JMAP body property"),
            "BodyProperty visitor: {body}"
        );
    }
}
