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

## Mock alternatives survey (2026-10-28)

Researched before committing further to the hand-rolled mock:

- **wiremock** — generic request→response matcher (used by `jmap-base-client`
  with hand-written fixtures). Cannot do: result-reference resolution within a
  request, createdIds, advancing change history, WebSocket, SSE streaming.
  Hand-written response JSON per test mirrors client assumptions instead of
  checking them against the RFCs.
- **mejl-fake 0.2.0** (MIT OR Apache-2.0, published 2 days ago, ~10.7k LoC,
  axum) — closest existing option: RFC-written, result refs, creation ids,
  change logs, EventSource, blob up/download, fault injection. Rejected as
  primary because it is Fastmail-shaped: no Sieve draft, no Principal/*,
  no Blob/copy, no RFC 8887 WebSocket; session at `/jmap/session` (client
  hardcodes `/.well-known/jmap`); couples to its own `mejl-jmap` transport
  types; effectively unproven (1 version). Patterns worth borrowing:
  `mutate_from_elsewhere()` for driving push tests, runtime `Quirks`
  fault switches.
- **crate-jmap / jmap-server kit** — server framework; its test jig is
  `publish = false`, not consumable.
- **saehrimnir** — binary-only fixture mock, JMAP read-path only, no WS.

Decision: keep `mock-jmap` (workspace crate). It is the only option covering
the client's full surface (all 38 Method-enum methods + WS + EventSource +
blob + sieve draft + principals) with RFC semantics rather than canned JSON.

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

### P2 — Mock crate: blob + EventSource + WebSocket  ✅
- [x] Upload endpoint (RFC 8620 §5.1) incl. size/type edge cases — *done in P1*
- [x] Download endpoint (RFC 8620 §5.2) incl. accept-type handling — *done in P1*
- [x] EventSource stream (chunked, multi-event, ping keepalives, Last-Event-ID)
      — `mock-jmap/src/eventsource.rs`. `GET /eventSource/…?types=&ping=&closeafter=`:
      auth required (else 401 problem+json), 200 `text/event-stream`, chunked,
      flushed per event, `id:` on every state change, `types` filter with `*`
      meaning all, `ping` keepalive frames, `closeafter=state`, and
      `Last-Event-ID` resume out of the push history.
- [x] WebSocket (RFC 8887): handshake w/ `jmap` subprotocol, request/response
      correlation by requestId, push frames, RequestError frame, ping/pong
      — `mock-jmap/src/websocket.rs` on `tokio-tungstenite 0.29`. `methodCalls`
      run through the *same* `dispatch::dispatch_request` as `POST /jmap`, so
      the two transports cannot drift.
- [x] Mock control API to drive pushes while streams are open — `PushBus` plus
      `MockHandle::{push_event, trigger_state_change, broadcast_state_change,
      mutate_from_elsewhere, push_email, push_email_of, push_subscriber_count,
      last_push_id}` (see “Mock push API” below).
- [x] Mock self-tests (streams answer raw bytes correctly) — 41 tests in
      `mock-jmap/tests/p2_streams.rs`
- [x] Orchestrator verifies + commit

#### Mock push API (for P3+ / P7)

Everything a test needs to make the mock “change state from elsewhere” while a
stream is open, and the one ordering rule that makes it deterministic.

| Call | Effect |
|------|--------|
| `handle().trigger_state_change("Mailbox", "10")` | publishes a `StateChange` for one data type of the primary account |
| `handle().broadcast_state_change(BTreeMap<account, BTreeMap<type, state>>)` | publishes an arbitrary multi-account/multi-type `StateChange` |
| `handle().mutate_from_elsewhere(\|store\| { …; vec!["Mailbox"] })` | runs a closure against the store, then publishes the returned types at their new state — what a real `*/set` does |
| `handle().push_email(json)` / `push_email_of(id)` | publishes an `EmailPush` (of an existing fixture email) |
| `handle().push_subscriber_count()` / `last_push_id()` | how many streams are attached; the current high-water event id |

`pushState` **is** the push event id: a single monotonic counter in `PushBus`.
So `Last-Event-ID: N` on SSE and `WebSocketPushEnable{pushState:"N"}` on WS
resume from exactly the same point, and neither can replay a notification twice.

**Ordering rule.** `WebSocketPushEnable`/`WebSocketPushDisable` get no reply of
their own, so a push published straight after sending one can overtake it and be
dropped before the session is switched on. Synchronise through the socket first:
send any `Core/echo` request after the enable and wait for its `Response` — the
socket is ordered, so the enable has been handled. `p2_streams.rs` wraps this as
`enable_push(ws, dataTypes, pushState)` / `disable_push(ws)`; P7 should reuse the
pattern. SSE needs no such handshake: the stream is live the moment the 200
arrives, and `wait_for_subscribers(handle, n)` makes that observable.

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
| 2 | 2026-10-28 | P2: `mock-jmap` EventSource (SSE) + RFC 8887 WebSocket legs, `PushBus` push model, mock control API for driving pushes against open streams | 6.04% (unchanged) | 9 client pass · 41 P2 stream tests pass · 35 P1 mock self-tests pass · 4 mock doctests pass | `cargo test -p mock-jmap`, `cargo test -p jmap-client`, `cargo llvm-cov --all-targets --ignore-filename-regex "mock-jmap\|tests" --summary-only` all green; the summary lists only `src/` rows. Coverage unchanged by design (P2 adds mock capability, not client-facing tests). Deviations recorded below. |

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
10. **A WebSocket frame with no `@type` is `notRequest`, not a close.** RFC 8887
    §4.3.2 makes `@type` mandatory on every frame. The mock answers
    `urn:ietf:params:jmap:error:notRequest` for a missing `@type` and for any
    value other than `Request` / `WebSocketPushEnable` / `WebSocketPushDisable`,
    and keeps the socket open.
11. **A WebSocket frame that is not JSON is `notJSON`.** RFC 8887 §4.4 spells it
    `urn:ietf:params:jmap:error:notJSON` (the same type as the HTTP problem in
    finding 1), delivered as a `RequestError` *frame* — the connection survives
    and answers the next request.
12. **An unknown capability over WS is a frame, not a close.** `using` an
    unregistered capability yields
    `urn:ietf:params:jmap:error:unknownCapability` as a `RequestError` frame;
    the same dispatcher that serves `POST /jmap` produces it, so HTTP and WS
    cannot disagree.
13. **`sessionState` is always present on a WS `Response`.** The RFC 8887 §4.4
    example omits it, but RFC 8620 §3.4 requires it and
    `jmap_client::WebSocketResponse` deserialises it as a non-optional `String`.
14. **Binary WebSocket frames are ignored.** RFC 8887 §4.3.1 allows either
    ignoring them or closing with 1003. The mock ignores, so a test can assert
    the next *text* request still works rather than asserting a close code.
15. **SSE event name is `StateChange`, not the RFC's `state`.** RFC 8620 §7.1
    uses `event: state`, but `jmap_client`'s parser maps any name that is not
    `ping` or `calendarAlert` to `EventType::State`, and the mock's own tests
    read the name literally. `StateChange` / `EmailPush` are what the mock emits.
16. **`EmailPush` over SSE breaks `jmap_client`.** `filter_notification()` in
    `src/event_source/parser.rs` returns
    `Error::Internal("Unexpected PushObject variant")` for anything that is not
    `StateChange` or `CalendarAlert`. The mock still emits `EmailPush` over SSE
    (it is RFC-correct); a P7 test that drives EmailPush through the *client's*
    SSE parser will fail for this reason. Use the WebSocket leg for `EmailPush`
    client assertions — it deserialises fine there.
17. **No unconditional initial `StateChange` on SSE connect.** Some servers send
    a hello state; the mock does not, because the client's parser dispatches an
    event on the first blank line and would then try to parse an empty payload.
    Instead the mock writes a liveness *comment*
    (`: mock-jmap event stream open`) with no trailing blank line, so a test can
    tell "connected" from "waiting" without a spurious event.
18. **`closeafter=state` fires only on a real `StateChange`** — never on the
    liveness comment and never on a `ping` keepalive.
19. **`ping` accepts fractional seconds (floor 20 ms).** RFC 8620 §7.1 intends
    an integer interval of at least 30 s. The mock accepts `ping=0.05` so a
    keepalive test finishes in milliseconds. Real servers may reject it; nothing
    in the client parses the value.
20. **Two mock-local problem types for the WS handshake**, not RFC-registered:
    `notWebSocketUpgrade` (a plain `GET /ws`) and `missingSubprotocol` (an
    upgrade that does not offer `jmap`). Both are `application/problem+json`,
    and both let a test assert the handshake was refused rather than upgraded.
21. **`Collection::create()` does not advance the collection's state.** A test
    that wants `state_str()` to move must call `record_change(created, updated,
    destroyed)` the way a `*/set` handler does. `mutate_from_elsewhere` exists
    precisely so tests do not have to remember this.
22. **Push delivery needs a synchronisation frame.** `WebSocketPushEnable` gets
    no reply, so a push published immediately after sending it can reach the
    mock *before* the enable is processed and be filtered out forever. Three P2
    tests were flaky until the test helper started sending a `Core/echo` after
    enable/disable and waiting for its `Response`. P7 must use the same pattern
    (see “Mock push API” above); `wait_for_subscribers()` alone is not enough,
    because it proves the socket is attached, not that push is enabled.
