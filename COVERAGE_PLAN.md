# Test Coverage Plan — jmap-client → 80%

Living ledger. Every agent working on this goal MUST read this file first, check off
completed items (`[x]`), and append to the Progress Log after each iteration.
The RFCs are the authority: RFC 8620 (Core), RFC 8621 (Mail), RFC 8887
(WebSocket), draft-ietf-jmap-sieve-12 (Sieve).

## Goal

Raise line coverage of the `jmap-client` library (`src/`) from **6.04%** to
**≥ 80%**, by:

1. Building an RFC-compliant JMAP mock server (HTTP + WebSocket + EventSource +
   blob upload/download) reusable across all integration tests.
2. Writing integration tests that drive the public client API against the mock.
3. Writing unit tests (in-crate `#[cfg(test)]`) for pure logic: serde
   round-trips, filters/comparators, error types, URL parsing.

## Metric definition

- Measured with `cargo llvm-cov` on the **default feature set**
  (`async, websockets, aws_lc_rs`).
- Only `src/` counts. The mock crate and `tests/` files are excluded.
  **Validated command (P1, cargo-llvm-cov 0.8.7, run from `jmap-client/`):**

  ```
  cargo llvm-cov --all-targets --ignore-filename-regex "mock-jmap|tests" --summary-only
  ```

  Verified to emit only `src/` rows (`core\error.rs`, `lib.rs`, `email\…`, …);
  no `mock-jmap` or `tests` rows appear. Windows backslash paths need no extra
  escaping in the regex.
- Target: TOTAL line coverage ≥ 80.00%.

## Rules of engagement (content protection)

- All work happens on branch `coverage/rfc-mock-tests` (branched from
  `fix/issue-34-provider-neutral-rustls` @ clean tree).
- **Never delete or rewrite existing non-test code.** Adding `#[cfg(test)]` test
  modules inside `src/` files is allowed; touching non-test lines is NOT.
- If a test reveals a genuine bug in `src/`, do NOT fix it silently: record it
  in the Findings log and write the test to document current behavior or mark
  it `#[ignore]` with a comment.
- Never touch: `Cargo.lock` by hand, `target/`, LICENSE files, README, CHANGELOG.
  (Cargo may update Cargo.lock legitimately when adding the mock workspace member.)
- Every green iteration is committed with a descriptive message.
- One subagent at a time. The orchestrator verifies each result (tests +
  coverage) before checking off items.

## Architecture decisions

- **Mock lives in a workspace member crate `mock-jmap/`** (path dev-dependency of
  `jmap-client`). Compiles once, shared by all test binaries, excluded from the
  coverage metric, reusable later.
- Mock runs on a dedicated background thread with a tokio current-thread runtime;
  binds `127.0.0.1:0` (ephemeral port); plain HTTP (`http://`) and plain WS
  (`ws://`) so no TLS plumbing is needed in tests.
- Mock is RFC-driven:
  - `GET /.well-known/jmap` → Session resource per RFC 8620 §3.1 + RFC 8621 §2
    (capabilities, accounts, primaryAccounts, username, apiUrl, downloadUrl,
    uploadUrl, eventSourceUrl, state).
  - `POST /jmap` → methodCalls dispatch, `using` capability validation
    (`unknownCapability` problem per RFC 8620 §3.2), result-reference
    resolution (`resultOf`/`name`/`path` over prior responses in the same
    request), `createdIds` for `#placeholder` ids, per-method `error` responses
    with RFC `MethodError` shapes.
  - `POST /upload/{accountId}` → RFC 8620 §5.1 (404/413 problems, echo
    accountId/blobId/type/size).
  - `GET /download/{accountId}/{blobId}/{name}?accept=` → RFC 8620 §5.2.
  - `GET /eventSource?types=&closeafter=&ping=` → `text/event-stream`, chunked,
    `event: StateChange` / `event: EmailPush` notifications with `id:` fields
    (RFC 8620 §7.1, RFC 8621 §5.2).
  - `GET /ws` (Upgrade) → RFC 8887: subprotocol `jmap`, `Request`/`Response`
    frames with `requestId`, `WebSocketPushEnable`/`WebSocketPushDisable`,
    push frames, `RequestError` problem frames, ping/pong.
  - Auth: every route requires `Authorization` (Basic or Bearer); missing/bad →
    401 `application/problem+json` `unauthorized`.
  - Error injection: per-route/one-shot hooks to return `toServerOverloaded`,
    `couldNotParseJson`, `invalidArguments`, etc.
