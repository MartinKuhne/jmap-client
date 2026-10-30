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

use std::{
    pin::Pin,
    sync::{atomic::Ordering, Arc},
};

use ahash::AHashMap;
use futures_util::{stream::SplitSink, SinkExt, Stream, StreamExt};
use reqwest::header::SEC_WEBSOCKET_PROTOCOL;
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    ClientConfig, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, Message},
    Connector, MaybeTlsStream, WebSocketStream,
};

use crate::{
    client::Client,
    core::{
        error::{ProblemDetails, ProblemType},
        request::{Arguments, Request},
        response::{Response, TaggedMethodResponse},
    },
    DataType, Method, PushObject, URI,
};

#[derive(Debug, Serialize)]
struct WebSocketRequest {
    #[serde(rename = "@type")]
    pub _type: WebSocketRequestType,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    using: Vec<URI>,

    #[serde(rename = "methodCalls")]
    method_calls: Vec<(Method, Arguments, String)>,

    #[serde(rename = "createdIds")]
    #[serde(skip_serializing_if = "Option::is_none")]
    created_ids: Option<AHashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct WebSocketResponse {
    #[serde(rename = "@type")]
    _type: WebSocketResponseType,

    #[serde(rename = "requestId")]
    request_id: Option<String>,

    #[serde(rename = "methodResponses")]
    method_responses: Vec<TaggedMethodResponse>,

    #[serde(rename = "createdIds")]
    created_ids: Option<AHashMap<String, String>>,

    #[serde(rename = "sessionState")]
    session_state: String,
}

#[derive(Debug, Serialize, Deserialize)]
enum WebSocketResponseType {
    Response,
}

#[derive(Debug, Serialize)]
struct WebSocketPushEnable {
    #[serde(rename = "@type")]
    _type: WebSocketPushEnableType,

    #[serde(rename = "dataTypes")]
    data_types: Option<Vec<DataType>>,

    #[serde(rename = "pushState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    push_state: Option<String>,
}

#[derive(Debug, Serialize)]
struct WebSocketPushDisable {
    #[serde(rename = "@type")]
    _type: WebSocketPushDisableType,
}

#[derive(Debug, Serialize)]
enum WebSocketRequestType {
    Request,
}

#[derive(Debug, Serialize)]
enum WebSocketPushEnableType {
    WebSocketPushEnable,
}

#[derive(Debug, Serialize)]
enum WebSocketPushDisableType {
    WebSocketPushDisable,
}

#[derive(Deserialize, Debug)]
pub struct WebSocketPushObject {
    #[serde(flatten)]
    pub push: PushObject,

    #[serde(rename = "pushState")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebSocketError {
    #[serde(rename = "@type")]
    pub type_: WebSocketErrorType,

    #[serde(rename = "requestId")]
    pub request_id: Option<String>,

