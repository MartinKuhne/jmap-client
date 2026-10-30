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

use serde::{Deserialize, Serialize};

use super::{BodyProperty, Email, Property};
use crate::{core::RequestParams, Error};
use ahash::AHashMap;

#[derive(Debug, Clone, Serialize)]
pub struct EmailParseRequest {
    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "blobIds")]
    blob_ids: Vec<String>,

    #[serde(rename = "properties")]
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<Vec<Property>>,

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

#[derive(Debug, Clone, Deserialize)]
pub struct EmailParseResponse {
    #[serde(rename = "accountId")]
    account_id: String,

    #[serde(rename = "parsed")]
    parsed: Option<AHashMap<String, Email>>,

    #[serde(rename = "notParsable")]
    not_parsable: Option<Vec<String>>,

    #[serde(rename = "notFound")]
    not_found: Option<Vec<String>>,
}

impl EmailParseRequest {
    pub fn new(params: RequestParams) -> Self {
        EmailParseRequest {
            account_id: params.account_id,
            blob_ids: Vec::new(),
            properties: None,
            body_properties: None,
            fetch_text_body_values: None,
            fetch_html_body_values: None,
            fetch_all_body_values: None,
            max_body_value_bytes: None,
        }
    }

    pub fn blob_ids<U, V>(&mut self, blob_ids: U) -> &mut Self
    where
        U: IntoIterator<Item = V>,
        V: Into<String>,
    {
        self.blob_ids = blob_ids.into_iter().map(|v| v.into()).collect();
        self
    }

    pub fn properties(&mut self, properties: impl IntoIterator<Item = Property>) -> &mut Self {
        self.properties = Some(properties.into_iter().collect());
        self
    }

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

impl EmailParseResponse {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn parsed(&mut self, blob_id: &str) -> crate::Result<Email> {
        if let Some(result) = self.parsed.as_mut().and_then(|r| r.remove(blob_id)) {
            Ok(result)
        } else if self
            .not_parsable
            .as_ref()
            .map(|np| np.iter().any(|id| id == blob_id))
            .unwrap_or(false)
        {
            Err(Error::Internal(format!(
                "blobId {} is not parsable.",
                blob_id
            )))
        } else {
            Err(Error::Internal(format!("blobId {} not found.", blob_id)))
        }
    }

    pub fn parsed_list(&self) -> Option<impl Iterator<Item = (&String, &Email)>> {
        self.parsed.as_ref().map(|map| map.iter())
    }

    pub fn not_parsable(&self) -> Option<&[String]> {
        self.not_parsable.as_deref()
    }

    pub fn not_found(&self) -> Option<&[String]> {
        self.not_found.as_deref()
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the `Email/parse` request builders and the response
    //! accessors (RFC 8621 §4.11).

    use super::*;
    use crate::Method;
    use serde_json::json;

    fn request() -> EmailParseRequest {
        EmailParseRequest::new(RequestParams::new("u33084183", Method::ParseEmail, 0))
    }

    /// RFC 8621 §4.11 — a bare request carries `accountId` and the
    /// (empty) `blobIds` list; every optional member is suppressed.
    #[test]
    fn a_bare_request_carries_only_the_required_members() {
        assert_eq!(
            serde_json::to_value(request()).unwrap(),
            json!({ "accountId": "u33084183", "blobIds": [] })
        );
    }

    /// RFC 8621 §4.11 — every builder, and the exact wire shape they
    /// produce together.
    #[test]
    fn every_builder_lands_on_its_rfc_member() {
        let mut request = request();
        request
            .blob_ids(["blob1", "blob2"])
            .properties([Property::Id, Property::Subject, Property::BodyStructure])
            .body_properties([BodyProperty::PartId, BodyProperty::Type])
            .fetch_text_body_values(true)
            .fetch_html_body_values(false)
            .fetch_all_body_values(true)
            .max_body_value_bytes(1024);

        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "accountId": "u33084183",
                "blobIds": ["blob1", "blob2"],
                "properties": ["id", "subject", "bodyStructure"],
                "bodyProperties": ["partId", "type"],
                "fetchTextBodyValues": true,
                "fetchHTMLBodyValues": false,
                "fetchAllBodyValues": true,
                "maxBodyValueBytes": 1024
            })
        );
    }

    /// `blob_ids` is generic over `Into<String>` and replaces the list
    /// rather than appending to it.
    #[test]
    fn blob_ids_replaces_the_whole_list() {
        let mut request = request();
        request
            .blob_ids(vec!["owned".to_string()])
            .blob_ids(["a", "b", "c"]);
        assert_eq!(
            serde_json::to_value(&request).unwrap()["blobIds"],
            json!(["a", "b", "c"])
        );

        request.blob_ids(Vec::<String>::new());
        assert_eq!(
            serde_json::to_value(&request).unwrap()["blobIds"],
            json!([]),
            "an empty list is still sent: blobIds is not optional"
        );
    }

    /// RFC 8621 §4.11 — the `Email/parse` result shape.
    const RESPONSE: &str = r#"{
        "accountId": "u33084183",
        "parsed": {
            "blob1": {
                "id": "M8a6cfa8094c0f41e6",
                "blobId": "blob1",
                "subject": "Parsed message"
            }
        },
        "notParsable": ["blob2"],
        "notFound": ["blob3"]
    }"#;

    /// `parsed(id)` hands the record out and removes it from the map, so
    /// a second read of the same blob cannot succeed.
    #[test]
    fn parsed_hands_out_the_record_and_drains_it() {
        let mut response: EmailParseResponse = serde_json::from_str(RESPONSE).unwrap();

        assert_eq!(response.account_id(), "u33084183");
        assert_eq!(response.parsed_list().unwrap().count(), 1);

        let email = response.parsed("blob1").unwrap();
        assert_eq!(email.id(), Some("M8a6cfa8094c0f41e6"));
        assert_eq!(email.subject(), Some("Parsed message"));

        let error = response.parsed("blob1").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Internal error: blobId blob1 not found.",
            "the record was taken out of the map"
        );
    }

    /// A blob the server could not parse and a blob it never had are
    /// reported with different messages.
    #[test]
    fn not_parsable_and_missing_blobs_report_distinctly() {
        let mut response: EmailParseResponse = serde_json::from_str(RESPONSE).unwrap();

        assert_eq!(response.not_parsable().unwrap(), ["blob2"]);
        assert_eq!(response.not_found().unwrap(), ["blob3"]);

        let error = response.parsed("blob2").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Internal error: blobId blob2 is not parsable."
        );

        let error = response.parsed("blob3").unwrap_err();
        assert_eq!(error.to_string(), "Internal error: blobId blob3 not found.");

        let error = response.parsed("never-mentioned").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Internal error: blobId never-mentioned not found."
        );
    }

    /// A response with no optional members at all: the list iterator is
    /// absent and both error lists read as `None`.
    #[test]
    fn a_response_with_only_the_required_member() {
        let mut response: EmailParseResponse =
            serde_json::from_value(json!({ "accountId": "u33084183" })).unwrap();

        assert_eq!(response.account_id(), "u33084183");
        assert!(response.parsed_list().is_none());
        assert!(response.not_parsable().is_none());
        assert!(response.not_found().is_none());
        assert_eq!(
            response.parsed("blob1").unwrap_err().to_string(),
            "Internal error: blobId blob1 not found."
        );
    }
}
