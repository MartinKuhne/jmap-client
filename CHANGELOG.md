jmap-client 0.5.0
================================
RFC compliance fixes. Several breaking changes; each is marked.

- Unrecognised method-error types are no longer swallowed as an empty success.
  `MethodErrorType` gains an `Other(String)` catch-all, since RFC 8620 §3.6.2
  requires a client to treat an unknown type the same as `serverFail`, and
  `is_server_fail_equivalent()` is provided to make that reachable.
  **Breaking:** exhaustive matches on `MethodErrorType` without a wildcard arm no
  longer compile.
- A method response is now classified as an error from the first element of the
  invocation alone, so a malformed error payload can no longer be absorbed as an
  all-`None` `SetResponse`.
- `send_single` returns the response that answers the request's call id instead
  of the last one in the array, so `Email/copy` with `onSuccessDestroyOriginal`
  and `EmailSubmission/set` with an implicit `Email/set` no longer report a
  successful operation as a parse error (RFC 8620 §3.2, §3.4; RFC 8621 §7.5.1).
- `PushSubscription/get` responses now parse without a `state` member, which
  RFC 8620 §7.2.1 says the method does not return.
- Sieve deactivation sends `onSuccessActivateScript: null` per
  draft-ietf-jmap-sieve §2.2 rather than a member that does not exist in the
  draft, which servers ignored, so the call reported success while the script
  stayed active. **Breaking:** `SieveSetRequest::on_success_deactivate_script()`
  is removed; sieve uploads default to `application/sieve`.
- RFC 9670 principal property names: `timeZone`, `shareWith`, and `capabilities`
  as an object keyed by URI. The client previously requested `shareWith` while
  reading `acl`, so a server's ACL answer was discarded. **Breaking:**
  `Principal::capabilities()` returns a map of URI to value rather than a list of
  strings.
- Capability limits are now reachable: `Account::{core,mail,submission,sieve}_
  capabilities()` read `accountCapabilities`, where RFC 8621 §1.3.1/§1.3.2 puts
  them; the session-level values are empty objects. **Breaking:**
  `max_mailbox_depth()` and `max_size_attachments_per_email()` return
  `Option<usize>`, and `submission_extensions()` returns a map of extension name
  to parameters per §1.3.2.
- `DataType` gains `Other(String)` so a data type the client does not model no
  longer fails to parse (RFC 8620 §7.2). **Breaking:** integer `as` casts on
  `DataType` no longer compile, and exhaustive matches need a wildcard arm.
- WebSocket pushes keep the RFC 8887 §5.2 `pushState`, which is the token needed
  to resume push after a reconnect. **Breaking:**
  `WebSocketMessage::PushNotification` now wraps `WebSocketPushObject`; use
  `.push` for the `PushObject` and `.push_state` for the token.
- `application/problem+json` error responses are recognised regardless of a
  `charset` parameter or letter case, instead of degrading to a bare status.
- The SSE parser keeps unconsumed bytes across reads and resets after an
  oversized event, rather than going permanently silent.
- The crate compiles with `--all-features`, and the async-only integration tests
  are feature-gated so the `blocking` feature can be tested.

jmap-client 0.4.3
================================
- Replace `base64` with `encodify`.

jmap-client 0.4.2
================================
- JMAP compliance fixes. 

jmap-client 0.4.1
================================
- Use `rustls-tls-native-roots` for TLS support.

jmap-client 0.4.0
================================
- Calendar Alerts support.

jmap-client 0.3.3
================================
- JMAP for Sharing support.

jmap-client 0.3.2
================================
- Bump to `rustls` 0.22.

jmap-client 0.3.0
================================
- JMAP for Sieve Scripts DRAFT-14 support.
- Set timeouts using `Duration` instead of `u64`.
- SetError handling of unknown properties.
- Support deserializing non-IANA registered roles.

jmap-client 0.2.1
================================
- Using maybe_async to reduce duplicate code.
- Added `accept_invalid_certs` option to builder.

jmap-client 0.2.0
================================
- JMAP for Sieve Scripts support.

jmap-client 0.1.0
================================
- First release.
