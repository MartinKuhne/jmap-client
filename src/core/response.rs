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

use ahash::AHashMap;
use serde::{de::Visitor, Deserialize};
use std::{fmt, marker::PhantomData};

use crate::{
    blob::copy::CopyBlobResponse,
    email::{
        import::EmailImportResponse, parse::EmailParseResponse,
        search_snippet::SearchSnippetGetResponse, Email,
    },
    email_submission::EmailSubmission,
    identity::Identity,
    mailbox::Mailbox,
    principal::Principal,
    push_subscription::PushSubscription,
    sieve::{validate::SieveScriptValidateResponse, SieveScript},
    thread::Thread,
    vacation_response::VacationResponse,
    Get, Method,
};

use super::{
    changes::ChangesResponse, copy::CopyResponse, error::MethodError, get::GetResponse,
    query::QueryResponse, query_changes::QueryChangesResponse, set::SetResponse,
};

#[derive(Debug, Deserialize)]
pub struct Response<T> {
    #[serde(rename = "methodResponses")]
    method_responses: Vec<T>,

    #[serde(rename = "createdIds")]
    created_ids: Option<AHashMap<String, String>>,

    #[serde(rename = "sessionState")]
    session_state: String,

    request_id: Option<String>,
}

impl<T> Response<T> {
    pub fn new(
        method_responses: Vec<T>,
        created_ids: Option<AHashMap<String, String>>,
        session_state: String,
        request_id: Option<String>,
    ) -> Self {
        Response {
            method_responses,
            created_ids,
            session_state,
            request_id,
        }
    }

    pub fn method_responses(&self) -> &[T] {
        self.method_responses.as_ref()
    }

    pub fn unwrap_method_responses(self) -> Vec<T> {
        self.method_responses
    }

    pub fn method_response_by_pos(&mut self, index: usize) -> T {
        self.method_responses.remove(index)
    }

    pub fn pop_method_response(&mut self) -> Option<T> {
        self.method_responses.pop()
    }

    pub fn created_ids(&self) -> Option<impl Iterator<Item = (&String, &String)>> {
        self.created_ids.as_ref().map(|map| map.iter())
    }

    pub fn session_state(&self) -> &str {
        &self.session_state
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
}

impl Response<TaggedMethodResponse> {
    pub fn method_response_by_id(&self, id: &str) -> Option<&TaggedMethodResponse> {
        self.method_responses
            .iter()
            .find(|response| response.call_id() == id)
    }
}

/// One method response of a JMAP `Response` object (RFC 8620 §3.4).
///
/// An Invocation is `[name, arguments, callId]` (§3.2), and §3.6.2 spells a
/// method-level error as `["error", {…}, callId]`: the only thing that tells
/// the two apart is element 0 being the literal `"error"`, so the arm is
/// decided *structurally*, on element 0 alone, before the payload is looked at.
///
/// That is why this is a hand-written [`Deserialize`] and not
/// `#[serde(untagged)]`. An untagged enum tries the `Error` arm and, on *any*
/// failure of that arm, silently retries the `Ok` arm — and the `Ok` arm binds
/// element 0 as a bare `String`, so `["error", <payload>, "0"]` becomes
/// `Ok(("error", <payload>, "0"))` whenever `<payload>` fails to parse as a
/// [`MethodError`] (no `type`, `type` not a string, payload not an object at
/// all). Against a `SetResponse`, whose every member is optional, that is an
/// all-`None` success: a server refusal reported to the caller as a successful
/// write. Here an `error` Invocation whose payload does not parse is a
/// deserialisation error; it can never become an `Ok`.
#[derive(Debug)]
pub enum SingleMethodResponse<T> {
    Error((Error, MethodError, String)),
    Ok((String, T, String)),
}

impl<T> SingleMethodResponse<T> {
    /// The method call id this response answers (RFC 8620 §3.2: the third
    /// element of the Invocation, equal to the `callId` of the call it
    /// answers).
    pub fn call_id(&self) -> &str {
        match self {
            SingleMethodResponse::Error((_, _, id)) => id,
            SingleMethodResponse::Ok((_, _, id)) => id,
        }
    }
}

impl<'de, T> Deserialize<'de> for SingleMethodResponse<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SingleMethodResponseVisitor(PhantomData))
    }
}

struct SingleMethodResponseVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for SingleMethodResponseVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = SingleMethodResponse<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP method response")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Element 0 picks the arm, and nothing downstream can change that
        // decision: an `error` Invocation whose payload will not parse fails
        // the deserialisation instead of falling through to the success arm.
        let name = seq
            .next_element::<String>()?
            .ok_or_else(|| serde::de::Error::custom("Expected a method name"))?;

        if name == "error" {
            let error = seq
                .next_element::<MethodError>()?
                .ok_or_else(|| serde::de::Error::custom("Expected a method error"))?;

            let id = seq
                .next_element::<String>()?
                .ok_or_else(|| serde::de::Error::custom("Expected method call id"))?;

            Ok(SingleMethodResponse::Error((Error::Error, error, id)))
        } else {
            let response = seq
                .next_element::<T>()?
                .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?;

            let id = seq
                .next_element::<String>()?
                .ok_or_else(|| serde::de::Error::custom("Expected method call id"))?;

            Ok(SingleMethodResponse::Ok((name, response, id)))
        }
    }
}

#[derive(Debug, Deserialize)]
pub enum Error {
    #[serde(rename = "error")]
    Error,
}

pub type PushSubscriptionSetResponse = SetResponse<PushSubscription<Get>>;
pub type PushSubscriptionGetResponse = GetResponse<PushSubscription<Get>>;
pub type MailboxChangesResponse = ChangesResponse<Mailbox<Get>>;
pub type MailboxSetResponse = SetResponse<Mailbox<Get>>;
pub type MailboxGetResponse = GetResponse<Mailbox<Get>>;
pub type ThreadGetResponse = GetResponse<Thread>;
pub type ThreadChangesResponse = ChangesResponse<Thread>;
pub type EmailGetResponse = GetResponse<Email<Get>>;
pub type EmailSetResponse = SetResponse<Email<Get>>;
pub type EmailCopyResponse = CopyResponse<Email<Get>>;
pub type EmailChangesResponse = ChangesResponse<Email<Get>>;
pub type IdentitySetResponse = SetResponse<Identity<Get>>;
pub type IdentityGetResponse = GetResponse<Identity<Get>>;
pub type IdentityChangesResponse = ChangesResponse<Identity<Get>>;
pub type EmailSubmissionSetResponse = SetResponse<EmailSubmission<Get>>;
pub type EmailSubmissionGetResponse = GetResponse<EmailSubmission<Get>>;
pub type EmailSubmissionChangesResponse = ChangesResponse<EmailSubmission<Get>>;
pub type VacationResponseGetResponse = GetResponse<VacationResponse<Get>>;
pub type VacationResponseSetResponse = SetResponse<VacationResponse<Get>>;
pub type SieveScriptGetResponse = GetResponse<SieveScript<Get>>;
pub type SieveScriptSetResponse = SetResponse<SieveScript<Get>>;
pub type PrincipalChangesResponse = ChangesResponse<Principal<Get>>;
pub type PrincipalSetResponse = SetResponse<Principal<Get>>;
pub type PrincipalGetResponse = GetResponse<Principal<Get>>;

#[derive(Debug)]
pub struct TaggedMethodResponse {
    id: String,
    response: MethodResponse,
}

#[derive(Debug)]
pub enum MethodResponse {
    CopyBlob(CopyBlobResponse),
    GetPushSubscription(PushSubscriptionGetResponse),
    SetPushSubscription(PushSubscriptionSetResponse),
    GetMailbox(MailboxGetResponse),
    ChangesMailbox(MailboxChangesResponse),
    QueryMailbox(QueryResponse),
    QueryChangesMailbox(QueryChangesResponse),
    SetMailbox(MailboxSetResponse),
    GetThread(ThreadGetResponse),
    ChangesThread(ThreadChangesResponse),
    GetEmail(EmailGetResponse),
    ChangesEmail(EmailChangesResponse),
    QueryEmail(QueryResponse),
    QueryChangesEmail(QueryChangesResponse),
    SetEmail(EmailSetResponse),
    CopyEmail(EmailCopyResponse),
    ImportEmail(EmailImportResponse),
    ParseEmail(EmailParseResponse),
    GetSearchSnippet(SearchSnippetGetResponse),
    GetIdentity(IdentityGetResponse),
    ChangesIdentity(IdentityChangesResponse),
    SetIdentity(IdentitySetResponse),
    GetEmailSubmission(EmailSubmissionGetResponse),
    ChangesEmailSubmission(EmailSubmissionChangesResponse),
    QueryEmailSubmission(QueryResponse),
    QueryChangesEmailSubmission(QueryChangesResponse),
    SetEmailSubmission(EmailSubmissionSetResponse),
    GetVacationResponse(VacationResponseGetResponse),
    SetVacationResponse(VacationResponseSetResponse),
    GetSieveScript(SieveScriptGetResponse),
    QuerySieveScript(QueryResponse),
    SetSieveScript(SieveScriptSetResponse),
    ValidateSieveScript(SieveScriptValidateResponse),

    GetPrincipal(PrincipalGetResponse),
    ChangesPrincipal(PrincipalChangesResponse),
    QueryPrincipal(QueryResponse),
    QueryChangesPrincipal(QueryChangesResponse),
    SetPrincipal(PrincipalSetResponse),

    Echo(serde_json::Value),
    Error(MethodError),
}

impl TaggedMethodResponse {
    pub fn call_id(&self) -> &str {
        self.id.as_str()
    }

    pub fn is_type(&self, type_: Method) -> bool {
        matches!(
            (&self.response, type_),
            (MethodResponse::CopyBlob(_), Method::CopyBlob)
                | (
                    MethodResponse::GetPushSubscription(_),
                    Method::GetPushSubscription
                )
                | (
                    MethodResponse::SetPushSubscription(_),
                    Method::SetPushSubscription
                )
                | (MethodResponse::GetMailbox(_), Method::GetMailbox)
                | (MethodResponse::ChangesMailbox(_), Method::ChangesMailbox)
                | (MethodResponse::QueryMailbox(_), Method::QueryMailbox)
                | (
                    MethodResponse::QueryChangesMailbox(_),
                    Method::QueryChangesMailbox
                )
                | (MethodResponse::SetMailbox(_), Method::SetMailbox)
                | (MethodResponse::GetThread(_), Method::GetThread)
                | (MethodResponse::ChangesThread(_), Method::ChangesThread)
                | (MethodResponse::GetEmail(_), Method::GetEmail)
                | (MethodResponse::ChangesEmail(_), Method::ChangesEmail)
                | (MethodResponse::QueryEmail(_), Method::QueryEmail)
                | (
                    MethodResponse::QueryChangesEmail(_),
                    Method::QueryChangesEmail
                )
                | (MethodResponse::SetEmail(_), Method::SetEmail)
                | (MethodResponse::CopyEmail(_), Method::CopyEmail)
                | (MethodResponse::ImportEmail(_), Method::ImportEmail)
                | (MethodResponse::ParseEmail(_), Method::ParseEmail)
                | (
                    MethodResponse::GetSearchSnippet(_),
                    Method::GetSearchSnippet
                )
                | (MethodResponse::GetIdentity(_), Method::GetIdentity)
                | (MethodResponse::ChangesIdentity(_), Method::ChangesIdentity)
                | (MethodResponse::SetIdentity(_), Method::SetIdentity)
                | (
                    MethodResponse::GetEmailSubmission(_),
                    Method::GetEmailSubmission
                )
                | (
                    MethodResponse::ChangesEmailSubmission(_),
                    Method::ChangesEmailSubmission
                )
                | (
                    MethodResponse::QueryEmailSubmission(_),
                    Method::QueryEmailSubmission
                )
                | (
                    MethodResponse::QueryChangesEmailSubmission(_),
                    Method::QueryChangesEmailSubmission
                )
                | (
                    MethodResponse::SetEmailSubmission(_),
                    Method::SetEmailSubmission
                )
                | (
                    MethodResponse::GetVacationResponse(_),
                    Method::GetVacationResponse
                )
                | (
                    MethodResponse::SetVacationResponse(_),
                    Method::SetVacationResponse
                )
                | (MethodResponse::GetSieveScript(_), Method::GetSieveScript)
                | (
                    MethodResponse::ValidateSieveScript(_),
                    Method::ValidateSieveScript
                )
                | (
                    MethodResponse::QuerySieveScript(_),
                    Method::QuerySieveScript
                )
                | (MethodResponse::SetSieveScript(_), Method::SetSieveScript)
                | (MethodResponse::GetPrincipal(_), Method::GetPrincipal)
                | (
                    MethodResponse::ChangesPrincipal(_),
                    Method::ChangesPrincipal
                )
                | (MethodResponse::QueryPrincipal(_), Method::QueryPrincipal)
                | (
                    MethodResponse::QueryChangesPrincipal(_),
                    Method::QueryChangesPrincipal
                )
                | (MethodResponse::SetPrincipal(_), Method::SetPrincipal)
                | (MethodResponse::Echo(_), Method::Echo)
                | (MethodResponse::Error(_), Method::Error)
        )
    }