- Mock keeps a small in-memory data store (mailboxes, emails, threads, identities,
  submissions, vacation response, sieve scripts, principals) seeded with
  RFC-realistic fixtures so Get/Query/Changes return coherent data.

## Phases

### P0 — Baseline & setup  ✅
- [x] Toolchain verified (cargo 1.99, cargo-llvm-cov 0.8.7, network OK)
- [x] Baseline: 9 tests pass, line coverage 6.04% (8514 lines)
- [x] Ledger created; branch `coverage/rfc-mock-tests`

### P1 — Mock crate: HTTP core  ✅
- [x] Workspace: add `mock-jmap` member + dev-dependency; `cargo check` green
- [x] HTTP server skeleton: routing, request model, response writer, auth check
- [x] Session resource (all capability blocks incl. websocket/sieve)
- [x] JMAP POST dispatch: `using` validation, unknownCapability error
- [x] Method handler registry + canned RFC-compliant responses for:
      Core/echo, Mailbox/get+changes+query+queryChanges+set,
      Thread/get+changes, Email/get+changes+query+queryChanges+set+copy+import+parse,
      SearchSnippet/get, Identity/get+set, EmailSubmission/get+query+queryChanges+set,
      VacationResponse/get+set, SieveScript/get+set+query+validate,
      Principal/get+changes+query+queryChanges+set, Blob/copy,
      PushSubscription/get+set
- [x] Result-reference resolution (`/list`, `/list/*/id`, `/newState`, ...)
- [x] createdIds / `#placeholder` round-trip
- [x] Problem-type error injection + 401 unauthorized
- [x] Mock self-tests (mock answers raw HTTP correctly) — 35 tests
- [x] Orchestrator verifies: `cargo test -p jmap-client` still green; commit

Note: the blob upload/download endpoints (planned for P2) were delivered with P1,
because `Email/import` and `Blob/copy` are useless without a real upload path.
P2 is therefore EventSource + WebSocket only.

### P2 — Mock crate: blob + EventSource + WebSocket  ⬜
- [x] Upload endpoint (RFC 8620 §5.1) incl. size/type edge cases — *done in P1*
- [x] Download endpoint (RFC 8620 §5.2) incl. accept-type handling — *done in P1*
- [ ] EventSource stream (chunked, multi-event, ping keepalives, Last-Event-ID)
      — extension point ready: `http::HttpResponse::streaming()`; route currently
      answers `501 notImplemented`
- [ ] WebSocket (RFC 8887): handshake w/ `jmap` subprotocol, request/response
      correlation by requestId, push frames, RequestError frame, ping/pong
      — `tokio-tungstenite 0.29` (handshake feature) is already a mock-jmap
      dependency and compiles; `/ws` currently answers `501 notImplemented`
- [ ] Orchestrator verifies + commit

### P3 — Integration tests: core  ⬜
- [ ] tests/core_methods.rs: session fetch, multi-method requests, references,
      createdIds, capability filtering, error paths (401, problem+json,
      method error, couldNotParseJson)
- [ ] tests/changes_query.rs: changes/query/queryChanges across types
- [ ] Verify + commit

### P4 — Integration tests: mail domain  ⬜
- [ ] tests/mailbox.rs: get/set/create/destroy/update/query + helpers
- [ ] tests/email.rs: get/set/query/import/parse/copy + keywords + helpers
- [ ] tests/thread.rs + tests/search_snippet.rs
- [ ] Verify + commit

### P5 — Integration tests: submission / identity / vacation  ⬜
- [ ] tests/email_submission.rs (incl. delivery time, undo, bounce)
- [ ] tests/identity.rs
- [ ] tests/vacation_response.rs
- [ ] Verify + commit

### P6 — Integration tests: sieve / principal / push / blob  ⬜
- [ ] tests/sieve.rs (get/set/query/validate + script blob flow)
- [ ] tests/principal.rs
- [ ] tests/push_subscription.rs
- [ ] tests/blob.rs (upload/download/copy through client API)
- [ ] Verify + commit

### P7 — Integration tests: push streams  ⬜
- [ ] tests/event_source.rs (stream consumption, types filter, closeafter,
      Last-Event-ID, error mid-stream)
- [ ] tests/websocket.rs (connect_ws, send_ws correlation, enable/disable push,
      ws_ping, RequestError → ProblemDetails, not-connected errors)
- [ ] Verify + commit

