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
- Only `src/` counts. The mock crate and `tests/` files are excluded:
  `cargo llvm-cov --all-targets --ignore-filename-regex "mock-jmap|tests" --summary-only`
  (exact command to be validated in P2 and recorded here).
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

### P1 — Mock crate: HTTP core  ⬜
- [ ] Workspace: add `mock-jmap` member + dev-dependency; `cargo check` green
- [ ] HTTP server skeleton: routing, request model, response writer, auth check
- [ ] Session resource (all capability blocks incl. websocket/sieve)
- [ ] JMAP POST dispatch: `using` validation, unknownCapability error
- [ ] Method handler registry + canned RFC-compliant responses for:
      Core/echo, Mailbox/get+changes+query+queryChanges+set,
      Thread/get+changes, Email/get+changes+query+queryChanges+set+copy+import+parse,
      SearchSnippet/get, Identity/get+set, EmailSubmission/get+query+queryChanges+set,
      VacationResponse/get+set, SieveScript/get+set+query+validate,
      Principal/get+changes+query+queryChanges+set, Blob/copy,
      PushSubscription/get+set
- [ ] Result-reference resolution (`/list`, `/list/*/id`, `/newState`, ...)
- [ ] createdIds / `#placeholder` round-trip
- [ ] Problem-type error injection + 401 unauthorized
- [ ] Mock self-tests (mock answers raw HTTP correctly)
- [ ] Orchestrator verifies: `cargo test -p jmap-client` still green; commit

### P2 — Mock crate: blob + EventSource + WebSocket  ⬜
- [ ] Upload endpoint (RFC 8620 §5.1) incl. size/type edge cases
- [ ] Download endpoint (RFC 8620 §5.2) incl. accept-type handling
- [ ] EventSource stream (chunked, multi-event, ping keepalives, Last-Event-ID)
- [ ] WebSocket (RFC 8887): handshake w/ `jmap` subprotocol, request/response
      correlation by requestId, push frames, RequestError frame, ping/pong
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

## Findings log

(bugs / RFC deviations discovered — none yet)