    #[serde(rename = "type")]
    p_type: ProblemType,
    status: Option<u32>,
    title: Option<String>,
    detail: Option<String>,
    limit: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WebSocketErrorType {
    RequestError,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WebSocketMessage_ {
    Response(WebSocketResponse),
    PushNotification(WebSocketPushObject),
    Error(WebSocketError),
}

#[derive(Debug)]
pub enum WebSocketMessage {
    Response(Response<TaggedMethodResponse>),
    /// A push notification, with the RFC 8887 §5.2 `pushState` kept alongside
    /// the [`PushObject`].
    ///
    /// RFC 8887 §5.2 defines the WebSocketPush object as a PushNotification
    /// plus `pushState: String|null`, the id of the last event. That token is
    /// what a client passes back to `Client::enable_push_ws` after reconnecting
    /// so the server replays what it missed (RFC 8620 §7.1 `Last-Event-ID`);
    /// without it, live push cannot be resumed. Read the notification itself
    /// through `.push` and the resume token through `.push_state`.
    PushNotification(WebSocketPushObject),
}

pub struct WsStream {
    tx: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    req_id: usize,
}

#[doc(hidden)]
#[derive(Debug)]
struct DummyVerifier;

impl ServerCertVerifier for DummyVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

impl Client {
    pub async fn connect_ws(
        &self,
    ) -> crate::Result<Pin<Box<impl Stream<Item = crate::Result<WebSocketMessage>>>>> {
        let session = self.session();
        let capabilities = session.websocket_capabilities().ok_or_else(|| {
            crate::Error::Internal(
                "JMAP server does not advertise any websocket capabilities.".to_string(),
            )
        })?;

        let mut request = capabilities.url().into_client_request()?;
        request
            .headers_mut()
            .insert("Authorization", self.authorization.parse().unwrap());
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, "jmap".parse().unwrap());

        let (stream, _) = if self.accept_invalid_certs & capabilities.url().starts_with("wss") {
            tokio_tungstenite::connect_async_tls_with_config(
                request,
                None,
                false,
                Connector::Rustls(Arc::new(
                    ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(DummyVerifier {}))
                        .with_no_client_auth(),
                ))
                .into(),
            )
            .await?
        } else {
            tokio_tungstenite::connect_async(request).await?
        };
        let (tx, mut rx) = stream.split();

        *self.ws.lock().await = WsStream { tx, req_id: 0 }.into();

        // The stream returned below is `'static`, so it cannot borrow `self`.
        // Share the session and the "session is up to date" flag instead of taking
        // a snapshot, so every response is compared against the *live* cached
        // session (a snapshot would wrongly report staleness as soon as the
        // session is refreshed over HTTP). This mirrors the check performed by
        // `Client::send()` on the HTTP path, see issue #31.
        let session_handle = self.session_handle();
        let session_updated = self.session_updated_flag();

        Ok(Box::pin(async_stream::stream! {
            while let Some(message) = rx.next().await {
                match message {
                    Ok(message) if message.is_text() => {
                        match serde_json::from_slice::<WebSocketMessage_>(&message.into_data()) {
                            Ok(message) => match message {
                                WebSocketMessage_::Response(response) => {
                                    if response.session_state != session_handle.lock().state() {
                                        session_updated.store(false, Ordering::Relaxed);
                                    }
                                    yield Ok(WebSocketMessage::Response(Response::new(
                                        response.method_responses,
                                        response.created_ids,
                                        response.session_state,
                                        response.request_id,
                                    )))
                                }
                                WebSocketMessage_::PushNotification(push) => {
                                    // RFC 8887 §5.2: carry `pushState` through with the
                                    // notification; it is the token a client needs to
                                    // resume push after a reconnect.
                                    yield Ok(WebSocketMessage::PushNotification(push))
                                }
                                WebSocketMessage_::Error(err) => yield Err(ProblemDetails::from(err).into()),
                            },
                            Err(err) => yield Err(err.into()),
                        }
                    }
                    Ok(_) => (),
                    Err(err) => yield Err(err.into()),
                }
            }
        }))
    }

    pub async fn send_ws(&self, request: Request<'_>) -> crate::Result<String> {
        let mut _ws = self.ws.lock().await;
        let ws = _ws
            .as_mut()
            .ok_or_else(|| crate::Error::Internal("Websocket stream not set.".to_string()))?;

        // Assign request id
        let request_id = ws.req_id.to_string();
        ws.req_id += 1;

        ws.tx
            .send(Message::text(
                serde_json::to_string(&WebSocketRequest {
                    _type: WebSocketRequestType::Request,
                    id: request_id.clone().into(),
                    using: request.using,
                    method_calls: request.method_calls,
                    created_ids: request.created_ids,
                })
                .unwrap_or_default(),
            ))
            .await?;

        Ok(request_id)
    }

    pub async fn enable_push_ws(
        &self,
        data_types: Option<impl IntoIterator<Item = DataType>>,
        push_state: Option<impl Into<String>>,
    ) -> crate::Result<()> {
        self.ws
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| crate::Error::Internal("Websocket stream not set.".to_string()))?
            .tx
            .send(Message::text(
                serde_json::to_string(&WebSocketPushEnable {
                    _type: WebSocketPushEnableType::WebSocketPushEnable,
                    data_types: data_types.map(|it| it.into_iter().collect()),
                    push_state: push_state.map(|it| it.into()),
                })
                .unwrap_or_default(),
            ))
            .await
            .map_err(|err| err.into())
    }

    pub async fn disable_push_ws(&self) -> crate::Result<()> {
        self.ws
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| crate::Error::Internal("Websocket stream not set.".to_string()))?
            .tx
            .send(Message::text(
                serde_json::to_string(&WebSocketPushDisable {
                    _type: WebSocketPushDisableType::WebSocketPushDisable,
                })
                .unwrap_or_default(),
            ))
            .await
            .map_err(|err| err.into())
    }

    pub async fn ws_ping(&self) -> crate::Result<()> {
        self.ws
            .lock()
            .await
            .as_mut()
            .ok_or_else(|| crate::Error::Internal("Websocket stream not set.".to_string()))?
            .tx
            .send(Message::Ping(vec![].into()))
            .await
            .map_err(|err| err.into())
    }
}