    pub fn unwrap_method_response(self) -> MethodResponse {
        self.response
    }

    pub fn unwrap_copy_blob(self) -> crate::Result<CopyBlobResponse> {
        match self.response {
            MethodResponse::CopyBlob(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_push_subscription(self) -> crate::Result<PushSubscriptionGetResponse> {
        match self.response {
            MethodResponse::GetPushSubscription(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_push_subscription(self) -> crate::Result<PushSubscriptionSetResponse> {
        match self.response {
            MethodResponse::SetPushSubscription(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_mailbox(self) -> crate::Result<MailboxGetResponse> {
        match self.response {
            MethodResponse::GetMailbox(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_mailbox(self) -> crate::Result<MailboxChangesResponse> {
        match self.response {
            MethodResponse::ChangesMailbox(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_mailbox(self) -> crate::Result<QueryResponse> {
        match self.response {
            MethodResponse::QueryMailbox(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_changes_mailbox(self) -> crate::Result<QueryChangesResponse> {
        match self.response {
            MethodResponse::QueryChangesMailbox(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_mailbox(self) -> crate::Result<MailboxSetResponse> {
        match self.response {
            MethodResponse::SetMailbox(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_thread(self) -> crate::Result<ThreadGetResponse> {
        match self.response {
            MethodResponse::GetThread(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_thread(self) -> crate::Result<ThreadChangesResponse> {
        match self.response {
            MethodResponse::ChangesThread(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_email(self) -> crate::Result<EmailGetResponse> {
        match self.response {
            MethodResponse::GetEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_email(self) -> crate::Result<EmailChangesResponse> {
        match self.response {
            MethodResponse::ChangesEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_email(self) -> crate::Result<QueryResponse> {
        match self.response {
            MethodResponse::QueryEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_changes_email(self) -> crate::Result<QueryChangesResponse> {
        match self.response {
            MethodResponse::QueryChangesEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_email(self) -> crate::Result<EmailSetResponse> {
        match self.response {
            MethodResponse::SetEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_copy_email(self) -> crate::Result<EmailCopyResponse> {
        match self.response {
            MethodResponse::CopyEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_import_email(self) -> crate::Result<EmailImportResponse> {
        match self.response {
            MethodResponse::ImportEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_parse_email(self) -> crate::Result<EmailParseResponse> {
        match self.response {
            MethodResponse::ParseEmail(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_search_snippet(self) -> crate::Result<SearchSnippetGetResponse> {
        match self.response {
            MethodResponse::GetSearchSnippet(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_identity(self) -> crate::Result<IdentityGetResponse> {
        match self.response {
            MethodResponse::GetIdentity(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_identity(self) -> crate::Result<IdentityChangesResponse> {
        match self.response {
            MethodResponse::ChangesIdentity(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_identity(self) -> crate::Result<IdentitySetResponse> {
        match self.response {
            MethodResponse::SetIdentity(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_email_submission(self) -> crate::Result<EmailSubmissionGetResponse> {
        match self.response {
            MethodResponse::GetEmailSubmission(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_email_submission(self) -> crate::Result<EmailSubmissionChangesResponse> {
        match self.response {
            MethodResponse::ChangesEmailSubmission(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_email_submission(self) -> crate::Result<EmailSubmissionSetResponse> {
        match self.response {
            MethodResponse::SetEmailSubmission(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_email_submission(self) -> crate::Result<QueryResponse> {
        match self.response {
            MethodResponse::QueryEmailSubmission(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_changes_email_submission(self) -> crate::Result<QueryChangesResponse> {
        match self.response {
            MethodResponse::QueryChangesEmailSubmission(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_vacation_response(self) -> crate::Result<VacationResponseGetResponse> {
        match self.response {
            MethodResponse::GetVacationResponse(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_vacation_response(self) -> crate::Result<VacationResponseSetResponse> {
        match self.response {
            MethodResponse::SetVacationResponse(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_sieve_script(self) -> crate::Result<SieveScriptGetResponse> {
        match self.response {
            MethodResponse::GetSieveScript(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_validate_sieve_script(self) -> crate::Result<SieveScriptValidateResponse> {
        match self.response {
            MethodResponse::ValidateSieveScript(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_sieve_script(self) -> crate::Result<SieveScriptSetResponse> {
        match self.response {
            MethodResponse::SetSieveScript(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_sieve_script(self) -> crate::Result<QueryResponse> {
        match self.response {
            MethodResponse::QuerySieveScript(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_get_principal(self) -> crate::Result<PrincipalGetResponse> {
        match self.response {
            MethodResponse::GetPrincipal(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_changes_principal(self) -> crate::Result<PrincipalChangesResponse> {
        match self.response {
            MethodResponse::ChangesPrincipal(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_principal(self) -> crate::Result<QueryResponse> {
        match self.response {
            MethodResponse::QueryPrincipal(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_query_changes_principal(self) -> crate::Result<QueryChangesResponse> {
        match self.response {
            MethodResponse::QueryChangesPrincipal(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_set_principal(self) -> crate::Result<PrincipalSetResponse> {
        match self.response {
            MethodResponse::SetPrincipal(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn unwrap_echo(self) -> crate::Result<serde_json::Value> {
        match self.response {
            MethodResponse::Echo(response) => Ok(response),
            MethodResponse::Error(err) => Err(err.into()),
            _ => Err("Response type mismatch".into()),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self.response, MethodResponse::Error(_))
    }
}

impl<'de> Deserialize<'de> for TaggedMethodResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(TaggedMethodResponseVisitor)
    }
}

struct TaggedMethodResponseVisitor;

impl<'de> Visitor<'de> for TaggedMethodResponseVisitor {
    type Value = TaggedMethodResponse;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid JMAP method response")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let response = match seq
            .next_element::<Method>()?
            .ok_or_else(|| serde::de::Error::custom("Expected a method name"))?
        {
            Method::Echo => MethodResponse::Echo(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::CopyBlob => MethodResponse::CopyBlob(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetPushSubscription => MethodResponse::GetPushSubscription(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetPushSubscription => MethodResponse::SetPushSubscription(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetMailbox => MethodResponse::GetMailbox(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesMailbox => MethodResponse::ChangesMailbox(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryMailbox => MethodResponse::QueryMailbox(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryChangesMailbox => MethodResponse::QueryChangesMailbox(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetMailbox => MethodResponse::SetMailbox(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetThread => MethodResponse::GetThread(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesThread => MethodResponse::ChangesThread(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetEmail => MethodResponse::GetEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesEmail => MethodResponse::ChangesEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryEmail => MethodResponse::QueryEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryChangesEmail => MethodResponse::QueryChangesEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetEmail => MethodResponse::SetEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::CopyEmail => MethodResponse::CopyEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ImportEmail => MethodResponse::ImportEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ParseEmail => MethodResponse::ParseEmail(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetSearchSnippet => MethodResponse::GetSearchSnippet(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetIdentity => MethodResponse::GetIdentity(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesIdentity => MethodResponse::ChangesIdentity(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetIdentity => MethodResponse::SetIdentity(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetEmailSubmission => MethodResponse::GetEmailSubmission(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesEmailSubmission => MethodResponse::ChangesEmailSubmission(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryEmailSubmission => MethodResponse::QueryEmailSubmission(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryChangesEmailSubmission => MethodResponse::QueryChangesEmailSubmission(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetEmailSubmission => MethodResponse::SetEmailSubmission(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetVacationResponse => MethodResponse::GetVacationResponse(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetVacationResponse => MethodResponse::SetVacationResponse(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetSieveScript => MethodResponse::GetSieveScript(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetSieveScript => MethodResponse::SetSieveScript(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QuerySieveScript => MethodResponse::QuerySieveScript(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ValidateSieveScript => MethodResponse::ValidateSieveScript(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::GetPrincipal => MethodResponse::GetPrincipal(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::ChangesPrincipal => MethodResponse::ChangesPrincipal(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryPrincipal => MethodResponse::QueryPrincipal(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::QueryChangesPrincipal => MethodResponse::QueryChangesPrincipal(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::SetPrincipal => MethodResponse::SetPrincipal(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
            Method::Error => MethodResponse::Error(
                seq.next_element()?
                    .ok_or_else(|| serde::de::Error::custom("Expected a method response"))?,
            ),
        };

        let id = seq
            .next_element::<String>()?
            .ok_or_else(|| serde::de::Error::custom("Expected method call id"))?;

        Ok(TaggedMethodResponse { response, id })
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the JMAP response envelope ([`Response`]) and the
    //! `unwrap_*` accessors of [`TaggedMethodResponse`].
    //!
    //! Every fixture below is a JMAP method response taken from the RFC
    //! examples (cited at the fixture). The tests deserialise the JSON
    //! straight into the types and then drive each accessor and assert the
    //! value that comes back out — no server and no socket involved.

    use super::*;
    use crate::core::error::MethodErrorType;
    use crate::core::set::SetErrorType;
    use crate::mailbox::Property as MailboxProperty;
    use serde_json::json;

    // ------------------------------------------------------------- helpers

    /// Deserialise a single `["Name", {args}, "callId"]` invocation
    /// (RFC 8620 §3.2).
    fn tagged(json: &str) -> TaggedMethodResponse {
        serde_json::from_str(json).expect("fixture must deserialise as a method response")
    }

    /// RFC 8620 §3.6.2 — a method-level error response.
    fn method_error() -> TaggedMethodResponse {
        tagged(r#"["error", {"type": "invalidArguments"}, "c0"]"#)
    }

    /// A well-formed response of a *different* type, which drives the
    /// catch-all arm of every `unwrap_*`.
    fn wrong_type() -> TaggedMethodResponse {
        tagged(r#"["Core/echo", {"seq": 1}, "c0"]"#)
    }

    /// Every `unwrap_*` must (a) surface a method-level error as
    /// `Error::Method` carrying the RFC error type, and (b) reject a response
    /// of another type instead of silently coercing it.
    ///
    /// `unwrap_echo` is deliberately absent: `wrong_type()` *is* an echo.
    macro_rules! assert_error_and_mismatch {
        ($($method:ident),+ $(,)?) => {
            $(
                match method_error().$method() {
                    Err(crate::Error::Method(err)) => assert_eq!(
                        err.error(),
                        &MethodErrorType::InvalidArguments,
                        concat!(stringify!($method), " must carry the method error type")
                    ),
                    other => panic!(concat!(
                        stringify!($method),
                        " must surface a method error, got {:?}"
                    ), other),
                }
                match wrong_type().$method() {
                    Err(err) => assert_eq!(
                        err.to_string(),
                        "Internal error: Response type mismatch",
                        concat!(stringify!($method), " must reject a mismatched response")
                    ),
                    _ => panic!(concat!(
                        stringify!($method),
                        " accepted a response of another type"
                    )),
                }
            )+
        };
    }

    // ------------------------------------------------------------ fixtures

    /// RFC 8620 §6.3 — `Blob/copy`: `copied` maps the blob id in the source
    /// account to the id it got in the destination account.
    const COPY_BLOB: &str = r#"["Blob/copy", {
        "fromAccountId": "A1",
        "accountId": "B1",
        "copied": { "BlobId1": "0a4b-d3v3-4d35-tmp1" },
        "notCopied": {
            "BlobId3": { "type": "notFound", "description": "Blob not found" }
        }
    }, "c0"]"#;

    /// RFC 8620 §7.2.3 — `PushSubscription/get`, verbatim. The method returns
    /// neither `accountId` nor `state` (§7.2.1), so the fixture carries neither.
    const PUSH_SUBSCRIPTION_GET: &str = r#"["PushSubscription/get", {
        "list": [{
            "id": "f2d0aab5-e976-4e8b-ad4b-b380a5b987e4",
            "deviceClientId": "X8980fc",
            "verificationCode": "f3d4618a9ae15c8b7f5582533786d531",
            "expires": "2018-07-12T05:55:00Z",
            "types": ["Mailbox", "Email", "EmailDelivery"]
        }],
        "notFound": []
    }, "c0"]"#;

    /// RFC 8620 §7.2.3 — `PushSubscription/set` create answer. No
    /// `accountId`/`oldState`/`newState`: push subscriptions are not scoped to
    /// an account, so every member of `SetResponse` is absent here.
    const PUSH_SUBSCRIPTION_SET: &str = r#"["PushSubscription/set", {
        "created": {
            "4f29": {
                "id": "P43dcfa4-1dd4-41ef-9156-2c89b3b19c60",
                "keys": null,
                "expires": "2018-07-13T02:14:29Z"
            }
        }
    }, "c0"]"#;

    /// RFC 8621 §2.6 — `Mailbox/get` for the Inbox.
    const MAILBOX_GET: &str = r#"["Mailbox/get", {
        "accountId": "u33084183",
        "state": "78540",
        "list": [{
            "id": "MB23cfa8094c0f41e6",
            "name": "Inbox",
            "parentId": null,
            "role": "inbox",
            "sortOrder": 10,
            "totalEmails": 16307,
            "unreadEmails": 13905,
            "totalThreads": 5833,
            "unreadThreads": 5128,
            "myRights": {
                "mayAddItems": true,
                "mayRename": false,
                "maySubmit": true,
                "mayDelete": false,
                "maySetKeywords": true,
                "mayRemoveItems": true,
                "mayCreateChild": true,
                "maySetSeen": true,
                "mayReadItems": true
            },
            "isSubscribed": true
        }],
        "notFound": ["MB000000000000000000dead"]
    }, "c0"]"#;

    /// RFC 8621 §2.6 — `Mailbox/changes` carrying `updatedProperties`, the
    /// one extra member the Mail changes response has over the standard one.
    const MAILBOX_CHANGES: &str = r#"["Mailbox/changes", {
        "accountId": "u33084183",
        "oldState": "78541",
        "newState": "78542",
        "hasMoreChanges": false,
        "updatedProperties": [
            "totalEmails", "unreadEmails", "totalThreads", "unreadThreads"
        ],
        "created": [],
        "updated": ["MB23cfa8094c0f41e6"],
        "destroyed": []
    }, "c0"]"#;

    /// RFC 8620 §5.5 — a standard `/query` result.
    const MAILBOX_QUERY: &str = r#"["Mailbox/query", {
        "accountId": "u33084183",
        "queryState": "75128aab4b1b",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ["MB23cfa8094c0f41e6", "MB674cc24095db49ce"],
        "total": 2,
        "limit": 10
    }, "c0"]"#;

    /// RFC 8620 §5.6 — a standard `/queryChanges` result.
    const MAILBOX_QUERY_CHANGES: &str = r#"["Mailbox/queryChanges", {
        "accountId": "u33084183",
        "oldQueryState": "y13213",
        "newQueryState": "y13218",
        "removed": ["MB674cc24095db49ce"],
        "added": [{ "id": "MB23cfa8094c0f41e6", "index": 0 }],
        "total": 4
    }, "c0"]"#;

    /// RFC 8621 §2.6 — `Mailbox/set` where the rename succeeded but the
    /// destroy was refused.
    const MAILBOX_SET: &str = r#"["Mailbox/set", {
        "accountId": "u33084183",
        "oldState": "78542",
        "newState": "78549",
        "updated": { "MB674cc24095db49ce": null },
        "notDestroyed": { "MB23cfa8094c0f41e6": { "type": "forbidden" } }
    }, "c0"]"#;

    /// RFC 8621 §3.1.1 — `Thread/get`.
    const THREAD_GET: &str = r#"["Thread/get", {
        "accountId": "acme",
        "state": "f6a7e214",
        "list": [
            { "id": "f123u4", "emailIds": ["eaa623", "f782cbb"] },
            { "id": "f41u44", "emailIds": ["82cf7bb"] }
        ],
        "notFound": []
    }, "c0"]"#;

    /// RFC 8620 §5.2 — a standard `/changes` result (Thread has no extra
    /// members of its own).
    const THREAD_CHANGES: &str = r#"["Thread/changes", {
        "accountId": "acme",
        "oldState": "f6a7e213",
        "newState": "f6a7e214",
        "hasMoreChanges": true,
        "created": ["f41u44"],
        "updated": ["f123u4"],
        "destroyed": []
    }, "c0"]"#;

    /// RFC 8621 §4.2.1 — `Email/get`, trimmed to the members under test.
    const EMAIL_GET: &str = r#"["Email/get", {
        "accountId": "abc",
        "state": "41234123231",
        "list": [{
            "id": "f123u457",
            "threadId": "ef1314a",
            "mailboxIds": { "f123": true },
            "keywords": { "$seen": true, "$flagged": false },
            "from": [{ "name": "Joe Bloggs", "email": "joe@example.com" }],
            "subject": "Dinner on Thursday?",
            "receivedAt": "2013-10-13T14:12:00Z",
            "size": 1024,
            "hasAttachment": false,
            "preview": "Are you free for dinner?"
        }],
        "notFound": ["f123u456"]
    }, "c0"]"#;

    /// RFC 8621 §4.10 — `Email/changes`.
    const EMAIL_CHANGES: &str = r#"["Email/changes", {
        "accountId": "ue150411c",
        "oldState": "780605",
        "newState": "780800",
        "hasMoreChanges": false,
        "created": ["Me8de6c9f6de198239b982ea2"],
        "updated": ["Ma783e5cdf5f2deffbc97930a"],
        "destroyed": ["M9bd17497e2a99cb345fc1d0a"]
    }, "c0"]"#;

    /// RFC 8621 §4.4 — `Email/query` result.
    const EMAIL_QUERY: &str = r#"["Email/query", {
        "accountId": "ue150411c",
        "queryState": "09aa9a075588-780599:0",
        "canCalculateChanges": false,
        "position": 0,
        "ids": ["Ma783e5cdf5f2deffbc97930a"],
        "total": 115,
        "limit": 30
    }, "c0"]"#;

    /// RFC 8621 §4.10 — `Email/queryChanges` with an `AddedItem`.
    const EMAIL_QUERY_CHANGES: &str = r#"["Email/queryChanges", {
        "accountId": "ue150411c",
        "oldQueryState": "09aa9a075588-780599:0",
        "newQueryState": "e35e9facf117-780615:0",
        "added": [{ "id": "Me8de6c9f6de198239b982ea2", "index": 0 }],
        "removed": ["M9bd17497e2a99cb345fc1d0a"],
        "total": 115
    }, "c0"]"#;

    /// RFC 8621 §4.10 — `Email/set` success.
    const EMAIL_SET: &str = r#"["Email/set", {
        "accountId": "ue150411c",
        "oldState": "780605",
        "newState": "780606",
        "updated": { "Ma783e5cdf5f2deffbc97930a": null }
    }, "c0"]"#;

    /// RFC 8621 §4.7 — `Email/copy` (the copy leg only; the RFC's implicit
    /// `Email/set` response is a second invocation, not part of this body).
    const EMAIL_COPY: &str = r#"["Email/copy", {
        "fromAccountId": "ue150411c",
        "accountId": "u6c6c41ac",
        "oldState": "7ee7e9263a6d",
        "newState": "5a0d2447ed26",
        "created": {
            "k45": {
                "id": "M138f9954a5cd2423daeafa55",
                "blobId": "G6b9fb047cba722c48c611e79233d057c6b0b74e8",
                "threadId": "T2f242ea424a4079a",
                "size": 11721
            }
        },
        "notCreated": null
    }, "c0"]"#;

    /// RFC 8621 §4.8 — `Email/import`; the created record carries only
    /// `{id, blobId, threadId, size}`.
    const EMAIL_IMPORT: &str = r#"["Email/import", {
        "accountId": "ue150411c",
        "oldState": "780605",
        "newState": "780606",
        "created": {
            "i0": {
                "id": "M2c1f0f4e5a3b4c8d9e0f1a2",
                "blobId": "S5a3b4c8d9e0f1a2b3c4d5e6",
                "threadId": "T9e0f1a2b3c4d5e6f7a8b9c0",
                "size": 1024
            }
        },
        "notCreated": {
            "i1": { "type": "alreadyExists", "description": "Duplicate message" }
        }
    }, "c0"]"#;

    /// RFC 8621 §4.9 — `Email/parse`.
    const EMAIL_PARSE: &str = r#"["Email/parse", {
        "accountId": "ue150411c",
        "parsed": {
            "Mblob1": {
                "subject": "Parsed message",
                "from": [{ "email": "a@example.com" }]
            }
        },
        "notParsable": ["Mblob2"],
        "notFound": ["Mblob3"]
    }, "c0"]"#;

    /// RFC 8621 §5.2 — `SearchSnippet/get`.
    const SEARCH_SNIPPET_GET: &str = r#"["SearchSnippet/get", {
        "accountId": "ue150411c",
        "list": [{
            "emailId": "M7bcbcb0b58d7729686e83d99",
            "subject": "The <mark>Foo</mark>sball competition",
            "preview": "...year the <mark>foo</mark>sball competition will be held..."
        }, {
            "emailId": "M28d12783a0969584b6deaac0",
            "subject": null,
            "preview": null
        }],
        "notFound": ["M44200ec123de277c0c1ce69c"]
    }, "c0"]"#;

    /// RFC 8621 §6.4 — `Identity/get`, the two records of the example.
    const IDENTITY_GET: &str = r#"["Identity/get", {
        "accountId": "acme",
        "state": "99401312ae-11-333",
        "list": [
            {
                "id": "XD-3301-222-11_22AAz",
                "name": "Joe Bloggs",
                "email": "joe@example.com",
                "replyTo": null,
                "bcc": [{ "name": null, "email": "joe+archive@example.com" }],
                "textSignature": "-- \nJoe Bloggs\nMaster of Email",
                "htmlSignature": "<div><b>Joe Bloggs</b></div>",
                "mayDelete": false
            },
            {
                "id": "XD-9911312-11_22AAz",
                "name": "Joe B",
                "email": "*@example.com",
                "replyTo": null,
                "bcc": null,
                "textSignature": "",
                "htmlSignature": "",
                "mayDelete": true
            }
        ],
        "notFound": []
    }, "c0"]"#;

    /// RFC 8621 §6.2 — `Identity/changes`.
    const IDENTITY_CHANGES: &str = r#"["Identity/changes", {
        "accountId": "acme",
        "oldState": "99401312ae-11-332",
        "newState": "99401312ae-11-333",
        "hasMoreChanges": false,
        "created": ["XD-9911312-11_22AAz"],
        "updated": ["XD-3301-222-11_22AAz"],
        "destroyed": []
    }, "c0"]"#;

    /// RFC 8621 §6.3 — `Identity/set` create.
    const IDENTITY_SET: &str = r#"["Identity/set", {
        "accountId": "acme",
        "oldState": "99401312ae-11-333",
        "newState": "99401312ae-11-334",
        "created": {
            "c0": {
                "id": "XD-new-identity",
                "name": "New Identity",
                "email": "new@example.com",
                "mayDelete": true
            }
        },
        "notCreated": {
            "c1": { "type": "forbiddenFrom", "description": "Not allowed to send as that address" }
        }
    }, "c0"]"#;

    /// RFC 8621 §7 — `EmailSubmission/get`, one delivered and one bounced
    /// submission.
    const EMAIL_SUBMISSION_GET: &str = r#"["EmailSubmission/get", {
        "accountId": "ue411d190",
        "state": "2c3f8a1e-8b9c-4d5e-9f01-23456789abcd",
        "list": [{
            "id": "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0",
            "identityId": "ID-1",
            "emailId": "M7f6ed5bcfd7e2604d1753f6c",
            "threadId": "T2f242ea424a4079a",
            "envelope": {
                "mailFrom": { "email": "joe@example.com" },
                "rcptTo": [{ "email": "bob@example.org" }]
            },
            "sendAt": "2018-07-06T02:14:29Z",
            "undoStatus": "final",
            "deliveryStatus": {
                "bob@example.org": {
                    "smtpReply": "250 2.0.0 Ok: queued",
                    "delivered": "yes",
                    "displayed": "unknown"
                },
                "gone@example.net": {
                    "smtpReply": "550 5.1.1 <gone@example.net>: Recipient address rejected",
                    "delivered": "no",
                    "displayed": "unknown"
                }
            },
            "dsnBlobIds": ["DsnBlob1"],
            "mdnBlobIds": []
        }],
        "notFound": []
    }, "c0"]"#;

    /// RFC 8621 §7.2 — `EmailSubmission/changes`.
    const EMAIL_SUBMISSION_CHANGES: &str = r#"["EmailSubmission/changes", {
        "accountId": "ue411d190",
        "oldState": "2c3f8a1e-8b9c-4d5e-9f01-23456789abce",
        "newState": "2c3f8a1e-8b9c-4d5e-9f01-23456789abcd",
        "hasMoreChanges": false,
        "created": ["ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"],
        "updated": [],
        "destroyed": []
    }, "c0"]"#;

    /// RFC 8621 §7.3 — `EmailSubmission/query`.
    const EMAIL_SUBMISSION_QUERY: &str = r#"["EmailSubmission/query", {
        "accountId": "ue411d190",
        "queryState": "a1b2c3d4:0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ["ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"],
        "total": 1,
        "limit": 250
    }, "c0"]"#;

    /// RFC 8621 §7.4 — `EmailSubmission/queryChanges`.
    const EMAIL_SUBMISSION_QUERY_CHANGES: &str = r#"["EmailSubmission/queryChanges", {
        "accountId": "ue411d190",
        "oldQueryState": "a1b2c3d3:0",
        "newQueryState": "a1b2c3d4:0",
        "removed": [],
        "added": [{
            "id": "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0",
            "index": 0
        }],
        "total": 1
    }, "c0"]"#;

    /// RFC 8621 §7.5.1 — `EmailSubmission/set` create answer.
    const EMAIL_SUBMISSION_SET: &str = r#"["EmailSubmission/set", {
        "accountId": "ue411d190",
        "oldState": "012421s6-8nrq-4ps4-n0p4-9330r951ns21",
        "newState": "355421f6-8aed-4cf4-a0c4-7377e951af36",
        "created": {
            "k1490": {
                "id": "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"
            }
        }
    }, "c0"]"#;

    /// RFC 8621 §8.1 — `VacationResponse/get`, the enabled singleton.
    const VACATION_RESPONSE_GET: &str = r#"["VacationResponse/get", {
        "accountId": "ue150411c",
        "state": "413b1c2a-1",
        "list": [{
            "id": "singleton",
            "isEnabled": true,
            "fromDate": "2019-01-01T00:00:00Z",
            "toDate": "2019-01-31T00:00:00Z",
            "subject": "On vacation",
            "textBody": "I am on vacation.",
            "htmlBody": "<h1>On vacation</h1>"
        }],
        "notFound": []
    }, "c0"]"#;

    /// RFC 8621 §8.2 — `VacationResponse/set` create.
    const VACATION_RESPONSE_SET: &str = r#"["VacationResponse/set", {
        "accountId": "ue150411c",
        "oldState": "413b1c2a-0",
        "newState": "413b1c2a-1",
        "created": {
            "c0": {
                "id": "singleton",
                "isEnabled": true,
                "subject": "On vacation",
                "textBody": "I am on vacation."
            }
        }
    }, "c0"]"#;

    /// draft-ietf-jmap-sieve-12 §2.1.1 — `SieveScript/get`.
    const SIEVE_SCRIPT_GET: &str = r#"["SieveScript/get", {
        "accountId": "ue150411c",
        "state": "1024",
        "list": [{
            "id": "S1",
            "name": "test1",
            "blobId": "0a4b-d3v3-4d35",
            "isActive": true
        }],
        "notFound": ["S404"]
    }, "c0"]"#;

    /// draft-ietf-jmap-sieve-12 §2.2 — `SieveScript/set` create.
    const SIEVE_SCRIPT_SET: &str = r#"["SieveScript/set", {
        "accountId": "ue150411c",
        "oldState": "1023",
        "newState": "1024",
        "created": {
            "c0": { "id": "S1", "name": "test1", "isActive": false }
        }
    }, "c0"]"#;

    /// draft-ietf-jmap-sieve-12 §2.4 — `SieveScript/query`.
    const SIEVE_SCRIPT_QUERY: &str = r#"["SieveScript/query", {
        "accountId": "ue150411c",
        "queryState": "sieve-0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ["S1"],
        "total": 1
    }, "c0"]"#;

    /// draft-ietf-jmap-sieve-12 §2.3 — `SieveScript/validate`. An invalid
    /// script is reported as a *success* response carrying an `error` member,
    /// not as a method error.
    const SIEVE_VALIDATE_ERROR: &str = r#"["SieveScript/validate", {
        "accountId": "ue150411c",
        "error": { "type": "invalidScript", "description": "Unexpected token" }
    }, "c0"]"#;

    /// draft-ietf-jmap-sieve-12 §2.3 — a script that validated cleanly.
    const SIEVE_VALIDATE_OK: &str = r#"["SieveScript/validate", {
        "accountId": "ue150411c"
    }, "c0"]"#;

    /// RFC 9670 §4.1 — `Principal/get`, the `Joe Bloggs` record. The crate
    /// spells the time zone `timezone` and reads the sharing map as `acl`.
    const PRINCIPAL_GET: &str = r#"["Principal/get", {
        "accountId": "P1",
        "state": "7b8eff5zz",
        "list": [{
            "id": "P2342fnddd20",
            "type": "individual",
            "name": "Joe Bloggs",
            "description": null,
            "email": "joe.bloggs@example.com",
            "timezone": "Australia/Melbourne",
            "capabilities": ["urn:ietf:params:jmap:mail"],
            "aliases": ["joe.bloggs@alternate.example.com"],
            "secret": "hunter2",
            "quota": 1048576,
            "picture": "https://example.org/avatar/joe.png",
            "members": [],
            "acl": { "P674pp24095qo49pr": ["mayReadItems", "mayAddItems"] }
        }],
        "notFound": []
    }, "c0"]"#;

    /// RFC 9670 §4.5 — `Principal/changes`.
    const PRINCIPAL_CHANGES: &str = r#"["Principal/changes", {
        "accountId": "P1",
        "oldState": "7b8eff5zy",
        "newState": "7b8eff5zz",
        "hasMoreChanges": false,
        "created": ["P2342fnddd20"],
        "updated": [],
        "destroyed": []
    }, "c0"]"#;

    /// RFC 9670 §4 — `Principal/query`.
    const PRINCIPAL_QUERY: &str = r#"["Principal/query", {
        "accountId": "P1",
        "queryState": "principal-0",
        "canCalculateChanges": true,
        "position": 0,
        "ids": ["P2342fnddd20"],
        "total": 1
    }, "c0"]"#;

    /// RFC 9670 §4 — `Principal/queryChanges`.
    const PRINCIPAL_QUERY_CHANGES: &str = r#"["Principal/queryChanges", {
        "accountId": "P1",
        "oldQueryState": "principal-1",
        "newQueryState": "principal-2",
        "removed": [],
        "added": [{ "id": "P2342fnddd20", "index": 0 }],
        "total": 1
    }, "c0"]"#;

    /// RFC 9670 §4 — `Principal/set` create.
    const PRINCIPAL_SET: &str = r#"["Principal/set", {
        "accountId": "P1",
        "oldState": "4242",
        "newState": "4243",
        "created": {
            "c0": { "id": "P2342fnddd20", "type": "individual", "name": "Joe Bloggs" }
        },
        "notUpdated": {
            "P404": { "type": "notFound", "description": "No such principal" }
        }
    }, "c0"]"#;

    /// RFC 8620 §3.4.1 — the example Response object, with the placeholder
    /// method names replaced by real ones so the envelope can deserialise.
    const ENVELOPE: &str = r#"{
        "methodResponses": [
            ["Mailbox/get", {
                "accountId": "u33084183",
                "state": "78540",
                "list": [],
                "notFound": []
            }, "c1"],
            ["Email/query", {
                "accountId": "u33084183",
                "queryState": "75128aab4b1b",
                "position": 0,
                "ids": []
            }, "c2"],
            ["error", { "type": "unknownMethod" }, "c3"]
        ],
        "sessionState": "75128aab4b1b"
    }"#;

    /// The same envelope with the two optional members present. Note the
    /// `request_id` spelling: `Response::request_id` has no `serde(rename)`,
    /// so it reads `request_id`, not the `requestId` RFC 8887 puts on a WS
    /// frame (that one is carried by `client_ws::WebSocketResponse`).
    const ENVELOPE_WITH_IDS: &str = r#"{
        "methodResponses": [
            ["Mailbox/set", {
                "accountId": "u33084183",
                "oldState": "1",
                "newState": "2",
                "created": { "c0": { "id": "MB1" } }
            }, "c0"]
        ],
        "createdIds": { "c0": "MB1" },
        "sessionState": "75128aab4b1b",
        "request_id": "r-1"
    }"#;

    // ------------------------------------------------------- the envelope

    #[test]
    fn response_envelope_exposes_every_member() {
        let mut response: Response<TaggedMethodResponse> = serde_json::from_str(ENVELOPE).unwrap();

        assert_eq!(response.session_state(), "75128aab4b1b");
        assert_eq!(response.request_id(), None, "absent optional member");
        assert!(response.created_ids().is_none(), "absent optional member");
        assert_eq!(response.method_responses().len(), 3);
        assert_eq!(response.method_responses()[0].call_id(), "c1");
        assert_eq!(response.method_responses()[2].call_id(), "c3");

        // lookup by call id borrows and does not consume
        assert_eq!(
            response.method_response_by_id("c2").unwrap().call_id(),
            "c2"
        );
        assert!(response.method_response_by_id("nope").is_none());
        assert_eq!(response.method_responses().len(), 3);

        // positional lookup removes the entry (it returns `T`, not `Option<T>`)
        assert_eq!(
            response
                .method_response_by_pos(0)
                .unwrap_get_mailbox()
                .unwrap()
                .state(),
            "78540"
        );
        assert_eq!(response.method_responses().len(), 2);

        // pop takes the tail, which is the method error
        let last = response.pop_method_response().unwrap();
        assert!(last.is_error());
        assert_eq!(last.call_id(), "c3");
        assert_eq!(response.method_responses().len(), 1);

        assert_eq!(response.unwrap_method_responses().len(), 1);
    }

    #[test]
    fn response_optional_members_when_present() {
        let response: Response<TaggedMethodResponse> =
            serde_json::from_str(ENVELOPE_WITH_IDS).unwrap();

        assert_eq!(response.request_id(), Some("r-1"));
        assert_eq!(
            response
                .created_ids()
                .unwrap()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            vec![("c0", "MB1")]
        );
    }

    #[test]
    fn response_new_keeps_what_it_is_given() {
        let mut created_ids = AHashMap::new();
        created_ids.insert("c0".to_string(), "MB1".to_string());

        let response = Response::<TaggedMethodResponse>::new(
            vec![tagged(MAILBOX_GET)],
            Some(created_ids),
            "state-1".to_string(),
            Some("r-2".to_string()),
        );

        assert_eq!(response.session_state(), "state-1");
        assert_eq!(response.request_id(), Some("r-2"));
        assert_eq!(response.created_ids().unwrap().count(), 1);
        assert_eq!(response.method_responses().len(), 1);
        assert_eq!(response.method_responses()[0].call_id(), "c0");
    }

    /// RFC 8620 §3.4.3: a single method response is untagged, discriminated
    /// only by the method name being the literal `"error"`.
    ///
    /// Each case is one guarded `matches!`, so the field values are asserted
    /// without an `if let` whose implicit else branch could never be taken.
    #[test]
    fn single_method_response_is_untagged_ok_or_error() {
        let ok = serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(
            r#"["Mailbox/get", {"a": 1}, "c1"]"#,
        )
        .unwrap();
        let err = serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(
            r#"["error", {"type": "serverFail"}, "c2"]"#,
        )
        .unwrap();

        assert!(
            matches!(
                &ok,
                SingleMethodResponse::Ok((name, args, id))
                    if name == "Mailbox/get" && *args == json!({ "a": 1 }) && id == "c1"
            ),
            "a non-`error` method name must land in the Ok arm, got {ok:?}"
        );
        assert!(
            matches!(
                &err,
                SingleMethodResponse::Error((kind, method, id))
                    if matches!(kind, Error::Error)
                        && matches!(method.error(), MethodErrorType::ServerFail)
                        && id == "c2"
            ),
            "`error` must land in the Error arm, got {err:?}"
        );
    }

    /// RFC 8620 §3.6.2 — an `error` Invocation must never read as a success.
    ///
    /// This is the root cause of Finding #112. Under `#[serde(untagged)]` serde
    /// tried the `Error` arm and, on *any* failure of that arm, silently retried
    /// the `Ok` arm — which binds element 0 as a bare `String`, so
    /// `["error", <payload>, "0"]` became `Ok(("error", <payload>, "0"))`
    /// whenever `<payload>` was not a valid [`MethodError`]. Every member of a
    /// `SetResponse` is optional, so against a `Set` call the server's refusal
    /// deserialised into an all-`None` success and the caller was told the write
    /// had worked. The arm is now chosen on element 0 alone, so a payload that
    /// will not parse fails the deserialisation instead.
    #[test]
    fn a_malformed_error_invocation_never_becomes_an_all_none_set_success() {
        // Each case is an `error` Invocation body for a `Mailbox/set` call. The
        // second field is the part of the deserialisation error that names the
        // problem; an empty one means the payload *is* a valid `MethodError`.
        let cases = [
            ("{}", "missing field"),
            (r#"{"type": 42}"#, "invalid type: integer `42`"),
            (r#"{"type": "serverFail", "description": 999}"#, ""),
            (r#""serverFail""#, "invalid type: string"),
            ("null", "invalid type: null"),
        ];

        for (payload, reason) in cases {
            let json = format!(r#"["error", {payload}, "0"]"#);
            let parsed = serde_json::from_str::<SingleMethodResponse<MailboxSetResponse>>(&json);

            if reason.is_empty() {
                match &parsed.expect("a well-formed method error must parse") {
                    SingleMethodResponse::Error((_, error, id)) => {
                        assert_eq!(
                            error.error(),
                            &MethodErrorType::ServerFail,
                            "{json} must reach the caller as its own error type"
                        );
                        assert_eq!(id, "0");
                    }
                    response => {
                        panic!("an `error` Invocation must not land in the Ok arm: {response:?}")
                    }
                }
            } else {
                let error = parsed.expect_err(&format!(
                    "{json} must not deserialise as a Set response: a refusal \
                     reported as a successful write"
                ));
                assert!(
                    error.to_string().contains(reason),
                    "{json} must report {reason:?}, got {error}"
                );
            }
        }

        // The fix must not swallow the honest case: a real `Mailbox/set` body
        // still arrives in the `Ok` arm (RFC 8621 §2.6 example).
        assert!(matches!(
            serde_json::from_str::<SingleMethodResponse<MailboxSetResponse>>(
                r#"["Mailbox/set", {
                    "accountId": "u33084183",
                    "oldState": "78542",
                    "newState": "78549",
                    "updated": { "mf2": null }
                }, "0"]"#,
            )
            .expect("a well-formed Set response must deserialise"),
            SingleMethodResponse::Ok(_)
        ));
    }

    /// RFC 8620 §3.2 fixes an Invocation at `[name, arguments, callId]`, and the
    /// hand-written visitor keeps that shape for both arms: anything that is not
    /// a three-element array is rejected with the visitor's own message.
    #[test]
    fn a_single_method_response_must_be_a_three_element_invocation() {
        let error =
            serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(r#"{"not": "a seq"}"#)
                .expect_err("a JMAP method response must be an array");
        assert_eq!(
            error.to_string(),
            "invalid type: map, expected a valid JMAP method response at line 1 column 0"
        );

        for (json, expected) in [
            (r#"[]"#, "Expected a method name at line 1 column 2"),
            (r#"["error"]"#, "Expected a method error at line 1 column"),
            (
                r#"["error", {"type": "forbidden"}]"#,
                "Expected method call id",
            ),
            (r#"["Mailbox/set", {}]"#, "Expected method call id"),
            (r#"["Mailbox/set"]"#, "Expected a method response"),
            (
                r#"[42, {}, "c0"]"#,
                "invalid type: integer `42`, expected a string",
            ),
        ] {
            let error = serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(json)
                .expect_err(&format!("{json} is not a valid Invocation"));
            assert!(
                error.to_string().contains(expected),
                "{json} must report {expected:?}, got {error}"
            );
        }

        // A fourth element is trailing garbage, not a longer Invocation.
        assert!(
            serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(
                r#"["Mailbox/set", {}, "c0", "extra"]"#
            )
            .is_err()
        );
    }

    /// The call id is the third element of the Invocation in both arms, which is
    /// what lets [`Request::send_single`](crate::core::request::Request::send_single)
    /// pick the response that answers its own call.
    #[test]
    fn call_id_reads_the_third_element_of_either_arm() {
        let ok = serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(
            r#"["Mailbox/get", {"a": 1}, "c1"]"#,
        )
        .unwrap();
        let err = serde_json::from_str::<SingleMethodResponse<serde_json::Value>>(
            r#"["error", {"type": "serverFail"}, "c2"]"#,
        )
        .unwrap();

        assert_eq!(ok.call_id(), "c1");
        assert_eq!(err.call_id(), "c2");
    }

    // ------------------------------------------------------- blob + push

    #[test]
    fn copy_blob_response_accessors() {
        let mut response = tagged(COPY_BLOB).unwrap_copy_blob().unwrap();

        assert_eq!(response.from_account_id(), "A1");
        assert_eq!(response.account_id(), "B1");
        assert_eq!(
            response.copied_ids().unwrap().cloned().collect::<Vec<_>>(),
            vec!["BlobId1"]
        );
        assert_eq!(
            response
                .not_copied_ids()
                .unwrap()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["BlobId3"]
        );
        assert_eq!(
            response.not_copied_reason("BlobId3").unwrap().error(),
            &SetErrorType::NotFound
        );
        assert_eq!(
            response.not_copied_reason("BlobId3").unwrap().description(),
            Some("Blob not found")
        );
        assert!(response.not_copied_reason("BlobId1").is_none());

        assert_eq!(response.copied("BlobId1").unwrap(), "0a4b-d3v3-4d35-tmp1");
        // the lookup removes, so the id is gone afterwards
        assert!(matches!(
            response.copied("BlobId1"),
            Err(crate::Error::Internal(_))
        ));
        // a blob that failed reports its SetError
        assert_eq!(
            response.copied("BlobId3").unwrap_err().to_string(),
            "Set failed: notFound: Blob not found"
        );
    }

    #[test]
    fn push_subscription_get_response_accessors() {
        let mut response = tagged(PUSH_SUBSCRIPTION_GET)
            .unwrap_get_push_subscription()
            .unwrap();

        assert_eq!(response.account_id(), None, "push is not account scoped");
        assert_eq!(
            response.state(),
            "",
            "RFC 8620 §7.2.1: the method returns no state"
        );
        assert_eq!(response.list().len(), 1);
        assert_eq!(response.not_found(), &[] as &[String]);

        let subscription = &response.list()[0];
        assert_eq!(
            subscription.id().unwrap(),
            "f2d0aab5-e976-4e8b-ad4b-b380a5b987e4"
        );
        assert_eq!(
            subscription.types(),
            Some(
                &[
                    crate::DataType::Mailbox,
                    crate::DataType::Email,
                    crate::DataType::EmailDelivery
                ][..]
            )
        );

        assert_eq!(
            response.take_state(),
            "",
            "nothing was taken from an absent state"
        );
        assert_eq!(response.state(), "");
        assert_eq!(response.take_list().len(), 1);
        assert!(response.list().is_empty());
        assert!(response.take_not_found().is_empty());
    }

    #[test]
    fn push_subscription_set_response_accessors() {
        let mut response = tagged(PUSH_SUBSCRIPTION_SET)
            .unwrap_set_push_subscription()
            .unwrap();

        // every member but `created` is absent
        assert_eq!(response.account_id(), None);
        assert_eq!(response.old_state(), None);
        assert_eq!(response.new_state(), "", "absent newState reads as empty");
        assert_eq!(response.updated_ids().map(|i| i.count()), None);
        assert_eq!(response.destroyed_ids().map(|i| i.count()), None);
        assert_eq!(response.not_created_ids().map(|i| i.count()), None);
        assert!(!response.has_updated());
        assert!(!response.has_destroyed());
        assert!(response.has_created(), "`created` is the only payload");
        assert!(response.unwrap_create_errors().is_ok());
        assert!(response.unwrap_update_errors().is_ok());

        assert_eq!(
            response.created_ids().unwrap().cloned().collect::<Vec<_>>(),
            vec!["4f29"]
        );
        let created = response.created("4f29").unwrap();
        assert_eq!(
            created.id().unwrap(),
            "P43dcfa4-1dd4-41ef-9156-2c89b3b19c60"
        );
        assert!(created.keys().is_none(), "`keys` was explicitly null");
        assert!(matches!(
            response.created("4f29"),
            Err(crate::Error::Internal(_))
        ));
        assert_eq!(response.take_new_state(), "");
    }

    // --------------------------------------------------------- mailboxes

    #[test]
    fn mailbox_get_response_accessors() {
        let mut response = tagged(MAILBOX_GET).unwrap_get_mailbox().unwrap();

        assert_eq!(response.account_id(), Some("u33084183"));
        assert_eq!(response.state(), "78540");
        assert_eq!(response.list().len(), 1);
        assert_eq!(response.not_found(), ["MB000000000000000000dead"]);

        let mailbox = &response.list()[0];
        assert_eq!(mailbox.id(), Some("MB23cfa8094c0f41e6"));
        assert_eq!(mailbox.name(), Some("Inbox"));
        assert_eq!(mailbox.parent_id(), None);
        assert_eq!(mailbox.role(), crate::mailbox::Role::Inbox);
        assert_eq!(mailbox.sort_order(), 10);
        assert_eq!(mailbox.total_emails(), 16307);
        assert_eq!(mailbox.unread_emails(), 13905);
        assert_eq!(mailbox.total_threads(), 5833);
        assert_eq!(mailbox.unread_threads(), 5128);
        assert!(mailbox.is_subscribed());
        assert!(mailbox.my_rights().unwrap().may_add_items());
        assert!(!mailbox.my_rights().unwrap().may_rename());

        assert_eq!(response.pop().unwrap().name(), Some("Inbox"));
        assert!(response.pop().is_none());
    }

    #[test]
    fn mailbox_changes_response_accessors() {
        let mut response = tagged(MAILBOX_CHANGES).unwrap_changes_mailbox().unwrap();

        assert_eq!(response.account_id(), "u33084183");
        assert_eq!(response.old_state(), "78541");
        assert_eq!(response.new_state(), "78542");
        assert!(!response.has_more_changes());
        assert!(response.created().is_empty());
        assert_eq!(response.updated(), ["MB23cfa8094c0f41e6"]);
        assert!(response.destroyed().is_empty());
        assert_eq!(response.total_changes(), 1);
        assert_eq!(
            response.arguments().updated_properties().unwrap(),
            [
                MailboxProperty::TotalEmails,
                MailboxProperty::UnreadEmails,
                MailboxProperty::TotalThreads,
                MailboxProperty::UnreadThreads
            ]
        );

        assert_eq!(response.take_account_id(), "u33084183");
        assert_eq!(response.take_new_state(), "78542");
        assert_eq!(
            response.take_updated(),
            vec!["MB23cfa8094c0f41e6".to_string()]
        );
        assert!(response.take_created().is_empty());
        assert!(response.take_destroyed().is_empty());
    }

    #[test]
    fn mailbox_query_response_accessors() {
        let mut response = tagged(MAILBOX_QUERY).unwrap_query_mailbox().unwrap();

        assert_eq!(response.account_id(), "u33084183");
        assert_eq!(response.query_state(), "75128aab4b1b");
        assert!(response.can_calculate_changes());
        assert_eq!(response.position(), 0);
        assert_eq!(response.ids().len(), 2);
        assert_eq!(response.id(1), "MB674cc24095db49ce");
        assert_eq!(response.total(), Some(2));
        assert_eq!(response.limit(), Some(10));

        assert_eq!(response.take_query_state(), "75128aab4b1b");
        assert_eq!(response.query_state(), "");
        assert_eq!(response.take_ids().len(), 2);
        assert!(response.ids().is_empty());
    }

    #[test]
    fn mailbox_query_changes_response_accessors() {
        let response = tagged(MAILBOX_QUERY_CHANGES)
            .unwrap_query_changes_mailbox()
            .unwrap();

        assert_eq!(response.account_id(), "u33084183");
        assert_eq!(response.old_query_state(), "y13213");
        assert_eq!(response.new_query_state(), "y13218");
        assert_eq!(response.total(), Some(4));
        assert_eq!(response.removed(), ["MB674cc24095db49ce"]);
        assert_eq!(response.added().len(), 1);
        assert_eq!(response.added()[0].id(), "MB23cfa8094c0f41e6");
        assert_eq!(response.added()[0].index(), 0);
    }

    #[test]
    fn mailbox_set_response_accessors() {
        let mut response = tagged(MAILBOX_SET).unwrap_set_mailbox().unwrap();

        assert_eq!(response.account_id(), Some("u33084183"));
        assert_eq!(response.old_state(), Some("78542"));
        assert_eq!(response.new_state(), "78549");
        assert!(response.has_updated());
        assert!(!response.has_created());
        assert!(!response.has_destroyed());
        // This response carries no `notUpdated` member at all.
        assert_eq!(response.not_updated_ids().map(|i| i.count()), None);
        assert_eq!(
            response
                .not_destroyed_ids()
                .unwrap()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["MB23cfa8094c0f41e6"]
        );

        // the RFC reports the update with a null object
        assert!(
            response.updated("MB674cc24095db49ce").unwrap().is_none(),
            "updated entry is explicitly null"
        );
        assert!(response.unwrap_update_errors().is_ok());
        assert!(response.unwrap_create_errors().is_ok());

        // the refused destroy surfaces the SetError
        assert_eq!(
            response
                .destroyed("MB23cfa8094c0f41e6")
                .unwrap_err()
                .to_string(),
            "Set failed: forbidden"
        );
        assert!(matches!(
            response.destroyed("MBunknown"),
            Err(crate::Error::Internal(_))
        ));
        // `updated()` removed the entry it handed out, so nothing is left
        assert_eq!(response.take_updated_ids().unwrap(), Vec::<String>::new());
        assert_eq!(response.take_destroyed_ids(), None);
    }

    // --------------------------------------------------------- threads

    #[test]
    fn thread_get_response_accessors() {
        let mut response = tagged(THREAD_GET).unwrap_get_thread().unwrap();

        assert_eq!(response.account_id(), Some("acme"));
        assert_eq!(response.state(), "f6a7e214");
        assert_eq!(response.list().len(), 2);
        assert!(response.not_found().is_empty());
        assert_eq!(response.list()[0].id(), "f123u4");
        assert_eq!(response.list()[0].email_ids(), ["eaa623", "f782cbb"]);
        assert_eq!(response.list()[1].email_ids(), ["82cf7bb"]);

        // `pop` returns the *last* record, not the first
        assert_eq!(response.pop().unwrap().id(), "f41u44");
    }

    #[test]
    fn thread_changes_response_accessors() {
        let response = tagged(THREAD_CHANGES).unwrap_changes_thread().unwrap();

        assert_eq!(response.account_id(), "acme");
        assert_eq!(response.old_state(), "f6a7e213");
        assert_eq!(response.new_state(), "f6a7e214");
        assert!(response.has_more_changes());
        assert_eq!(response.created(), ["f41u44"]);
        assert_eq!(response.updated(), ["f123u4"]);
        assert!(response.destroyed().is_empty());
        assert_eq!(response.total_changes(), 2);
    }

    // ----------------------------------------------------------- emails

    #[test]
    fn email_get_response_accessors() {
        let mut response = tagged(EMAIL_GET).unwrap_get_email().unwrap();

        assert_eq!(response.account_id(), Some("abc"));
        assert_eq!(response.state(), "41234123231");
        assert_eq!(response.list().len(), 1);
        assert_eq!(response.not_found(), ["f123u456"]);

        let email = &response.list()[0];
        assert_eq!(email.id(), Some("f123u457"));
        assert_eq!(email.thread_id(), Some("ef1314a"));
        assert_eq!(email.mailbox_ids(), ["f123"]);
        // only the keywords whose value is true are reported
        assert_eq!(email.keywords(), ["$seen"]);
        assert_eq!(email.subject(), Some("Dinner on Thursday?"));
        assert_eq!(email.size(), 1024);
        assert!(!email.has_attachment());
        assert_eq!(email.preview(), Some("Are you free for dinner?"));
        assert_eq!(email.from().unwrap()[0].email(), "joe@example.com");
        assert_eq!(email.from().unwrap()[0].name(), Some("Joe Bloggs"));

        assert_eq!(response.take_list().len(), 1);
        assert_eq!(response.take_not_found(), vec!["f123u456".to_string()]);
    }

    #[test]
    fn email_changes_response_accessors() {
        let response = tagged(EMAIL_CHANGES).unwrap_changes_email().unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.old_state(), "780605");
        assert_eq!(response.new_state(), "780800");
        assert!(!response.has_more_changes());
        assert_eq!(response.created(), ["Me8de6c9f6de198239b982ea2"]);
        assert_eq!(response.updated(), ["Ma783e5cdf5f2deffbc97930a"]);
        assert_eq!(response.destroyed(), ["M9bd17497e2a99cb345fc1d0a"]);
        assert_eq!(response.total_changes(), 3);
    }

    #[test]
    fn email_query_response_accessors() {
        let response = tagged(EMAIL_QUERY).unwrap_query_email().unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.query_state(), "09aa9a075588-780599:0");
        assert!(!response.can_calculate_changes());
        assert_eq!(response.position(), 0);
        assert_eq!(response.ids(), ["Ma783e5cdf5f2deffbc97930a"]);
        assert_eq!(response.total(), Some(115));
        assert_eq!(response.limit(), Some(30));
    }

    #[test]
    fn email_query_changes_response_accessors() {
        let response = tagged(EMAIL_QUERY_CHANGES)
            .unwrap_query_changes_email()
            .unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.old_query_state(), "09aa9a075588-780599:0");
        assert_eq!(response.new_query_state(), "e35e9facf117-780615:0");
        assert_eq!(response.total(), Some(115));
        assert_eq!(response.removed(), ["M9bd17497e2a99cb345fc1d0a"]);
        assert_eq!(response.added().len(), 1);
        assert_eq!(response.added()[0].id(), "Me8de6c9f6de198239b982ea2");
        assert_eq!(response.added()[0].index(), 0);
    }

    #[test]
    fn email_set_response_accessors() {
        let mut response = tagged(EMAIL_SET).unwrap_set_email().unwrap();

        assert_eq!(response.account_id(), Some("ue150411c"));
        assert_eq!(response.old_state(), Some("780605"));
        assert_eq!(response.new_state(), "780606");
        assert!(response.has_updated());
        assert_eq!(
            response.updated_ids().unwrap().cloned().collect::<Vec<_>>(),
            vec!["Ma783e5cdf5f2deffbc97930a"]
        );
        assert!(response
            .updated("Ma783e5cdf5f2deffbc97930a")
            .unwrap()
            .is_none());
        assert!(matches!(
            response.updated("Mnope"),
            Err(crate::Error::Internal(_))
        ));
    }

    #[test]
    fn email_copy_response_accessors() {
        let mut response = tagged(EMAIL_COPY).unwrap_copy_email().unwrap();

        assert_eq!(response.from_account_id(), "ue150411c");
        assert_eq!(response.account_id(), "u6c6c41ac");
        assert_eq!(response.old_state(), Some("7ee7e9263a6d"));
        assert_eq!(response.new_state(), "5a0d2447ed26");
        assert_eq!(
            response.created_ids().unwrap().cloned().collect::<Vec<_>>(),
            vec!["k45"]
        );
        // `notCreated` was explicitly null on the wire.
        assert_eq!(response.not_created_ids().map(|i| i.count()), None);

        let created = response.created("k45").unwrap();
        assert_eq!(created.id(), Some("M138f9954a5cd2423daeafa55"));
        assert_eq!(created.thread_id(), Some("T2f242ea424a4079a"));
        assert_eq!(created.size(), 11721);
        assert!(matches!(
            response.created("k45"),
            Err(crate::Error::Internal(_))
        ));
    }

    #[test]
    fn email_import_response_accessors() {
        let mut response = tagged(EMAIL_IMPORT).unwrap_import_email().unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.old_state(), Some("780605"));
        assert_eq!(response.new_state(), "780606");
        assert_eq!(
            response.created_ids().unwrap().cloned().collect::<Vec<_>>(),
            vec!["i0"]
        );
        assert_eq!(
            response
                .not_created_ids()
                .unwrap()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["i1"]
        );

        let created = response.created("i0").unwrap();
        assert_eq!(created.id(), Some("M2c1f0f4e5a3b4c8d9e0f1a2"));
        assert_eq!(created.blob_id(), Some("S5a3b4c8d9e0f1a2b3c4d5e6"));
        // the created record carries only id/blobId/threadId/size
        assert!(created.mailbox_ids().is_empty());
        assert_eq!(
            response.created("i1").unwrap_err().to_string(),
            "Set failed: alreadyExists: Duplicate message"
        );
        assert_eq!(response.take_new_state(), "780606");
    }

    #[test]
    fn email_parse_response_accessors() {
        let mut response = tagged(EMAIL_PARSE).unwrap_parse_email().unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        let parsed = response.parsed("Mblob1").unwrap();
        assert_eq!(parsed.subject(), Some("Parsed message"));
        assert_eq!(parsed.from().unwrap()[0].email(), "a@example.com");

        // a blob the server could not parse is an error, as is one it never saw
        assert!(response.parsed("Mblob2").is_err());
        assert!(response.parsed("Mblob3").is_err());
    }

    #[test]
    fn search_snippet_get_response_accessors() {
        let response = tagged(SEARCH_SNIPPET_GET)
            .unwrap_get_search_snippet()
            .unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.list().len(), 2);
        assert_eq!(response.not_found().unwrap(), ["M44200ec123de277c0c1ce69c"]);

        let hit = response.snippet("M7bcbcb0b58d7729686e83d99").unwrap();
        assert_eq!(hit.email_id(), "M7bcbcb0b58d7729686e83d99");
        assert_eq!(hit.subject(), Some("The <mark>Foo</mark>sball competition"));
        assert!(hit.preview().unwrap().contains("<mark>foo</mark>"));

        // a snippet with both members null deserialises, and finds nothing extra
        let miss = response.snippet("M28d12783a0969584b6deaac0").unwrap();
        assert_eq!(miss.subject(), None);
        assert_eq!(miss.preview(), None);
        assert!(response.snippet("Munknown").is_none());
    }

    // ------------------------------------------------------ identities

    #[test]
    fn identity_get_response_accessors() {
        let response = tagged(IDENTITY_GET).unwrap_get_identity().unwrap();

        assert_eq!(response.account_id(), Some("acme"));
        assert_eq!(response.state(), "99401312ae-11-333");
        assert_eq!(response.list().len(), 2);
        assert!(response.not_found().is_empty());

        let exact = &response.list()[0];
        assert_eq!(exact.email.as_deref(), Some("joe@example.com"));
        assert_eq!(exact.name.as_deref(), Some("Joe Bloggs"));
        assert!(exact.reply_to.is_none(), "explicit null list reads as None");
        assert_eq!(
            exact.bcc.as_ref().unwrap()[0].email(),
            "joe+archive@example.com"
        );
        assert_eq!(exact.may_delete, Some(false));

        let wildcard = &response.list()[1];
        assert_eq!(wildcard.email.as_deref(), Some("*@example.com"));
        assert!(wildcard.bcc.is_none());
        assert_eq!(wildcard.may_delete, Some(true));
    }

    #[test]
    fn identity_changes_response_accessors() {
        let response = tagged(IDENTITY_CHANGES).unwrap_changes_identity().unwrap();

        assert_eq!(response.account_id(), "acme");
        assert_eq!(response.old_state(), "99401312ae-11-332");
        assert_eq!(response.new_state(), "99401312ae-11-333");
        assert!(!response.has_more_changes());
        assert_eq!(response.created(), ["XD-9911312-11_22AAz"]);
        assert_eq!(response.updated(), ["XD-3301-222-11_22AAz"]);
        assert!(response.destroyed().is_empty());
        assert_eq!(response.total_changes(), 2);
    }

    #[test]
    fn identity_set_response_accessors() {
        let mut response = tagged(IDENTITY_SET).unwrap_set_identity().unwrap();

        assert_eq!(response.account_id(), Some("acme"));
        assert_eq!(response.new_state(), "99401312ae-11-334");
        assert!(response.has_created());
        assert_eq!(
            response.created("c0").unwrap().email.as_deref(),
            Some("new@example.com")
        );
        assert!(response.unwrap_create_errors().is_err());
        assert_eq!(
            response
                .not_created_ids()
                .unwrap()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["c1"]
        );
        assert_eq!(
            response.created("c1").unwrap_err().to_string(),
            "Set failed: forbiddenFrom: Not allowed to send as that address"
        );
    }

    // ------------------------------------------------- email submission

    #[test]
    fn email_submission_get_response_accessors() {
        let response = tagged(EMAIL_SUBMISSION_GET)
            .unwrap_get_email_submission()
            .unwrap();

        assert_eq!(response.account_id(), Some("ue411d190"));
        assert_eq!(response.list().len(), 1);

        let submission = &response.list()[0];
        assert_eq!(
            submission.id(),
            Some("ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0")
        );
        assert_eq!(submission.identity_id(), Some("ID-1"));
        assert_eq!(submission.email_id(), Some("M7f6ed5bcfd7e2604d1753f6c"));
        assert_eq!(
            submission.undo_status(),
            Some(&crate::email_submission::UndoStatus::Final)
        );
        assert_eq!(submission.mail_from().unwrap().email(), "joe@example.com");
        assert_eq!(submission.rcpt_to().unwrap().len(), 1);
        assert_eq!(
            submission
                .delivery_status()
                .unwrap()
                .get("bob@example.org")
                .unwrap()
                .delivered(),
            &crate::email_submission::Delivered::Yes
        );
        assert_eq!(
            submission
                .delivery_status()
                .unwrap()
                .get("gone@example.net")
                .unwrap()
                .delivered(),
            &crate::email_submission::Delivered::No,
            "a bounce is a delivered:no entry carrying the SMTP reply"
        );
        assert_eq!(
            submission
                .delivery_status()
                .unwrap()
                .get("gone@example.net")
                .unwrap()
                .smtp_reply(),
            "550 5.1.1 <gone@example.net>: Recipient address rejected"
        );
        assert_eq!(
            submission.dsn_blob_ids(),
            Some(&["DsnBlob1".to_string()][..])
        );
    }

    #[test]
    fn email_submission_changes_response_accessors() {
        let response = tagged(EMAIL_SUBMISSION_CHANGES)
            .unwrap_changes_email_submission()
            .unwrap();

        assert_eq!(response.account_id(), "ue411d190");
        assert_eq!(response.old_state(), "2c3f8a1e-8b9c-4d5e-9f01-23456789abce");
        assert_eq!(response.new_state(), "2c3f8a1e-8b9c-4d5e-9f01-23456789abcd");
        assert!(!response.has_more_changes());
        assert_eq!(
            response.created(),
            ["ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"]
        );
        assert!(response.updated().is_empty());
        assert!(response.destroyed().is_empty());
        assert_eq!(response.total_changes(), 1);
    }

    #[test]
    fn email_submission_query_response_accessors() {
        let response = tagged(EMAIL_SUBMISSION_QUERY)
            .unwrap_query_email_submission()
            .unwrap();

        assert_eq!(response.account_id(), "ue411d190");
        assert_eq!(response.query_state(), "a1b2c3d4:0");
        assert!(response.can_calculate_changes());
        assert_eq!(response.position(), 0);
        assert_eq!(response.id(0), "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0");
        assert_eq!(response.total(), Some(1));
        assert_eq!(response.limit(), Some(250));
    }

    #[test]
    fn email_submission_query_changes_response_accessors() {
        let response = tagged(EMAIL_SUBMISSION_QUERY_CHANGES)
            .unwrap_query_changes_email_submission()
            .unwrap();

        assert_eq!(response.account_id(), "ue411d190");
        assert_eq!(response.old_query_state(), "a1b2c3d3:0");
        assert_eq!(response.new_query_state(), "a1b2c3d4:0");
        assert_eq!(response.total(), Some(1));
        assert!(response.removed().is_empty());
        assert_eq!(response.added().len(), 1);
        assert_eq!(
            response.added()[0].id(),
            "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"
        );
        assert_eq!(response.added()[0].index(), 0);
    }

    #[test]
    fn email_submission_set_response_accessors() {
        let mut response = tagged(EMAIL_SUBMISSION_SET)
            .unwrap_set_email_submission()
            .unwrap();

        assert_eq!(response.account_id(), Some("ue411d190"));
        assert_eq!(
            response.old_state(),
            Some("012421s6-8nrq-4ps4-n0p4-9330r951ns21")
        );
        assert_eq!(response.new_state(), "355421f6-8aed-4cf4-a0c4-7377e951af36");
        assert!(response.has_created());
        assert_eq!(
            response.created("k1490").unwrap().id().unwrap(),
            "ES-3bab7f9a-623e-4acf-99a5-2e67facb02a0"
        );
        assert!(response.unwrap_create_errors().is_ok());
        assert_eq!(
            response.take_new_state(),
            "355421f6-8aed-4cf4-a0c4-7377e951af36"
        );
    }

    // ------------------------------------------------ vacation response

    #[test]
    fn vacation_response_get_response_accessors() {
        let response = tagged(VACATION_RESPONSE_GET)
            .unwrap_get_vacation_response()
            .unwrap();

        assert_eq!(response.account_id(), Some("ue150411c"));
        assert_eq!(response.state(), "413b1c2a-1");
        assert_eq!(response.list().len(), 1);
        assert_eq!(response.list()[0].id().unwrap(), "singleton");
        assert!(response.list()[0].is_enabled());
        assert_eq!(response.list()[0].subject().unwrap(), "On vacation");
        assert_eq!(
            response.list()[0].html_body().unwrap(),
            "<h1>On vacation</h1>"
        );
    }

    #[test]
    fn vacation_response_set_response_accessors() {
        let mut response = tagged(VACATION_RESPONSE_SET)
            .unwrap_set_vacation_response()
            .unwrap();

        assert_eq!(response.account_id(), Some("ue150411c"));
        assert_eq!(response.old_state(), Some("413b1c2a-0"));
        assert_eq!(response.new_state(), "413b1c2a-1");
        assert!(response.has_created());
        assert_eq!(response.created("c0").unwrap().id().unwrap(), "singleton");
        assert!(matches!(
            response.created("c9"),
            Err(crate::Error::Internal(_))
        ));
    }

    // ---------------------------------------------------------- sieve

    #[test]
    fn sieve_script_get_response_accessors() {
        let response = tagged(SIEVE_SCRIPT_GET).unwrap_get_sieve_script().unwrap();

        assert_eq!(response.account_id(), Some("ue150411c"));
        assert_eq!(response.state(), "1024");
        assert_eq!(response.list().len(), 1);
        assert_eq!(response.not_found(), ["S404"]);
        assert_eq!(response.list()[0].name.as_deref(), Some("test1"));
        assert_eq!(
            response.list()[0].blob_id.as_deref(),
            Some("0a4b-d3v3-4d35")
        );
        assert_eq!(response.list()[0].is_active, Some(true));
    }

    #[test]
    fn sieve_script_set_response_accessors() {
        let mut response = tagged(SIEVE_SCRIPT_SET).unwrap_set_sieve_script().unwrap();

        assert_eq!(response.account_id(), Some("ue150411c"));
        assert_eq!(response.old_state(), Some("1023"));
        assert_eq!(response.new_state(), "1024");
        assert!(response.has_created());
        assert_eq!(
            response.created("c0").unwrap().name.as_deref(),
            Some("test1")
        );
        assert!(response.unwrap_create_errors().is_ok());
    }

    #[test]
    fn sieve_script_query_response_accessors() {
        let response = tagged(SIEVE_SCRIPT_QUERY)
            .unwrap_query_sieve_script()
            .unwrap();

        assert_eq!(response.account_id(), "ue150411c");
        assert_eq!(response.query_state(), "sieve-0");
        assert!(response.can_calculate_changes());
        assert_eq!(response.position(), 0);
        assert_eq!(response.ids(), ["S1"]);
        assert_eq!(response.total(), Some(1));
        assert_eq!(response.limit(), None, "absent optional member");
    }

    #[test]
    fn sieve_validate_reports_an_invalid_script_inside_a_success_response() {
        let response = tagged(SIEVE_VALIDATE_ERROR)
            .unwrap_validate_sieve_script()
            .unwrap();

        assert_eq!(
            response.unwrap_error().unwrap_err().to_string(),
            "Set failed: invalidScript: Unexpected token"
        );
    }

    #[test]
    fn sieve_validate_reports_a_clean_script_as_ok() {
        let response = tagged(SIEVE_VALIDATE_OK)
            .unwrap_validate_sieve_script()
            .unwrap();

        assert!(response.unwrap_error().is_ok());
    }

    // ------------------------------------------------------ principals

    #[test]
    fn principal_get_response_accessors() {
        let response = tagged(PRINCIPAL_GET).unwrap_get_principal().unwrap();

        assert_eq!(response.account_id(), Some("P1"));
        assert_eq!(response.state(), "7b8eff5zz");
        assert_eq!(response.list().len(), 1);

        let principal = &response.list()[0];
        assert_eq!(principal.id(), Some("P2342fnddd20"));
        assert_eq!(principal.ptype(), Some(&crate::principal::Type::Individual));
        assert_eq!(principal.name(), Some("Joe Bloggs"));
        assert_eq!(principal.description(), None, "explicit null");
        assert_eq!(principal.email(), Some("joe.bloggs@example.com"));
        assert_eq!(principal.timezone(), Some("Australia/Melbourne"));
        assert_eq!(
            principal.capabilities(),
            Some(&["urn:ietf:params:jmap:mail".to_string()][..])
        );
        assert_eq!(
            principal.aliases(),
            Some(&["joe.bloggs@alternate.example.com".to_string()][..])
        );
        assert_eq!(principal.secret(), Some("hunter2"));
        assert_eq!(principal.quota(), Some(1_048_576));
        assert_eq!(principal.members(), Some(&[][..]));
        assert_eq!(
            principal.acl().unwrap().get("P674pp24095qo49pr").unwrap(),
            &[
                crate::principal::ACL::ReadItems,
                crate::principal::ACL::AddItems
            ]
        );
    }

    #[test]
    fn principal_changes_response_accessors() {
        let response = tagged(PRINCIPAL_CHANGES)
            .unwrap_changes_principal()
            .unwrap();

        assert_eq!(response.account_id(), "P1");
        assert_eq!(response.old_state(), "7b8eff5zy");
        assert_eq!(response.new_state(), "7b8eff5zz");
        assert!(!response.has_more_changes());
        assert_eq!(response.created(), ["P2342fnddd20"]);
        assert!(response.updated().is_empty());
        assert!(response.destroyed().is_empty());
        assert_eq!(response.total_changes(), 1);
    }

    #[test]
    fn principal_query_response_accessors() {
        let response = tagged(PRINCIPAL_QUERY).unwrap_query_principal().unwrap();

        assert_eq!(response.account_id(), "P1");
        assert_eq!(response.query_state(), "principal-0");
        assert!(response.can_calculate_changes());
        assert_eq!(response.position(), 0);
        assert_eq!(response.ids(), ["P2342fnddd20"]);
        assert_eq!(response.total(), Some(1));
    }

    #[test]
    fn principal_query_changes_response_accessors() {
        let response = tagged(PRINCIPAL_QUERY_CHANGES)
            .unwrap_query_changes_principal()
            .unwrap();

        assert_eq!(response.account_id(), "P1");
        assert_eq!(response.old_query_state(), "principal-1");
        assert_eq!(response.new_query_state(), "principal-2");
        assert_eq!(response.total(), Some(1));
        assert!(response.removed().is_empty());
        assert_eq!(response.added()[0].id(), "P2342fnddd20");
        assert_eq!(response.added()[0].index(), 0);
    }

    #[test]
    fn principal_set_response_accessors() {
        let mut response = tagged(PRINCIPAL_SET).unwrap_set_principal().unwrap();

        assert_eq!(response.account_id(), Some("P1"));
        assert_eq!(response.new_state(), "4243");
        assert_eq!(
            response.created("c0").unwrap().id().unwrap(),
            "P2342fnddd20"
        );
        assert!(response.unwrap_update_errors().is_err());
        assert_eq!(
            response.updated("P404").unwrap_err().to_string(),
            "Set failed: notFound: No such principal"
        );
    }

    // ----------------------------------------------------------- echo

    #[test]
    fn echo_response_carries_its_arguments_verbatim() {
        let value = tagged(r#"["Core/echo", {"seq": 1, "note": "hi"}, "c0"]"#)
            .unwrap_echo()
            .unwrap();

        assert_eq!(value, json!({ "seq": 1, "note": "hi" }));
        assert_eq!(value["seq"], 1);

        // the mismatch arm for the echo accessor itself
        assert_eq!(
            tagged(MAILBOX_GET).unwrap_echo().unwrap_err().to_string(),
            "Internal error: Response type mismatch"
        );

        // ...and its error arm, which the shared macro cannot reach
        // because `wrong_type()` is itself an echo.
        assert!(matches!(
            method_error().unwrap_echo(),
            Err(crate::Error::Method(_))
        ));
    }

    /// `TaggedMethodResponse` is deserialised by a hand-written seq
    /// visitor; its `expecting()` message surfaces whenever the payload is
    /// not a JSON array at all.
    #[test]
    fn a_non_array_payload_reports_what_the_visitor_expected() {
        let error = serde_json::from_str::<TaggedMethodResponse>(r#"{"not": "a seq"}"#)
            .expect_err("a JMAP method response must be an array");
        assert_eq!(
            error.to_string(),
            "invalid type: map, expected a valid JMAP method response at line 1 column 0"
        );

        // an array with no method name is rejected too
        let error = serde_json::from_str::<TaggedMethodResponse>("[]")
            .expect_err("the method name is mandatory");
        assert_eq!(
            error.to_string(),
            "Expected a method name at line 1 column 2"
        );
    }

    // ------------------------------------------------------- is_type()

    #[test]
    fn is_type_reports_the_response_variant() {
        let cases: &[(&str, Method)] = &[
            (COPY_BLOB, Method::CopyBlob),
            (PUSH_SUBSCRIPTION_GET, Method::GetPushSubscription),
            (PUSH_SUBSCRIPTION_SET, Method::SetPushSubscription),
            (MAILBOX_GET, Method::GetMailbox),
            (MAILBOX_CHANGES, Method::ChangesMailbox),
            (MAILBOX_QUERY, Method::QueryMailbox),
            (MAILBOX_QUERY_CHANGES, Method::QueryChangesMailbox),
            (MAILBOX_SET, Method::SetMailbox),
            (THREAD_GET, Method::GetThread),
            (THREAD_CHANGES, Method::ChangesThread),
            (EMAIL_GET, Method::GetEmail),
            (EMAIL_CHANGES, Method::ChangesEmail),
            (EMAIL_QUERY, Method::QueryEmail),
            (EMAIL_QUERY_CHANGES, Method::QueryChangesEmail),
            (EMAIL_SET, Method::SetEmail),
            (EMAIL_COPY, Method::CopyEmail),
            (EMAIL_IMPORT, Method::ImportEmail),
            (EMAIL_PARSE, Method::ParseEmail),
            (SEARCH_SNIPPET_GET, Method::GetSearchSnippet),
            (IDENTITY_GET, Method::GetIdentity),
            (IDENTITY_CHANGES, Method::ChangesIdentity),
            (IDENTITY_SET, Method::SetIdentity),
            (EMAIL_SUBMISSION_GET, Method::GetEmailSubmission),
            (EMAIL_SUBMISSION_CHANGES, Method::ChangesEmailSubmission),
            (EMAIL_SUBMISSION_QUERY, Method::QueryEmailSubmission),
            (
                EMAIL_SUBMISSION_QUERY_CHANGES,
                Method::QueryChangesEmailSubmission,
            ),
            (EMAIL_SUBMISSION_SET, Method::SetEmailSubmission),
            (VACATION_RESPONSE_GET, Method::GetVacationResponse),
            (VACATION_RESPONSE_SET, Method::SetVacationResponse),
            (SIEVE_SCRIPT_GET, Method::GetSieveScript),
            (SIEVE_SCRIPT_SET, Method::SetSieveScript),
            (SIEVE_SCRIPT_QUERY, Method::QuerySieveScript),
            (SIEVE_VALIDATE_OK, Method::ValidateSieveScript),
            (PRINCIPAL_GET, Method::GetPrincipal),
            (PRINCIPAL_CHANGES, Method::ChangesPrincipal),
            (PRINCIPAL_QUERY, Method::QueryPrincipal),
            (PRINCIPAL_QUERY_CHANGES, Method::QueryChangesPrincipal),
            (PRINCIPAL_SET, Method::SetPrincipal),
        ];

        for (fixture, method) in cases {
            let response = tagged(fixture);
            assert!(
                response.is_type(*method),
                "the fixture must deserialise as {:?}",
                method
            );
            // and as nothing else in the enum
            for other in [
                Method::Echo,
                Method::Error,
                Method::GetMailbox,
                Method::SetPrincipal,
            ] {
                if std::mem::discriminant(&other) != std::mem::discriminant(method) {
                    assert!(
                        !response.is_type(other),
                        "a {:?} response must not report itself as {:?}",
                        method,
                        other
                    );
                }
            }
        }

        let error = method_error();
        assert!(error.is_type(Method::Error));
        assert!(!error.is_type(Method::GetMailbox));
        assert!(error.is_error());
        assert!(!tagged(MAILBOX_GET).is_error());
    }

    // --------------------------------- the error/mismatch arm of unwrap_*

    #[test]
    fn every_unwrap_accessor_surfaces_method_errors_and_rejects_mismatches() {
        assert_error_and_mismatch!(
            unwrap_copy_blob,
            unwrap_get_push_subscription,
            unwrap_set_push_subscription,
            unwrap_get_mailbox,
            unwrap_changes_mailbox,
            unwrap_query_mailbox,
            unwrap_query_changes_mailbox,
            unwrap_set_mailbox,
            unwrap_get_thread,
            unwrap_changes_thread,
            unwrap_get_email,
            unwrap_changes_email,
            unwrap_query_email,
            unwrap_query_changes_email,
            unwrap_set_email,
            unwrap_copy_email,
            unwrap_import_email,
            unwrap_parse_email,
            unwrap_get_search_snippet,
            unwrap_get_identity,
            unwrap_changes_identity,
            unwrap_set_identity,
            unwrap_get_email_submission,
            unwrap_changes_email_submission,
            unwrap_set_email_submission,
            unwrap_query_email_submission,
            unwrap_query_changes_email_submission,
            unwrap_get_vacation_response,
            unwrap_set_vacation_response,
            unwrap_get_sieve_script,
            unwrap_validate_sieve_script,
            unwrap_set_sieve_script,
            unwrap_query_sieve_script,
            unwrap_get_principal,
            unwrap_changes_principal,
            unwrap_query_principal,
            unwrap_query_changes_principal,
            unwrap_set_principal,
        );
    }

    // --------------------------------------------------- malformed envelopes

    /// Every method arm of the [`TaggedMethodResponse`] visitor must reject an
    /// envelope that names a method but omits the response object.
    ///
    /// RFC 8620 §3.4 fixes a method response as `["Name", {args}, "callId"]`,
    /// so a bare `["Name"]` is malformed for *every* method, not just some. The
    /// `match` in `visit_seq` is exhaustive over [`Method`] with no catch-all,
    /// so walking all variants here touches each arm's missing-payload guard
    /// exactly once.
    #[test]
    fn a_response_envelope_must_carry_a_response_object() {
        let methods = [
            Method::Echo,
            Method::CopyBlob,
            Method::GetPushSubscription,
            Method::SetPushSubscription,
            Method::GetMailbox,
            Method::ChangesMailbox,
            Method::QueryMailbox,
            Method::QueryChangesMailbox,
            Method::SetMailbox,
            Method::GetThread,
            Method::ChangesThread,
            Method::GetEmail,
            Method::ChangesEmail,
            Method::QueryEmail,
            Method::QueryChangesEmail,
            Method::SetEmail,
            Method::CopyEmail,
            Method::ImportEmail,
            Method::ParseEmail,
            Method::GetSearchSnippet,
            Method::GetIdentity,
            Method::ChangesIdentity,
            Method::SetIdentity,
            Method::GetEmailSubmission,
            Method::ChangesEmailSubmission,
            Method::QueryEmailSubmission,
            Method::QueryChangesEmailSubmission,
            Method::SetEmailSubmission,
            Method::GetVacationResponse,
            Method::SetVacationResponse,
            Method::GetSieveScript,
            Method::SetSieveScript,
            Method::QuerySieveScript,
            Method::ValidateSieveScript,
            Method::GetPrincipal,
            Method::ChangesPrincipal,
            Method::QueryPrincipal,
            Method::QueryChangesPrincipal,
            Method::SetPrincipal,
            Method::Error,
        ];
        // Tripwire: a new `Method` variant breaks the exhaustive `match` at
        // compile time, but only this list keeps the *runtime* guard covered.
        assert_eq!(methods.len(), 40, "add the new Method variant to this list");

        for method in methods {
            let name = serde_json::to_string(&method).unwrap();
            let json = format!("[{name}]");
            let err = serde_json::from_str::<TaggedMethodResponse>(&json).expect_err(&format!(
                "{name} without a response object must be rejected"
            ));
            assert!(
                err.to_string()
                    .contains("Expected a method response at line 1 column"),
                "{name} must report the missing response object, got {err}"
            );
        }
    }

    /// The call id is mandatory even when the response object is present
    /// (RFC 8620 §3.4), so a two-element array must not deserialise.
    #[test]
    fn a_response_envelope_must_carry_a_call_id() {
        let err = serde_json::from_str::<TaggedMethodResponse>(r#"["Core/echo", {"seq": 1}]"#)
            .expect_err("a response without a call id must be rejected");
        assert!(
            err.to_string()
                .contains("Expected method call id at line 1 column"),
            "a missing call id must be reported, got {err}"
        );
    }
}