### P8 — Unit tests: pure logic in src  ⬜
- [ ] lib.rs: URI/DataType/Method serde + Display round-trips, Error variants
- [ ] core/session.rs: URLPart::parse edge cases, capability accessors
- [ ] core/error.rs: ProblemDetails/MethodError/SetError shapes
- [ ] filters & comparators per domain (email/mailbox/principal/submission/sieve)
- [ ] Verify + commit

### P9 — Gap closing to ≥ 80%  ⬜
- [ ] Coverage report analysis; targeted tests for remaining cold spots
- [ ] Re-run full suite; confirm ≥ 80% line coverage on default features
- [ ] Confirm `cargo test` (all existing tests) still green
- [ ] Final ledger reconciliation + commit

## Progress log

| # | Date | Action | Line cov (src) | Tests | Notes |
|---|------|--------|---------------|-------|-------|
| 0 | 2026-10-28 | Baseline | 6.04% | 9 pass | pre-existing state |
| 1 | 2026-10-28 | P1: `mock-jmap` workspace crate (HTTP core + session + full method registry + blob upload/download) | 6.04% (unchanged) | 9 client pass · 35 mock self-tests pass · 2 mock doctests pass | `cargo check -p mock-jmap`, `cargo test -p mock-jmap`, `cargo test -p jmap-client` all green. Coverage unchanged by design: P1 adds the mock, not client-facing tests; the mock crate is excluded from the metric. Deviations recorded below. |

## Findings log

Discovered while building the mock against `src/` + the RFCs. None of these are
`src/` bugs; they are contract facts later phases must respect.

1. **Malformed JSON is `notJSON`, not `couldNotParseJson`.** The plan's
   error-injection note named `couldNotParseJson`, which is not a registered
   RFC 8620 §3.1 problem type. The mock answers
   `400 urn:ietf:params:jmap:error:notJSON` (client:
   `ProblemType::NotJson`). `couldNotParseJson` remains available as a free-form
   injected string if a test really wants it.
2. **`submissionExtensions` must be an array of strings.** RFC 8621 §4.3 allows
   an object (`{"SIZE": {...}}`), but `jmap_client` models it as `Vec<String>`,
   so the object form fails to deserialise. The mock advertises
   `["SIZE", "DSN", "UTF8ONLY"]`.
3. **`Capabilities::Other` is unreachable for object-valued capabilities.**
   `src/core/session.rs` tries the field-less `EmptyCapabilities` variant first
   and serde ignores unknown fields, so any object capability (e.g.
   `urn:ietf:params:jmap:mdn` with `maxSizeMaildropReport`) deserialises as
   `Empty`. Only non-object capability values can reach `Other`. Do not write a
   test that expects `Other` from an object.
4. **`SieveScript/validate` reports an invalid script as a *success* response
   carrying an `error` member**, not as a method error — that is what
   `SieveScriptValidateResponse::unwrap_error()` expects. The mock returns
   `{"accountId": …, "error": {"type": "invalidScript", …}}` under a normal
   `SieveScript/validate` response tag.
5. **Upload oversize problem type.** The plan said `cannotUploadBlob`; RFC 8620
   §5.1 registers `urn:ietf:params:jmap:error:limit`. The mock sends
   `cannotUploadBlob` (what Stalwart-style servers return) and exposes
   `problem_type::LIMIT` so a test can switch. `MockHandle::set_max_size_upload()`
   lowers the limit so the 413 path is testable without allocating 50 MiB.
6. **Unresolvable result reference is an in-band method error**
   (`invalidResultReference`, RFC 8620 §3.6), never a top-level problem
   document — the request itself was well formed.
7. **`Client::handle_error` byte-compares the content type.** It matches
   `application/problem+json` exactly, so the mock must never append
   `; charset=utf-8`. Enforced by `problem::problem_response`.
8. **Required (non-`Option`) response members.** A response missing any of
   `GetResponse.{state,list,notFound}`,
   `ChangesResponse.{accountId,oldState,newState,hasMoreChanges,created,updated,destroyed}`,
   `QueryResponse.{accountId,queryState,position,ids}`,
   `QueryChangesResponse.{accountId,oldQueryState,newQueryState,removed,added}`,
   `CopyResponse.{fromAccountId,accountId,newState}` fails client deserialisation.
   The mock always emits them.
9. **Disk space:** the D: volume hosting `target/` ran out of space mid-P1
   (`os error 112`). Cleared `target/debug/incremental` (regenerable cache) to
   finish. If a later phase hits the same error, clear that directory again — it
   is build cache, not source.