impl From<WebSocketError> for ProblemDetails {
    fn from(problem: WebSocketError) -> Self {
        ProblemDetails::new(
            problem.p_type,
            problem.status,
            problem.title,
            problem.detail,
            problem.limit,
            problem.request_id,
        )
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for WebSocket session-state handling and the `DummyVerifier`
    //! used when the caller enables `accept_invalid_certs` (RFC 8887 §4.2).

    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            handshake::server::{Request, Response},
            http::HeaderValue,
            Message,
        },
    };

    use crate::{client::Client, client_ws::WebSocketMessage, core::session::Session};

    /// Session state the client under test has cached.
    const CACHED_SESSION_STATE: &str = "cached-state-AAA";

    /// A JMAP session document that advertises the WebSocket capability at
    /// `ws_url` and carries `state` as its session state.
    fn session_json(state: &str, ws_url: &str) -> String {
        format!(
            r#"{{
                "capabilities": {{
                    "urn:ietf:params:jmap:core": {{
                        "maxSizeUpload": 52428800,
                        "maxConcurrentUpload": 2,
                        "maxSizeRequest": 10485760,
                        "maxConcurrentRequests": 2,
                        "maxCallsInRequest": 16,
                        "maxObjectsInGet": 512,
                        "maxObjectsInSet": 512,
                        "collationAlgorithms": ["i;ascii-numeric"]
                    }},
                    "urn:ietf:params:jmap:websocket": {{
                        "url": "{ws_url}",
                        "supportsPush": true
                    }}
                }},
                "accounts": {{}},
                "primaryAccounts": {{}},
                "username": "test@example.org",
                "apiUrl": "https://example.org/jmap/api/",
                "downloadUrl": "https://example.org/jmap/download/{{accountId}}/{{blobId}}/{{name}}?accept={{type}}",
                "uploadUrl": "https://example.org/jmap/upload/{{accountId}}",
                "eventSourceUrl": "https://example.org/jmap/event/?types={{types}}&closeafter={{closeafter}}&ping={{ping}}",
                "state": "{state}"
            }}"#
        )
    }

    /// A canned `WebSocketResponse` frame as sent by a RFC 8887 compliant server.
    fn ws_response_json(session_state: &str) -> String {
        format!(
            r#"{{
                "@type": "Response",
                "requestId": "0",
                "methodResponses": [["Core/echo", {{}}, "c0"]],
                "createdIds": {{}},
                "sessionState": "{session_state}"
            }}"#
        )
    }

    /// Starts a local WebSocket server that answers the first connection with a
    /// single canned text frame and then keeps the connection open for a while.
    #[allow(clippy::result_large_err)]
    async fn spawn_ws_server(frame: String) -> (u16, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // `connect_ws()` asks for the "jmap" sub-protocol, so the server has to
            // echo it back or the client aborts the handshake.
            let mut ws = accept_hdr_async(stream, |_request: &Request, mut response: Response| {
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", HeaderValue::from_static("jmap"));
                Ok(response)
            })
            .await
            .unwrap();
            let _ = ws.send(Message::Text(frame.into())).await;
            // Stay alive so the client is not raced by an early close.
            let _ = tokio::time::sleep(Duration::from_secs(5)).await;
        });
        (port, handle)
    }

    fn test_client(cached_state: &str, ws_url: &str) -> Client {
        let session: Session = serde_json::from_str(&session_json(cached_state, ws_url)).unwrap();
        Client::from_session_for_tests(session)
    }

    async fn first_message(client: &Client) -> WebSocketMessage {
        let mut stream = client.connect_ws().await.unwrap();
        timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("timed out waiting for the canned WebSocket response")
            .expect("WebSocket stream ended unexpectedly")
            .expect("WebSocket stream yielded an error")
    }

    #[tokio::test]
    async fn ws_response_with_changed_session_state_marks_session_stale() {
        let (port, _server) = spawn_ws_server(ws_response_json("server-state-BBB")).await;
        let client = test_client(
            CACHED_SESSION_STATE,
            &format!("ws://127.0.0.1:{port}/jmap/ws"),
        );

        assert!(
            client.is_session_updated(),
            "pre-condition: a freshly built client reports an up-to-date session"
        );

        let message = first_message(&client).await;

        assert!(
            matches!(message, WebSocketMessage::Response(_)),
            "expected a WebSocket response message, got {message:?}"
        );
        assert_eq!(message_session_state(&message), "server-state-BBB");
        assert!(
            !client.is_session_updated(),
            "a WebSocket response whose sessionState differs from the cached session must mark \
             the session stale, just like the HTTP path does"
        );
    }

    #[tokio::test]
    async fn ws_response_with_unchanged_session_state_keeps_session_updated() {
        let (port, _server) = spawn_ws_server(ws_response_json(CACHED_SESSION_STATE)).await;
        let client = test_client(
            CACHED_SESSION_STATE,
            &format!("ws://127.0.0.1:{port}/jmap/ws"),
        );

        assert!(
            client.is_session_updated(),
            "pre-condition: a freshly built client reports an up-to-date session"
        );

        let message = first_message(&client).await;

        assert!(
            matches!(message, WebSocketMessage::Response(_)),
            "expected a WebSocket response message, got {message:?}"
        );
        assert_eq!(message_session_state(&message), CACHED_SESSION_STATE);
        assert!(
            client.is_session_updated(),
            "a WebSocket response whose sessionState matches the cached session must not mark \
             the session stale"
        );
    }

    fn message_session_state(message: &WebSocketMessage) -> &str {
        match message {
            WebSocketMessage::Response(response) => response.session_state(),
            WebSocketMessage::PushNotification(_) => panic!("expected a response, got a push"),
        }
    }
    use super::{DummyVerifier, ServerCertVerifier, SignatureScheme};
    use rustls::internal::msgs::codec::{Codec, Reader};
    use rustls::DigitallySignedStruct;
    use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

    /// An empty end-entity certificate, no intermediates, no OCSP
    /// response and the epoch: the verifier looks at none of it and
    /// returns the `ServerCertVerified::assertion()` marker.
    #[test]
    fn dummy_verifier_accepts_any_server_certificate() {
        let verified = DummyVerifier
            .verify_server_cert(
                &CertificateDer::from(&b""[..]),
                &[],
                &ServerName::try_from("example.org").unwrap(),
                &[],
                UnixTime::since_unix_epoch(std::time::Duration::from_secs(1_767_225_600)),
            )
            .expect("the dummy verifier never rejects a chain");

        assert_eq!(format!("{verified:?}"), "ServerCertVerified(())");
    }

    /// `DigitallySignedStruct` has no public constructor, so build one
    /// from its wire form: the scheme as two bytes, then a
    /// `u16`-length signature.
    fn signed_struct(scheme: SignatureScheme) -> DigitallySignedStruct {
        let mut bytes = scheme.to_array().to_vec();
        bytes.extend_from_slice(&[0x00, 0x04, 0xde, 0xad, 0xbe, 0xef]);

        let mut reader = Reader::init(&bytes);
        let dss = DigitallySignedStruct::read(&mut reader).expect("the fixture must parse");
        assert_eq!(dss.scheme, scheme, "the scheme survives the round trip");
        assert_eq!(dss.signature(), &[0xde, 0xad, 0xbe, 0xef]);
        dss
    }

    /// Both signature hooks ignore the message, the certificate and the
    /// scheme they are handed, and return
    /// `HandshakeSignatureValid::assertion()`.
    #[test]
    fn dummy_verifier_accepts_tls12_and_tls13_signatures() {
        let cert = CertificateDer::from(&b""[..]);

        for scheme in [
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
        ] {
            let dss = signed_struct(scheme);

            let tls12 = DummyVerifier
                .verify_tls12_signature(b"handshake message", &cert, &dss)
                .expect("the dummy verifier never rejects a TLS 1.2 signature");
            assert_eq!(
                format!("{tls12:?}"),
                "HandshakeSignatureValid(())",
                "{scheme:?} over TLS 1.2"
            );

            let tls13 = DummyVerifier
                .verify_tls13_signature(b"handshake message", &cert, &dss)
                .expect("the dummy verifier never rejects a TLS 1.3 signature");
            assert_eq!(
                format!("{tls13:?}"),
                "HandshakeSignatureValid(())",
                "{scheme:?} over TLS 1.3"
            );
        }
    }

    /// The verifier advertises the full thirteen-entry scheme list, in the
    /// order the source writes it: every PKCS#1, ECDSA, RSA-PSS, Ed25519
    /// and Ed448 scheme. A server that picks any of them meets a verifier
    /// that has already claimed to support it.
    #[test]
    fn dummy_verifier_advertises_all_thirteen_schemes() {
        let schemes = DummyVerifier.supported_verify_schemes();

        assert_eq!(
            schemes,
            vec![
                SignatureScheme::RSA_PKCS1_SHA1,
                SignatureScheme::ECDSA_SHA1_Legacy,
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
                SignatureScheme::ED25519,
                SignatureScheme::ED448,
            ]
        );
        assert_eq!(schemes.len(), 13, "thirteen schemes, no more");
    }
}
