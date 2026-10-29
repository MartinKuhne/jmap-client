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

use std::fmt::Display;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    p_type: ProblemType,
    pub status: Option<u32>,
    title: Option<String>,
    detail: Option<String>,
    limit: Option<String>,
    request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub enum JMAPError {
    #[serde(rename = "urn:ietf:params:jmap:error:unknownCapability")]
    UnknownCapability,
    #[serde(rename = "urn:ietf:params:jmap:error:notJSON")]
    NotJSON,
    #[serde(rename = "urn:ietf:params:jmap:error:notRequest")]
    NotRequest,
    #[serde(rename = "urn:ietf:params:jmap:error:limit")]
    Limit,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ProblemType {
    JMAP(JMAPError),
    Other(String),
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct MethodError {
    #[serde(rename = "type")]
    pub p_type: MethodErrorType,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum MethodErrorType {
    #[serde(rename = "serverUnavailable")]
    ServerUnavailable,
    #[serde(rename = "serverFail")]
    ServerFail,
    #[serde(rename = "serverPartialFail")]
    ServerPartialFail,
    #[serde(rename = "unknownMethod")]
    UnknownMethod,
    #[serde(rename = "invalidArguments")]
    InvalidArguments,
    #[serde(rename = "invalidResultReference")]
    InvalidResultReference,
    #[serde(rename = "forbidden")]
    Forbidden,
    #[serde(rename = "accountNotFound")]
    AccountNotFound,
    #[serde(rename = "accountNotSupportedByMethod")]
    AccountNotSupportedByMethod,
    #[serde(rename = "accountReadOnly")]
    AccountReadOnly,
    #[serde(rename = "requestTooLarge")]
    RequestTooLarge,
    #[serde(rename = "cannotCalculateChanges")]
    CannotCalculateChanges,
    #[serde(rename = "stateMismatch")]
    StateMismatch,
    #[serde(rename = "alreadyExists")]
    AlreadyExists,
    #[serde(rename = "fromAccountNotFound")]
    FromAccountNotFound,
    #[serde(rename = "fromAccountNotSupportedByMethod")]
    FromAccountNotSupportedByMethod,
    #[serde(rename = "anchorNotFound")]
    AnchorNotFound,
    #[serde(rename = "unsupportedSort")]
    UnsupportedSort,
    #[serde(rename = "unsupportedFilter")]
    UnsupportedFilter,
    #[serde(rename = "tooManyChanges")]
    TooManyChanges,
}

impl ProblemDetails {
    pub fn new(
        p_type: ProblemType,
        status: Option<u32>,
        title: Option<String>,
        detail: Option<String>,
        limit: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        ProblemDetails {
            p_type,
            status,
            title,
            detail,
            limit,
            request_id,
        }
    }

    pub fn error(&self) -> &ProblemType {
        &self.p_type
    }

    pub fn status(&self) -> Option<u32> {
        self.status
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub fn limit(&self) -> Option<&str> {
        self.limit.as_deref()
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
}

impl MethodError {
    pub fn error(&self) -> &MethodErrorType {
        &self.p_type
    }
}

impl Display for MethodError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.p_type {
            MethodErrorType::ServerUnavailable => write!(f, "Server unavailable"),
            MethodErrorType::ServerFail => write!(f, "Server fail"),
            MethodErrorType::ServerPartialFail => write!(f, "Server partial fail"),
            MethodErrorType::UnknownMethod => write!(f, "Unknown method"),
            MethodErrorType::InvalidArguments => write!(f, "Invalid arguments"),
            MethodErrorType::InvalidResultReference => write!(f, "Invalid result reference"),
            MethodErrorType::Forbidden => write!(f, "Forbidden"),
            MethodErrorType::AccountNotFound => write!(f, "Account not found"),
            MethodErrorType::AccountNotSupportedByMethod => {
                write!(f, "Account not supported by method")
            }
            MethodErrorType::AccountReadOnly => write!(f, "Account read only"),
            MethodErrorType::RequestTooLarge => write!(f, "Request too large"),
            MethodErrorType::CannotCalculateChanges => write!(f, "Cannot calculate changes"),
            MethodErrorType::StateMismatch => write!(f, "State mismatch"),
            MethodErrorType::AlreadyExists => write!(f, "Already exists"),
            MethodErrorType::FromAccountNotFound => write!(f, "From account not found"),
            MethodErrorType::FromAccountNotSupportedByMethod => {
                write!(f, "From account not supported by method")
            }
            MethodErrorType::AnchorNotFound => write!(f, "Anchor not found"),
            MethodErrorType::UnsupportedSort => write!(f, "Unsupported sort"),
            MethodErrorType::UnsupportedFilter => write!(f, "Unsupported filter"),
            MethodErrorType::TooManyChanges => write!(f, "Too many changes"),
        }
    }
}

impl Display for ProblemDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.p_type {
            ProblemType::JMAP(err) => match err {
                JMAPError::UnknownCapability => write!(f, "Unknown capability")?,
                JMAPError::NotJSON => write!(f, "Not JSON")?,
                JMAPError::NotRequest => write!(f, "Not request")?,
                JMAPError::Limit => write!(f, "Limit")?,
            },
            ProblemType::Other(err) => f.write_str(err.as_str())?,
        }

        if let Some(status) = self.status {
            write!(f, " (status {})", status)?
        }

        if let Some(title) = &self.title {
            write!(f, ": {}", title)?
        }

        if let Some(detail) = &self.detail {
            write!(f, ". Details: {}", detail)?
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 8620 §3.6.1.1 — first problem-details example, verbatim.
    const UNKNOWN_CAPABILITY: &str = r#"{
        "type": "urn:ietf:params:jmap:error:unknownCapability",
        "status": 400,
        "detail": "The Request object used capability 'https://example.com/apis/foobar', which is not supported by this server."
    }"#;

    /// RFC 8620 §3.6.1.1 — second problem-details example, verbatim. The
    /// `limit` member names the capability limit that was hit.
    const LIMIT: &str = r#"{
        "type": "urn:ietf:params:jmap:error:limit",
        "limit": "maxSizeRequest",
        "status": 400,
        "detail": "The request is larger than the server is willing to process."
    }"#;

    fn problem(json: &str) -> ProblemDetails {
        serde_json::from_str(json).expect("problem details fixture must deserialise")
    }

    #[test]
    fn unknown_capability_problem_parses_and_renders() {
        let details = problem(UNKNOWN_CAPABILITY);

        assert_eq!(
            format!("{:?}", details.error()),
            "JMAP(UnknownCapability)",
            "the RFC example must deserialise as the registered unknownCapability type"
        );
        assert_eq!(details.status(), Some(400));
        assert_eq!(details.title(), None, "the RFC example carries no title");
        assert_eq!(
            details.limit(),
            None,
            "`limit` is only present on the limit problem"
        );
        assert_eq!(details.request_id(), None);
        assert_eq!(
            details.to_string(),
            "Unknown capability (status 400). Details: The Request object used \
             capability 'https://example.com/apis/foobar', which is not supported \
             by this server."
        );
    }

    #[test]
    fn limit_problem_exposes_the_limit_name() {
        let details = problem(LIMIT);

        assert!(
            matches!(details.error(), ProblemType::JMAP(JMAPError::Limit)),
            "the RFC limit example must deserialise as the registered limit type"
        );
        assert_eq!(details.limit(), Some("maxSizeRequest"));
        assert_eq!(details.status(), Some(400));
        assert_eq!(
            details.to_string(),
            "Limit (status 400). Details: The request is larger than the server \
             is willing to process."
        );
    }

    /// The `ProblemType` is untagged: any URN that is not one of the four
    /// registered JMAP error types must still deserialise, as `Other`.
    #[test]
    fn unregistered_problem_type_falls_back_to_other() {
        let details =
            problem(r#"{"type": "urn:example:params:error:boom", "status": 500, "title": "Boom"}"#);

        assert_eq!(
            format!("{:?}", details.error()),
            r#"Other("urn:example:params:error:boom")"#,
            "an unknown URN must land in the untagged `Other` arm"
        );
        assert_eq!(details.title(), Some("Boom"));
        assert_eq!(
            details.to_string(),
            "urn:example:params:error:boom (status 500): Boom",
            "an unregistered type renders its raw URN"
        );
    }

    /// Each of the four registered JMAP problem types has its own human
    /// readable label, and the optional members are appended independently.
    #[test]
    fn every_registered_problem_type_renders_its_own_label() {
        let cases: Vec<(JMAPError, &str)> = vec![
            (JMAPError::UnknownCapability, "Unknown capability"),
            (JMAPError::NotJSON, "Not JSON"),
            (JMAPError::NotRequest, "Not request"),
            (JMAPError::Limit, "Limit"),
        ];

        for (error, label) in cases {
            let details =
                ProblemDetails::new(ProblemType::JMAP(error), None, None, None, None, None);
            assert_eq!(
                details.to_string(),
                label,
                "with no status, title or detail only the label is written"
            );
        }
    }

    /// The Display composition is `type` + ` (status N)` + `: title` +
    /// `. Details: detail`, each part present only when the member is.
    #[test]
    fn problem_details_display_composes_only_present_members() {
        assert_eq!(
            ProblemDetails::new(
                ProblemType::Other("Boom".into()),
                None,
                None,
                None,
                None,
                None
            )
            .to_string(),
            "Boom"
        );
        assert_eq!(
            ProblemDetails::new(
                ProblemType::Other("Boom".into()),
                Some(404),
                None,
                None,
                None,
                None
            )
            .to_string(),
            "Boom (status 404)"
        );
        assert_eq!(
            ProblemDetails::new(
                ProblemType::Other("Boom".into()),
                None,
                Some("Title".into()),
                None,
                None,
                None
            )
            .to_string(),
            "Boom: Title"
        );
        assert_eq!(
            ProblemDetails::new(
                ProblemType::Other("Boom".into()),
                None,
                None,
                Some("Detail".into()),
                None,
                None
            )
            .to_string(),
            "Boom. Details: Detail"
        );
        assert_eq!(
            ProblemDetails::new(
                ProblemType::Other("Boom".into()),
                Some(500),
                Some("Title".into()),
                Some("Detail".into()),
                Some("maxSizeRequest".into()),
                Some("req-42".into()),
            )
            .to_string(),
            "Boom (status 500): Title. Details: Detail"
        );
    }

    /// `ProblemDetails::request_id` and `limit` are carried but never
    /// rendered; assert they survive a round trip through the constructor.
    #[test]
    fn problem_details_accessors_report_every_member() {
        let details = ProblemDetails::new(
            ProblemType::JMAP(JMAPError::NotRequest),
            Some(400),
            Some("Not a request".into()),
            Some("body was an array".into()),
            Some("maxCallsInRequest".into()),
            Some("req-7".into()),
        );

        assert_eq!(
            format!("{:?}", details.error()),
            "JMAP(NotRequest)",
            "`notRequest` must deserialise as the registered JMAP problem type"
        );
        assert_eq!(details.status(), Some(400));
        assert_eq!(details.title(), Some("Not a request"));
        assert_eq!(details.detail(), Some("body was an array"));
        assert_eq!(details.limit(), Some("maxCallsInRequest"));
        assert_eq!(details.request_id(), Some("req-7"));
    }

    /// RFC 8620 §3.6.2 (plus the RFC 8621 additions) — every method error
    /// type has a distinct human readable rendering.
    #[test]
    fn every_method_error_type_renders_its_own_message() {
        let cases: Vec<(&str, &str)> = vec![
            ("serverUnavailable", "Server unavailable"),
            ("serverFail", "Server fail"),
            ("serverPartialFail", "Server partial fail"),
            ("unknownMethod", "Unknown method"),
            ("invalidArguments", "Invalid arguments"),
            ("invalidResultReference", "Invalid result reference"),
            ("forbidden", "Forbidden"),
            ("accountNotFound", "Account not found"),
            (
                "accountNotSupportedByMethod",
                "Account not supported by method",
            ),
            ("accountReadOnly", "Account read only"),
            ("requestTooLarge", "Request too large"),
            ("cannotCalculateChanges", "Cannot calculate changes"),
            ("stateMismatch", "State mismatch"),
            ("alreadyExists", "Already exists"),
            ("fromAccountNotFound", "From account not found"),
            (
                "fromAccountNotSupportedByMethod",
                "From account not supported by method",
            ),
            ("anchorNotFound", "Anchor not found"),
            ("unsupportedSort", "Unsupported sort"),
            ("unsupportedFilter", "Unsupported filter"),
            ("tooManyChanges", "Too many changes"),
        ];

        assert_eq!(cases.len(), 20, "all twenty method error types");
        for (rfc_type, message) in cases {
            let error: MethodError = serde_json::from_str(&format!(r#"{{"type":"{rfc_type}"}}"#))
                .unwrap_or_else(|e| panic!("{rfc_type} must deserialise: {e}"));
            assert_eq!(
                error.to_string(),
                message,
                "{rfc_type} must render as {message:?}"
            );
        }
    }

    /// A method-level error response body deserialises into `MethodError`
    /// and the type survives the round trip.
    #[test]
    fn method_error_deserialises_from_a_method_response_body() {
        let error: MethodError =
            serde_json::from_str(r#"{"type": "cannotCalculateChanges"}"#).unwrap();
        assert_eq!(
            error.error(),
            &MethodErrorType::CannotCalculateChanges,
            "RFC 8620 §5.2.2 cannotCalculateChanges"
        );
        assert_eq!(error.to_string(), "Cannot calculate changes");
    }
}
