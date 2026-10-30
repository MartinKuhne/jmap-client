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

use crate::{
    blob::copy::CopyBlobRequest,
    client::Client,
    email::{
        import::EmailImportRequest, parse::EmailParseRequest,
        search_snippet::SearchSnippetGetRequest, Email,
    },
    email_submission::EmailSubmission,
    identity::Identity,
    mailbox::Mailbox,
    principal::Principal,
    push_subscription::PushSubscription,
    sieve::{validate::SieveScriptValidateRequest, SieveScript},
    thread::Thread,
    vacation_response::VacationResponse,
    Error, Method, Set, URI,
};
use ahash::AHashMap;
use serde::{de::DeserializeOwned, Serialize};

use super::{
    changes::ChangesRequest,
    copy::CopyRequest,
    get::GetRequest,
    query::QueryRequest,
    query_changes::QueryChangesRequest,
    response::{Response, SingleMethodResponse, TaggedMethodResponse},
    set::SetRequest,
    RequestParams,
};

#[derive(Serialize)]
pub struct Request<'x> {
    #[serde(skip)]
    client: &'x Client,
    #[serde(skip)]
    account_id: String,

    pub using: Vec<URI>,

    #[serde(rename = "methodCalls")]
    pub method_calls: Vec<(Method, Arguments, String)>,

    #[serde(rename = "createdIds")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_ids: Option<AHashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultReference {
    #[serde(rename = "resultOf")]
    result_of: String,
    name: Method,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Arguments {
    Changes(ChangesRequest),
    PushGet(GetRequest<PushSubscription<Set>>),
    PushSet(SetRequest<PushSubscription<Set>>),
    BlobCopy(CopyBlobRequest),
    MailboxGet(GetRequest<Mailbox<Set>>),
    MailboxQuery(QueryRequest<Mailbox<Set>>),
    MailboxQueryChanges(QueryChangesRequest<Mailbox<Set>>),
    MailboxSet(SetRequest<Mailbox<Set>>),
    ThreadGet(GetRequest<Thread>),
    EmailGet(GetRequest<Email<Set>>),
    EmailQuery(QueryRequest<Email<Set>>),
    EmailQueryChanges(QueryChangesRequest<Email<Set>>),
    EmailSet(SetRequest<Email<Set>>),
    EmailCopy(CopyRequest<Email<Set>>),
    EmailImport(EmailImportRequest),
    EmailParse(EmailParseRequest),
    SearchSnippetGet(SearchSnippetGetRequest),
    IdentityGet(GetRequest<Identity<Set>>),
    IdentitySet(SetRequest<Identity<Set>>),
    EmailSubmissionGet(GetRequest<EmailSubmission<Set>>),
    EmailSubmissionQuery(QueryRequest<EmailSubmission<Set>>),
    EmailSubmissionQueryChanges(QueryChangesRequest<EmailSubmission<Set>>),
    EmailSubmissionSet(SetRequest<EmailSubmission<Set>>),
    VacationResponseGet(GetRequest<VacationResponse<Set>>),
    VacationResponseSet(SetRequest<VacationResponse<Set>>),
    SieveScriptGet(GetRequest<SieveScript<Set>>),
    SieveScriptQuery(QueryRequest<SieveScript<Set>>),
    SieveScriptValidate(SieveScriptValidateRequest),
    SieveScriptSet(SetRequest<SieveScript<Set>>),
    PrincipalGet(GetRequest<Principal<Set>>),
    PrincipalQuery(QueryRequest<Principal<Set>>),
    PrincipalQueryChanges(QueryChangesRequest<Principal<Set>>),
    PrincipalSet(SetRequest<Principal<Set>>),
}

impl Arguments {
    pub fn changes(params: RequestParams, since_state: String) -> Self {
        Arguments::Changes(ChangesRequest::new(params, since_state))
    }

    pub fn push_get(params: RequestParams) -> Self {
        Arguments::PushGet(GetRequest::new(params))
    }

    pub fn push_set(params: RequestParams) -> Self {
        Arguments::PushSet(SetRequest::new(params))
    }

    pub fn blob_copy(params: RequestParams, from_account_id: String) -> Self {
        Arguments::BlobCopy(CopyBlobRequest::new(params, from_account_id))
    }

    pub fn mailbox_get(params: RequestParams) -> Self {
        Arguments::MailboxGet(GetRequest::new(params))
    }

    pub fn mailbox_query(params: RequestParams) -> Self {
        Arguments::MailboxQuery(QueryRequest::new(params))
    }

    pub fn mailbox_query_changes(params: RequestParams, since_query_state: String) -> Self {
        Arguments::MailboxQueryChanges(QueryChangesRequest::new(params, since_query_state))
    }

    pub fn mailbox_set(params: RequestParams) -> Self {
        Arguments::MailboxSet(SetRequest::new(params))
    }

    pub fn thread_get(params: RequestParams) -> Self {
        Arguments::ThreadGet(GetRequest::new(params))
    }

    pub fn email_get(params: RequestParams) -> Self {
        Arguments::EmailGet(GetRequest::new(params))
    }

    pub fn email_query(params: RequestParams) -> Self {
        Arguments::EmailQuery(QueryRequest::new(params))
    }

    pub fn email_query_changes(params: RequestParams, since_query_state: String) -> Self {
        Arguments::EmailQueryChanges(QueryChangesRequest::new(params, since_query_state))
    }

    pub fn email_set(params: RequestParams) -> Self {
        Arguments::EmailSet(SetRequest::new(params))
    }

    pub fn email_copy(params: RequestParams, from_account_id: String) -> Self {
        Arguments::EmailCopy(CopyRequest::new(params, from_account_id))
    }

    pub fn email_import(params: RequestParams) -> Self {
        Arguments::EmailImport(EmailImportRequest::new(params))
    }

    pub fn email_parse(params: RequestParams) -> Self {
        Arguments::EmailParse(EmailParseRequest::new(params))
    }

    pub fn search_snippet_get(params: RequestParams) -> Self {
        Arguments::SearchSnippetGet(SearchSnippetGetRequest::new(params))
    }

    pub fn identity_get(params: RequestParams) -> Self {
        Arguments::IdentityGet(GetRequest::new(params))
    }

    pub fn identity_set(params: RequestParams) -> Self {
        Arguments::IdentitySet(SetRequest::new(params))
    }

    pub fn email_submission_get(params: RequestParams) -> Self {
        Arguments::EmailSubmissionGet(GetRequest::new(params))
    }

    pub fn email_submission_query(params: RequestParams) -> Self {
        Arguments::EmailSubmissionQuery(QueryRequest::new(params))
    }

    pub fn email_submission_query_changes(
        params: RequestParams,
        since_query_state: String,
    ) -> Self {
        Arguments::EmailSubmissionQueryChanges(QueryChangesRequest::new(params, since_query_state))
    }

    pub fn email_submission_set(params: RequestParams) -> Self {
        Arguments::EmailSubmissionSet(SetRequest::new(params))
    }

    pub fn vacation_response_get(params: RequestParams) -> Self {
        Arguments::VacationResponseGet(GetRequest::new(params))
    }

    pub fn vacation_response_set(params: RequestParams) -> Self {
        Arguments::VacationResponseSet(SetRequest::new(params))
    }

    pub fn sieve_script_get(params: RequestParams) -> Self {
        Arguments::SieveScriptGet(GetRequest::new(params))
    }

    pub fn sieve_script_query(params: RequestParams) -> Self {
        Arguments::SieveScriptQuery(QueryRequest::new(params))
    }

    pub fn sieve_script_validate(params: RequestParams, blob_id: impl Into<String>) -> Self {
        Arguments::SieveScriptValidate(SieveScriptValidateRequest::new(params, blob_id))
    }

    pub fn sieve_script_set(params: RequestParams) -> Self {
        Arguments::SieveScriptSet(SetRequest::new(params))
    }

    pub fn principal_get(params: RequestParams) -> Self {
        Arguments::PrincipalGet(GetRequest::new(params))
    }

    pub fn principal_query(params: RequestParams) -> Self {
        Arguments::PrincipalQuery(QueryRequest::new(params))
    }

    pub fn principal_query_changes(params: RequestParams, since_query_state: String) -> Self {
        Arguments::PrincipalQueryChanges(QueryChangesRequest::new(params, since_query_state))
    }

    pub fn principal_set(params: RequestParams) -> Self {
        Arguments::PrincipalSet(SetRequest::new(params))
    }

    pub fn changes_mut(&mut self) -> &mut ChangesRequest {
        match self {
            Arguments::Changes(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn push_get_mut(&mut self) -> &mut GetRequest<PushSubscription<Set>> {
        match self {
            Arguments::PushGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn push_set_mut(&mut self) -> &mut SetRequest<PushSubscription<Set>> {
        match self {
            Arguments::PushSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn blob_copy_mut(&mut self) -> &mut CopyBlobRequest {
        match self {
            Arguments::BlobCopy(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn mailbox_get_mut(&mut self) -> &mut GetRequest<Mailbox<Set>> {
        match self {
            Arguments::MailboxGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn mailbox_query_mut(&mut self) -> &mut QueryRequest<Mailbox<Set>> {
        match self {
            Arguments::MailboxQuery(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn mailbox_query_changes_mut(&mut self) -> &mut QueryChangesRequest<Mailbox<Set>> {
        match self {
            Arguments::MailboxQueryChanges(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn mailbox_set_mut(&mut self) -> &mut SetRequest<Mailbox<Set>> {
        match self {
            Arguments::MailboxSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn thread_get_mut(&mut self) -> &mut GetRequest<Thread> {
        match self {
            Arguments::ThreadGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_get_mut(&mut self) -> &mut GetRequest<Email<Set>> {
        match self {
            Arguments::EmailGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_query_mut(&mut self) -> &mut QueryRequest<Email<Set>> {
        match self {
            Arguments::EmailQuery(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_query_changes_mut(&mut self) -> &mut QueryChangesRequest<Email<Set>> {
        match self {
            Arguments::EmailQueryChanges(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_set_mut(&mut self) -> &mut SetRequest<Email<Set>> {
        match self {
            Arguments::EmailSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_copy_mut(&mut self) -> &mut CopyRequest<Email<Set>> {
        match self {
            Arguments::EmailCopy(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_import_mut(&mut self) -> &mut EmailImportRequest {
        match self {
            Arguments::EmailImport(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_parse_mut(&mut self) -> &mut EmailParseRequest {
        match self {
            Arguments::EmailParse(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn search_snippet_get_mut(&mut self) -> &mut SearchSnippetGetRequest {
        match self {
            Arguments::SearchSnippetGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn identity_get_mut(&mut self) -> &mut GetRequest<Identity<Set>> {
        match self {
            Arguments::IdentityGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn identity_set_mut(&mut self) -> &mut SetRequest<Identity<Set>> {
        match self {
            Arguments::IdentitySet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_submission_get_mut(&mut self) -> &mut GetRequest<EmailSubmission<Set>> {
        match self {
            Arguments::EmailSubmissionGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_submission_query_mut(&mut self) -> &mut QueryRequest<EmailSubmission<Set>> {
        match self {
            Arguments::EmailSubmissionQuery(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_submission_query_changes_mut(
        &mut self,
    ) -> &mut QueryChangesRequest<EmailSubmission<Set>> {
        match self {
            Arguments::EmailSubmissionQueryChanges(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn email_submission_set_mut(&mut self) -> &mut SetRequest<EmailSubmission<Set>> {
        match self {
            Arguments::EmailSubmissionSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn vacation_response_get_mut(&mut self) -> &mut GetRequest<VacationResponse<Set>> {
        match self {
            Arguments::VacationResponseGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn vacation_response_set_mut(&mut self) -> &mut SetRequest<VacationResponse<Set>> {
        match self {
            Arguments::VacationResponseSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn sieve_script_get_mut(&mut self) -> &mut GetRequest<SieveScript<Set>> {
        match self {
            Arguments::SieveScriptGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn sieve_script_query_mut(&mut self) -> &mut QueryRequest<SieveScript<Set>> {
        match self {
            Arguments::SieveScriptQuery(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn sieve_script_validate_mut(&mut self) -> &mut SieveScriptValidateRequest {
        match self {
            Arguments::SieveScriptValidate(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn sieve_script_set_mut(&mut self) -> &mut SetRequest<SieveScript<Set>> {
        match self {
            Arguments::SieveScriptSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn principal_get_mut(&mut self) -> &mut GetRequest<Principal<Set>> {
        match self {
            Arguments::PrincipalGet(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn principal_query_mut(&mut self) -> &mut QueryRequest<Principal<Set>> {
        match self {
            Arguments::PrincipalQuery(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn principal_query_changes_mut(&mut self) -> &mut QueryChangesRequest<Principal<Set>> {
        match self {
            Arguments::PrincipalQueryChanges(ref mut r) => r,
            _ => unreachable!(),
        }
    }

    pub fn principal_set_mut(&mut self) -> &mut SetRequest<Principal<Set>> {
        match self {
            Arguments::PrincipalSet(ref mut r) => r,
            _ => unreachable!(),
        }
    }
}

/// The JMAP capabilities this crate may announce, in canonical order.
///
/// This is only the candidate list: [`Request::new`] keeps the entries the server
/// actually advertised. The set is the same one this crate has always announced,
/// which is why [`URI::PrincipalsOwner`] is not a candidate.
const KNOWN_CAPABILITIES: [URI; 11] = [
    URI::Core,
    URI::Mail,
    URI::Submission,
    URI::VacationResponse,
    URI::Contacts,
    URI::Calendars,
    URI::WebSocket,
    URI::Sieve,
    URI::Blob,
    URI::Quota,
    URI::Principals,
];

impl<'x> Request<'x> {
    /// Builds a request that announces only the capabilities the server advertised.
    ///
    /// Strict servers such as Fastmail and Apache James reject a request whose
    /// `using` array contains a capability they did not advertise with an
    /// `unknownCapability` error, so [`KNOWN_CAPABILITIES`] is filtered against
    /// the session. The canonical order is preserved and no entry can repeat,
    /// because the candidate list holds each capability exactly once.
    ///
    /// [`URI::Core`] is kept even when the session does not advertise it: RFC 8620
    /// section 3.6.1 requires clients to always include the core capability.
    pub fn new(client: &'x Client) -> Self {
        let session = client.session();
        let using = KNOWN_CAPABILITIES
            .iter()
            .filter(|uri| **uri == URI::Core || session.has_capability(uri.as_ref()))
            .cloned()
            .collect();

        Request {
            using,
            method_calls: vec![],
            created_ids: None,
            account_id: client.default_account_id().to_string(),
            client,
        }
    }

    pub fn account_id(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = account_id.into();
        self
    }

    #[maybe_async::maybe_async]
    pub async fn send(self) -> crate::Result<Response<TaggedMethodResponse>> {
        self.client.send(&self).await
    }

    #[cfg(feature = "websockets")]
    pub async fn send_ws(self) -> crate::Result<String> {
        self.client.send_ws(self).await
    }

    /// Sends the request and returns the body of the *primary* method response,
    /// or turns a method-level error into [`Error::Method`].
    ///
    /// The primary response is the **first** one that carries this request's
    /// call id, not the last one in the array. RFC 8620 §3.2 lets a single call
    /// answer with several responses under the *same* call id — "a method may
    /// return 1 or more responses, as it may make implicit calls to other
    /// methods; all responses initiated by this method call get the same method
    /// call id in the response" — and RFC 8620 §3.4 keeps them in processing
    /// order, so the primary answer comes first and the implicit one after it.
    /// RFC 8621 §7.5.1 shows exactly this for `EmailSubmission/set`: "Note that
    /// there are two responses due to the implicit "Email/set" call, but both
    /// have the same method call id as they are due to the same call in the
    /// request". RFC 8620 §5.4 (`/copy` with `onSuccessDestroyOriginal`, which
    /// RFC 8621 §4.7 adopts for `Email/copy`) has the same shape. Reading the
    /// tail instead hands the caller the implicit body and fails an operation
    /// that succeeded.
    ///
    /// Only the selected response's arguments are deserialised into `T`; the
    /// others are read as opaque JSON. An implicit secondary response is a
    /// *different* method with a *different* body, so parsing every one of them
    /// as `T` would fail the envelope before the selection could happen.
    #[maybe_async::maybe_async]
    pub async fn send_single<T>(self) -> crate::Result<T>
    where
        T: DeserializeOwned,
    {
        // The call id this request sent. Every helper builds a single method
        // call (`s0`); when a caller does add several, the last one is the
        // result they are asking for.
        let call_id = self.method_calls.last().map(|call| call.2.clone());

        let response: Response<SingleMethodResponse<serde_json::Value>> =
            self.client.send(&self).await?;
        let mut responses = response.unwrap_method_responses();

        let primary = responses
            .iter()
            .position(|response| Some(response.call_id()) == call_id.as_deref())
            // A server that did not echo the call id still answered, so keep
            // the first body rather than throwing a well-formed one away.
            .or_else(|| (!responses.is_empty()).then_some(0))
            .ok_or_else(|| Error::Internal("Server returned no results".to_string()))?;

        match responses.remove(primary) {
            SingleMethodResponse::Ok((_, body, _)) => {
                // The selected body is re-read from its JSON text rather than
                // converted with `from_value`: a few of the crate's
                // `Deserialize` impls — `Role` in particular (Finding #90) —
                // read borrowed text, which a `Value` cannot hand out.
                Ok(serde_json::from_str(&body.to_string())?)
            }
            SingleMethodResponse::Error((_, err, _)) => Err(err.into()),
        }
    }

    pub fn params(&self, method: Method) -> RequestParams {
        RequestParams {
            account_id: self.account_id.clone(),
            method,
            call_id: self.method_calls.len(),
        }
    }

    pub fn add_method_call(&mut self, method: Method, arguments: Arguments) -> &mut Arguments {
        let call_id = format!("s{}", self.method_calls.len());
        self.method_calls.push((method, arguments, call_id));
        &mut self.method_calls.last_mut().unwrap().1
    }

    pub fn add_capability(&mut self, uri: URI) {
        if !self.using.contains(&uri) {
            self.using.push(uri);
        }
    }

    pub fn last_result_reference(&self, path: impl Into<String>) -> ResultReference {
        let last_method = self.method_calls.last().unwrap();
        ResultReference {
            result_of: last_method.2.clone(),
            name: last_method.0,
            path: path.into(),
        }
    }
}

impl ResultReference {
    pub fn new(method: Method, call_id: usize, path: impl Into<String>) -> Self {
        ResultReference {
            result_of: format!("s{}", call_id),
            name: method,
            path: path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{client::Client, core::request::Request, core::session::Session, URI};
    use serde_json::{json, Value};

    /// Builds the `capabilities` map value advertised for a given capability id.
    ///
    /// Capabilities the crate models explicitly get a realistic payload, everything
    /// else (including capabilities unknown to this crate) gets an empty object.
    fn capability_value(capability: &str) -> Value {
        match capability {
            "urn:ietf:params:jmap:core" => json!({
                "maxSizeUpload": 52_428_800,
                "maxConcurrentUpload": 2,
                "maxSizeRequest": 10_485_760,
                "maxConcurrentRequests": 2,
                "maxCallsInRequest": 16,
                "maxObjectsInGet": 512,
                "maxObjectsInSet": 512,
                "collationAlgorithms": [
                    "i;ascii-numeric",
                    "i;ascii-casemap",
                    "i;unicode-casemap"
                ]
            }),
            "urn:ietf:params:jmap:mail" => json!({
                "maxMailboxesPerEmail": null,
                "maxMailboxDepth": 10,
                "maxSizeMailboxName": 46,
                "maxSizeAttachmentsPerEmail": 26_214_400,
                "emailQuerySortOptions": ["receivedAt", "subject", "size"],
                "mayCreateTopLevelMailbox": true
            }),
            "urn:ietf:params:jmap:submission" => json!({
                "maxDelayedSend": 0,
                "submissionExtensions": {}
            }),
            "urn:ietf:params:jmap:websocket" => json!({
                "url": "wss://example.com/jmap/ws",
                "supportsPush": true
            }),
            _ => json!({}),
        }
    }

    /// Builds a session, as advertised by `GET /.well-known/jmap`, that advertises
    /// exactly the given capabilities.
    fn session_advertising(capabilities: &[&str]) -> Session {
        let mut advertised = serde_json::Map::new();
        for capability in capabilities {
            advertised.insert((*capability).to_string(), capability_value(capability));
        }

        let payload = json!({
            "capabilities": advertised,
            "accounts": {
                "account1": {
                    "name": "user@example.com",
                    "isPersonal": true,
                    "isReadOnly": false,
                    "accountCapabilities": {}
                }
            },
            "primaryAccounts": {
                "urn:ietf:params:jmap:mail": "account1"
            },
            "username": "user@example.com",
            "apiUrl": "https://example.com/jmap/session",
            "downloadUrl":
                "https://example.com/jmap/download/{accountId}/{blobId}/{name}?accept={type}",
            "uploadUrl": "https://example.com/jmap/upload/{accountId}",
            "eventSourceUrl":
                "https://example.com/jmap/eventSource?types={types}&closeafter={closeafter}&ping={ping}",
            "state": "1234567-1"
        });

        let session = serde_json::from_str::<Session>(&payload.to_string())
            .expect("test session payload must deserialize");

        // Guard the fixture itself: a typo in a capability id above would otherwise
        // silently change what the tests below are asserting.
        assert_eq!(
            session.capabilities().count(),
            capabilities.len(),
            "every requested capability must end up in the session"
        );
        for capability in capabilities {
            assert!(
                session.has_capability(*capability),
                "session fixture does not advertise {:?}",
                capability
            );
        }

        session
    }

    /// Builds a client, without any network access, whose session advertises
    /// exactly the given capabilities.
    fn client_advertising(capabilities: &[&str]) -> Client {
        Client::from_session_for_tests(session_advertising(capabilities))
    }

    #[test]
    fn fastmail_like_session_only_uses_advertised_capabilities() {
        let client = client_advertising(&[
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:mdn",
            "urn:ietf:params:jmap:changes",
            "urn:ietf:params:jmap:serverMerge",
            "urn:ietf:params:jmap:oldEmailIds",
        ]);
        let request = Request::new(&client);

        assert_eq!(
            request.using,
            vec![URI::Core, URI::Mail, URI::Submission, URI::VacationResponse,],
            "using must contain only the capabilities advertised by the server"
        );
    }

    #[test]
    fn james_like_session_only_uses_advertised_capabilities_in_canonical_order() {
        let client = client_advertising(&[
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:mdn",
            "urn:ietf:params:jmap:quota",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:websocket",
        ]);
        let request = Request::new(&client);

        assert_eq!(
            request.using,
            vec![
                URI::Core,
                URI::Mail,
                URI::Submission,
                URI::VacationResponse,
                URI::WebSocket,
                URI::Quota,
            ],
            "using must follow the canonical order, not the order advertised"
        );
    }

    #[test]
    fn full_session_keeps_all_known_capabilities() {
        let client = client_advertising(&[
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:contacts",
            "urn:ietf:params:jmap:calendars",
            "urn:ietf:params:jmap:websocket",
            "urn:ietf:params:jmap:sieve",
            "urn:ietf:params:jmap:blob",
            "urn:ietf:params:jmap:quota",
            "urn:ietf:params:jmap:principals",
        ]);
        let request = Request::new(&client);

        assert_eq!(
            request.using,
            vec![
                URI::Core,
                URI::Mail,
                URI::Submission,
                URI::VacationResponse,
                URI::Contacts,
                URI::Calendars,
                URI::WebSocket,
                URI::Sieve,
                URI::Blob,
                URI::Quota,
                URI::Principals,
            ],
            "a server advertising everything must keep the previous behaviour"
        );
    }

    #[test]
    fn core_capability_is_always_included() {
        let client = client_advertising(&["urn:ietf:params:jmap:mail"]);
        let request = Request::new(&client);

        assert_eq!(
            request.using,
            vec![URI::Core, URI::Mail],
            "RFC 8620 3.6.1: clients must always include the core capability"
        );
    }

    #[test]
    fn using_serializes_as_advertised_capability_uris() {
        let client = client_advertising(&[
            "urn:ietf:params:jmap:core",
            "urn:ietf:params:jmap:mail",
            "urn:ietf:params:jmap:submission",
            "urn:ietf:params:jmap:vacationresponse",
            "urn:ietf:params:jmap:mdn",
            "urn:ietf:params:jmap:changes",
            "urn:ietf:params:jmap:serverMerge",
            "urn:ietf:params:jmap:oldEmailIds",
        ]);
        let request = Request::new(&client);

        let serialized = serde_json::to_value(&request).expect("request must serialize");

        assert_eq!(
            serialized["using"],
            json!([
                "urn:ietf:params:jmap:core",
                "urn:ietf:params:jmap:mail",
                "urn:ietf:params:jmap:submission",
                "urn:ietf:params:jmap:vacationresponse"
            ]),
            "the JSON `using` array must contain exactly the advertised capabilities"
        );
    }
}
