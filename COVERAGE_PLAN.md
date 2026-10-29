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

### Coverage-surface analysis (2026-10-28)

Exact instrumented totals (llvm-cov, default features): **6953 lines** in `src/`
(80% = 5563 lines). Earlier "6.04%" figures were the Regions column; the Lines
column baseline is 7.05%. Surface by transport:

| Surface | Lines | wiremock-feasible |
|---|---|---|
| HTTP JMAP API (client, core, all 9 domain modules) | ~5990 | Yes — canned JSON + spying on request bodies |
| Blob upload/download (HTTP) | 170 | Yes |
| Pure logic (enums, errors, SSE parser, URL parsing) | ~270 | N/A — unit tests, no server |
| EventSource stream (templating, headers, parse loop) | 77 | Mostly (static `text/event-stream` body; loses mid-stream error/keepalive timing) |
| WebSocket (`client_ws.rs` + `send_ws` glue) | ~197 | **No — structurally impossible** |

≈ 97% of lines are wiremock-testable; ~3% (WS) is not.

### Mock strategy decision — HYBRID (user-approved 2026-10-28)

- **P3–P6 (HTTP domains): `wiremock`** drives the client. Canned responses are
  taken **verbatim from RFC 8620/8621 example payloads wherever available** so
  the oracle is the RFC text, not our interpretation. Request shapes are
  asserted with wiremock matchers/spying. Static pairs suffice for
  result-refs/createdIds because the *client* only serializes references and
  parses `createdIds` — resolution is server-side.
- **P7 (push streams): `mock-jmap`** — the only wiremock-impossible surface
  (WebSocket + live SSE). Its HTTP dispatch stays alive as the WS dispatcher.
- **P8/P9: in-crate unit tests** for pure logic (no server involved).
- Hand-rolled *trusted* mock surface shrinks from ~6k LoC to the WS/SSE
  modules (~1k LoC, 80 self-tests).
- Optional later risk-closer: differential smoke subset against a real
  Stalwart server (Docker) to validate mock/RFC agreement.

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
| `handle().keepalive_count()` | how many `ping` frames the SSE leg has written — the only way to observe a keepalive, since the client's parser drops them (Finding #81) |

`pushState` **is** the push event id: a single monotonic counter in `PushBus`.
So `Last-Event-ID: N` on SSE and `WebSocketPushEnable{pushState:"N"}` on WS
resume from exactly the same point, and neither can replay a notification twice.

**Ordering rule.** `WebSocketPushEnable`/`WebSocketPushDisable` get no reply of
their own, so a push published straight after sending one can overtake it and be
dropped before the session is switched on. Synchronise through the socket first:
send any request after the enable and wait for its `Response` — the socket is
ordered, so the enable has been handled. (The RFC's natural probe, `Core/echo`,
is *not* sendable: `core::request::Arguments` has no `Echo` variant — Finding
#84. `tests/websocket.rs` uses a `Mailbox/get`.) `p2_streams.rs` wraps this as
`enable_push(ws, dataTypes, pushState)` / `disable_push(ws)`; P7 reuses the
pattern as its own `sync(client, stream)` helper. SSE needs no such handshake:
the stream is live the moment the 200 arrives, and `wait_for_subscribers(handle, n)`
makes that observable.

#### Wiremock fixture API (for P3–P6)

`tests/common/mod.rs` is the shared HTTP fixture layer. Every P3–P6 test binary
includes it with `mod common;`; unused helpers are allowed
(`#![allow(dead_code)]`), so each phase adds only what it mounts.

| Helper | Purpose |
|---|---|
| `ACCOUNT_ID` / `USERNAME` / `SESSION_STATE` / `API_PATH` / `SESSION_PATH` | the constants every fixture and assertion shares (`u33084183`, `test@example.org`, `75128aab4b1b`, `/jmap`, `/.well-known/jmap`) |
| `DEFAULT_CAPABILITIES` | the URIs the default session advertises — **sieve deliberately absent** so filtering is observable |
| `session_json(server)` / `session_json_with_sieve(server)` | the Session resource body, with and without `urn:ietf:params:jmap:sieve` |
| `mount_session` / `mount_session_json` / `mount_session_template` | answer `GET /.well-known/jmap` (template variant serves a 401 problem where the session should be) |
| `mount_sieve_session` | answer with the sieve-advertising session |
| `connected_client(server)` / `connected_client_json(server, session)` | mount a session **and** run `Client::new().credentials((USERNAME,"secret")).connect()` |
| `jmap_response(method_responses, session_state)` | `200 application/json` `{"methodResponses":[…],"sessionState":"…"}` |
| `jmap_response_with_created_ids(…, created_ids, …)` | the same plus the optional `createdIds` member |
| `problem_response(status, body)` | RFC 7807 body with content type `application/problem+json` and **no** `; charset=utf-8` (Finding 7) |
| `mount_jmap(server, template)` / `mount_jmap_scoped(server, template) -> MockGuard` | answer `POST /jmap` for the server's life / until the guard drops |
| `request_bodies` / `single_request_body` | spy on every recorded request body (single variant asserts exactly one) |
| `jmap_request_bodies` / `single_jmap_request` | spy on `POST /jmap` only — use once the session GET would shift indices |
| `expected_basic_auth(user, pass)` | the exact `Authorization` value the client builds for Basic auth |

A method response fixture is an **Invocation**, `["Name", {args}, "callId"]`
(RFC 8620 §3.2). `tests/core_methods.rs` wraps bodies with a local `inv()` and
`method_error()`; P4–P6 should do the same rather than passing a bare arguments
object — `TaggedMethodResponse` deserialises from the array, never the object.

**Hermeticity rule:** each test owns its `MockServer::start()` and mounts only
what it needs. An unmatched request gets wiremock's 404, which surfaces as
`jmap_client::Error::Server` — a loud failure instead of a hang.

### P3 — Integration tests: core  ✅
- [x] tests/core_methods.rs: session fetch, multi-method requests, references,
      createdIds, capability filtering, error paths (401, problem+json,
      method error, couldNotParseJson) — **28 tests**, wiremock-backed, plus the
      shared fixture module `tests/common/mod.rs` (see “Wiremock fixture API”
      above). Note: the plan named `couldNotParseJson`; per Finding 1 the
      client-side counterpart of a malformed body is `Error::Parse`, which is
      what is asserted.
- [x] tests/changes_query.rs: changes/query/queryChanges across types —
      **23 tests**, wiremock-backed:
      * Changes: `Mailbox/changes`, `Email/changes`, `Thread/changes`
        round-trips (RFC 8620 §5.2 / RFC 8621 §2.2, §3.2, §4.3), paging
        through the RFC's `B → B-D-E → E` intermediate states,
        `updatedProperties` present and `null`, `cannotCalculateChanges`,
        and a missing-required-member parse failure (Finding 8).
      * Query: `Mailbox/query` and `Email/query` with AND/OR filter
        operators, `Comparator`s (ascending/descending + `collation`),
        `position`/`limit`/`calculateTotal`, Mailbox `sortAsTree`/
        `filterAsTree`, Email `collapseThreads` (RFC 8620 §5.5,
        RFC 8621 §2.3, §4.4); `unsupportedSort` / `unsupportedFilter`
        method errors; `canCalculateChanges=false` and its absent-member
        default.
      * QueryChanges: `Mailbox/queryChanges` and `Email/queryChanges`
        round-trips with `sinceQueryState`/`upToId`/`maxChanges`/
        `calculateTotal` and `AddedItem{id,index}` (RFC 8620 §5.6,
        RFC 8621 §2.4, §4.5); `tooManyChanges` and
        `cannotCalculateChanges` errors.
      * Cross-cutting: the `src/lib.rs` doc-example chain
        (`changes_mailbox` → `updated_reference`/`updated_properties_reference`
        → `get_mailbox().ids_ref().properties_ref()`), the full RFC 8621
        §2.6 three-call changes→get chain, the RFC 8621 §4.10
        query→get `#ids: /ids` chain, and a mixed request where one call
        errors and the other still unwraps.
      Section numbering note: the standard methods live at RFC 8620
      **§5.2** (`/changes`), **§5.5** (`/query`) and **§5.6**
      (`/queryChanges`) — not §5.1 as the task brief assumed.
- [x] Verify + commit — P3 closed by this increment (28 + 23 tests green).

### P3-followup — mock-jmap WS handshake flake (fix before P7) ✅

- Fixed in P7 (Finding #78). `websocket_refuses_a_connection_that_offers_no_subprotocol`
  intermittently read an empty rejection body.
  **Root cause, read out of tungstenite 0.29
  `src/handshake/client.rs`:** on a refused handshake the client stores
  `*e.body_mut() = Some(tail)` — the bytes still sitting in the read buffer at
  the moment the response *head* parsed. The mock wrote the head and the body
  with two separate `write_all` calls, so they could leave as two TCP segments;
  when the head parsed before the body arrived the "tail" was empty and the
  rejection body came back `""`. Not a flush race in the usual sense: the bytes
  were flushed, they just arrived later than the parse.
  **Fix:** `mock-jmap/src/http.rs::write_response` is now generic over
  `S: AsyncWrite + Unpin` and composes status line + headers + body into **one**
  `write_all`, followed by a single `flush` and then the shutdown. Locked in by
  the unit test `http::tests::write_response_emits_head_and_body_in_one_write`
  (a `RecordingSink` that fails if the response is written in more than one
  call).
  **Evidence:** 8 consecutive full `cargo test -p mock-jmap` runs green
  (1 lib + 41 `p2_streams` + 35 `self_tests` + 4 doctests) before the P7 test
  files existed, plus 5 more after — **13 full-suite runs, 0 flakes** — and 20
  targeted `--test p2_streams websocket_refuses` runs, 0 failures. Against the
  4-in-9 / 2-in-5 rate P6 recorded, the flake is gone.
  The same single-write reasoning was applied to the SSE `write_chunk` (see
  Finding #79).

### P4 — Integration tests: mail domain  ✅
- [x] tests/mailbox.rs: get/set/create/destroy/update/query + helpers
      (26 tests — `Client::mailbox_{get,get_with_role,create,rename,move,
      set_role,set_sort_order,subscribe,unsubscribe,set_rights,acl,
      update,destroy,has_email,has_child,query,changes}` plus the
      `Request::{get,changes,query,query_changes,set}_mailbox` builders,
      `Role` (de)serialisation and `Property`/`ACL` Display).
- [x] tests/email.rs: get/set/query/import/parse/copy + keywords + helpers
      (37 tests — `Client::email_{get,get_body_value,import,
      import_account,set_mailbox,set_mailboxes,set_keyword,set_keywords,
      update,destroy,copy,parse,query,search_snippet_get}` plus the
      `Request::{get,changes,query,query_changes,set,copy,import,parse}_email`
      builders, `EmailBodyPart`/`EmailValue` builders, header forms and the
      `header:…:as<Form>` alias table).
- [x] tests/thread.rs + tests/search_snippet.rs
      (11 + 11 tests — `Client::thread_get`,
      `Request::{get_thread,send_get_thread,changes_thread,
      send_changes_thread}`, `thread::Property`; `Client::search_snippet_get`,
      `Request::{get_search_snippet,send_get_search_snippet}`,
      `SearchSnippetGetRequest::{email_id,email_ids,email_ids_ref,filter}`).
- [x] Verify + commit — P4 closed by this increment (85 new tests green).

### P5 — Integration tests: submission / identity / vacation  ✅
- [x] tests/email_submission.rs — **27 tests**, wiremock-backed:
      `Client::email_submission_{get,create,create_envelope,change_status,
      destroy,query,changes}` plus the `Request::{get,set,query,
      query_changes,changes}_email_submission` builders and
      `TaggedMethodResponse::unwrap_{get,set,query,query_changes,changes}
      _email_submission`.
      * Get: full RFC 8621 §7.1 object round-trip (envelope, `sendAt`,
        `undoStatus`, per-recipient `deliveryStatus`, `dsnBlobIds`,
        `mdnBlobIds`), property-subset request shape, `notFound` → `None`,
        the Finding-#48 `take_list().pop()` trap, and a missing-required-
        member parse failure (Finding #8).
      * Create: creation-id round-trip (`c0`), `create_envelope` putting
        `mailFrom`/`rcptTo` on the wire, and the three RFC example failures
        `forbiddenTo` / `invalidProperties` / `noRecipients`.
      * Set: `change_status` cancel of a `pending` submission,
        `cannotUnsend`, `destroy` + `notDestroyed`, and a raw
        `set_email_submission` carrying `onSuccessUpdateEmail` /
        `onSuccessDestroyEmail`.
      * Query: filter + sort round-trip, one AND group with **every**
        `FilterCondition` of §7.3, `unsupportedSort`, and the empty-filter
        default; `queryChanges` with `AddedItem`/`removed` (§7.4).
      * Changes: every member round-trip (§7.2) and `cannotCalculateChanges`.
      * Enums: exhaustive `DeliveryStatus`/`Delivered`/`Displayed` and
        `UndoStatus` (de)serialisation, `Property` Display.
      * Chain: `EmailSubmission/query` → `Email/get` by `#ids: /ids`
        result reference.
      Note: the box said "delivery time, undo, bounce". There is no
      delivery-timestamp member in the RFC (Finding #53); delivery state is
      `deliveryStatus[recipient] = {smtpReply, delivered, displayed}` and a
      bounce is `delivered: "no"`, so the tests assert that shape.
- [x] tests/identity.rs — **22 tests**, wiremock-backed:
      `Client::identity_{get,create,destroy,changes}` plus
      `Request::{get,set,changes}_identity`.
      * Get: RFC §6.1 record round-trip, the `*@domain` wildcard record with
        empty signatures, `notFound` → `None`, the `pop()` trap, property
        subset, missing-required-member parse error.
      * Create: creation-id round-trip, `replyTo`/`bcc`/both signatures on
        the wire, `forbiddenFrom`, `invalidProperties`.
      * Update: html-signature patch, updated-object echo,
        `notUpdated.invalidProperties`, and a single call that creates,
        updates and destroys together.
      * Destroy: success, `forbidden` when `mayDelete` is false, and
        `ifInState` state mismatch.
      * Changes: every member (§6.2) and `cannotCalculateChanges`.
      * Cross-cutting: the mail-only `using` list (§1.3.4), `Property`
        Display, and a locally implemented RFC §6 *most likely identity*
        rule (exact address preferred over the wildcard) that documents the
        missing crate helper (Finding #58).
- [x] tests/vacation_response.rs — **18 tests**, wiremock-backed:
      `Client::vacation_response_{get,enable,disable,set_dates,create,
      destroy}` plus `Request::{get,set}_vacation_response`.
      * Get: enabled and disabled singleton, property subset, singleton
        missing → `None`, missing-required-member parse error.
      * Set: full enable patch, `enable` with an unset body member
        serialising as explicit `null`, `disable` → `isEnabled: false`,
        `set_dates` emitting both timestamps, `set_dates(…, None, None)`
        clearing both, `create` with a creation id, the `singleton`
        already-exists error, `invalidDate` for a reversed range,
        `ifInState` mismatch, `destroy` + `notDestroyed`.
      * Chain: `VacationResponse/get` then `VacationResponse/set` in one
        request, unwrapped per call id (Finding #49).
      * `Property` Display.
- [x] Verify + commit — P5 closed by this increment (67 new tests green).

### P6 — Integration tests: sieve / principal / push / blob  ✅
- [x] tests/sieve.rs — **22 tests**, wiremock-backed:
      `Client::sieve_script_{get,create,replace,rename,activate,deactivate,
      destroy,query,validate}` plus the `Request::{get,set,query}_sieve_script`
      builders and `unwrap_{get,set,query}_sieve_script`.
      * Get: draft-ietf-jmap-sieve-12 §2.1.1 record round-trip (`name`,
        `isActive`, `blobId`), property subset, `notFound` → `None`, the
        Finding-#48 `take_list().pop()` trap.
      * Create/replace: the two-leg flow — script text uploaded to
        `/upload/{ACCOUNT_ID}`, then `SieveScript/set` with the returned
        `blobId`; `onSuccessActivateScript` present and absent; `replace`
        re-activating by the *real* script id; the upload leg failing
        aborts the whole helper before any JMAP call is made.
      * Activate/deactivate/rename/destroy: single-member patches, the
        `alreadyExists` / `invalidProperties` / `forbidden` SetError paths,
        `notDestroyed`, `ifInState` mismatch.
      * Validate: `SieveScript/validate` round-trip with `errAtLine`,
        `errAtColumn`, `errDescription`, and the ok-answer shape.
      * Query: filter + sort round-trip, omitted members with no criteria.
      * Wire names: every `SieveScriptProperty` Display/serde pair.
      Note: the box said "get/set/query/validate + script blob flow" — all
      four method families plus the upload leg are covered. `SieveScript`
      has no `order` member (Finding #68).
- [x] tests/principal.rs — **25 tests**, wiremock-backed:
      `Client::principal_{get,query,changes,destroy}` plus the five create
      helpers `individual_create` / `domain_create` / `domain_enable_dkim` /
      `list_create` / `group_create`, the seven patch helpers
      `principal_set_{name,secret,email,timezone,members,aliases,
      capabilities}`, and `Request::{get,set,query,query_changes,changes}
      _principal`.
      * Get: RFC 9670 §4.1 record round-trip, `isPersonal`/`isGroup`,
        `Type`/`ACL`/`Property` wire names, property subset, `notFound` →
        `None`, the `pop()` trap.
      * Create: minimal individual patch, `alreadyExists`, DKIM enable
        patching `secret` + `dkim` together, explicit `dkimExpiration:
        null`, list and group creates carrying `members`.
      * Set: each of the seven single-property patch helpers, `notUpdated`
        with `invalidProperties`, `ifInState`, destroy + `notDestroyed`.
      * Query: AND filter with every `PrincipalFilter` variant on the wire,
        sort, window args, omitted filter/sort.
      * Changes: created/updated/destroyed round-trip (§4.5) and
        `cannotCalculateChanges`.
- [x] tests/push_subscription.rs — **15 tests**, wiremock-backed:
      `Client::push_subscription_{get,create,verify,update_types,destroy}`
      plus `Request::{get,set}_push_subscription`.
      * Get: the RFC 8620 §7.2.1 example list, `types` per subscription,
        and the `Keys` `{p256-dh, as128gcm}` members decoded from URL-safe
        base64.
      * Create: the §7.2.3 example create with `onSuccessEnable`/`verify`,
        keys encoded with the URL-safe alphabet, and a rejected create
        (`tooManySubscriptions` / `invalidProperties`).
      * Verify: patching `verificationCode`, and the wrong-code rejection.
      * Update: `update_types` sending the type list, `update_types(.., None)`
        asking for every type.
      * Destroy: removal and `notDestroyed`.
      * Wire names: `PushSubscriptionProperty` Display/serde pairs, and the
        seven-member object shape pinned.
- [x] tests/blob.rs — **12 tests**, wiremock-backed:
      `Client::{upload,download,blob_copy}` plus `Request::copy_blob` and
      `CopyBlobResponse::{copied,not_copied}`. Routes are mounted on the
      blob endpoints (`/upload/{ACCOUNT_ID}`,
      `/download/{ACCOUNT_ID}/{blobId}/none?accept=…`), not `POST /jmap`.
      * Upload: metadata returned (`blobId`, `type`, `size`), the raw
        octets arriving byte-identical (Finding #7), the `Content-Type`
        actually sent, and an upload rejection surfacing as `Error::Http`.
      * Download: bytes round-trip, `application/octet-stream` accept
        header, a 404 blob.
      * Copy: `copied` map round-trip (`oldBlobId` → new `blobId`),
        `notCopied` with `notFound`, an id the server never reported being
        an internal error, and a multi-blob copy through the `Request::`
        builder.
- [x] Verify + commit — P6 closed by this increment (74 new tests green).

### P7 — Integration tests: push streams ✅

- [x] `tests/event_source.rs` — **11 tests** (9 `#[tokio::test]` + 2 plain),
      driven by `Client::event_source()` against the mock's real
      `GET /eventSource` leg (the wiremock layer cannot reach a server-pushed
      stream):
      * delivery — `StateChange` arrives with its SSE `id`, the changed account,
        the `{DataType → state}` map, `has_type`/`is_empty`, and the draining
        `account_changes()` semantics;
      * `types=` filter — an Email change published to a `Mailbox`-only stream
        is never delivered (proved by asserting the *next* wanted event's id,
        not by sleeping for silence);
      * `closeafter=state` — the closing event is the state change itself, then
        the stream ends (`next() → None`) and the mock's subscriber drops to 0;
      * `ping=1` — the keepalive is observed through the mock's
        `keepalive_count()` (the client's parser swallows pings, Finding #81)
        and the stream still delivers afterwards;
      * `Last-Event-ID` resume — replaying from `"1"` yields exactly id 2 then
        live 3 with no gap or overlap; a token ahead of the bus (`"99"`)
        replays nothing;
      * URL templating — the query is asserted verbatim:
        `types=Mailbox,Email&closeafter=state&ping=45` and the default
        `types=*&closeafter=no&ping=0` (Finding #82);
      * Finding #16 documented from the client side — `EmailPush` over SSE
        surfaces as `Error::Internal("Unexpected PushObject variant: …")` and
        **the stream survives** and keeps delivering;
      * two sync tests over `Changes` built by hand (accessor round-trip,
        empty-value semantics, Finding #83).
- [x] `tests/websocket.rs` — **15 tests**, driven by `Client::connect_ws()` /
      `send_ws()` / `enable_push_ws()` / `disable_push_ws()` / `ws_ping()`
      against the mock's RFC 8887 leg:
      * request/response — `requestId` correlation, two sequential requests
        with distinct ids, `sessionState` present on every `Response`;
      * push — `enable_push_ws` state change delivery, `dataTypes` filtering,
        `disable_push_ws` stopping delivery, a socket that never enabled push
        receiving nothing, `EmailPush` via `push_email_of("em_1")` and via an
        explicit `push_email` payload (the RFC 8621 notification the SSE leg
        cannot carry);
      * `ws_ping` round-trip without breaking the stream;
      * errors — a `RequestError` frame (forced with the unadvertised
        `URI::Contacts` via `Request::add_capability`) surfaces as
        `Error::Problem` / `unknownCapability` **and the socket stays usable**;
        a method-level error inside a `Response` is contrasted with it;
      * not-connected errors for `send_ws`/`enable_push_ws`/`disable_push_ws`/
        `ws_ping` (`"Websocket stream not set."`), and `connect_ws` against a
        session with the websocket capability stripped out
        (`"JMAP server does not advertise any websocket capabilities."`);
      * the handshake request is logged on `/ws`.
      Every enable/disable is followed by the echo-synchronisation frame of
      Finding #22 (`WebSocketPushEnable`/`Disable` get no reply of their own),
      so no push can overtake the switch.
- [x] Verify + commit — P7 closed by this increment (26 new tests green).
      `cargo test -p jmap-client` green ×3, `cargo test -p mock-jmap` green ×5
      on top of the 8 earlier runs, `cargo clippy -p jmap-client --all-targets`
      clean for `tests/` (the only warnings are the 7 pre-existing
      `mock-jmap` lib ones plus 5 pre-existing in P4/P6 test files), coverage
      summary lists only `src/` rows. **TOTAL 75.64% → 80.25% lines.**
      The P3-followup flake is fixed (box above). New contract facts recorded
      as Findings 78–84.

### P8 — Unit tests: pure logic in src  ✅ (scope re-cut, see note)

The 80% goal was already met at P7 (80.25%), so P8 was re-cut from "the
coldest modules per the original boxes" to "the coldest *pure* logic the
integration batches structurally cannot reach" — read off the P7 delta, which
left `core/response.rs` at 31.70% and `email/query.rs` at 21.38% because
their surface is per-method-variant and per-filter-condition code.

- [x] `lib.rs`: `URI::as_ref` + `Display for DataType` verified against each
      variant's own `#[serde(rename)]` (not a hand-copied table), `Display`
      for every `Error` variant, and the `From<T> for Error` impls.
      **49.32% → 100.00%**
- [x] `core/error.rs`: `Display for ProblemDetails` over the RFC 8620
      §3.6.1.1 problem examples (all four `JMAPError` URIs + `Other`),
      optional `status`/`title`/`detail` composition, `Display for
      MethodError`, `SetError` shapes. **72.50% → 99.58%**
- [x] `core/response.rs`: the `Response` envelope accessors, the
      `TaggedMethodResponse::unwrap_*` family (all 37 accessors driven
      through both the method-error arm and the type-mismatch arm via one
      macro), and the `visit_seq` guards for a malformed envelope.
      **31.70% → 99.61%**
- [x] `email/query.rs`: all 24 `Filter` variants serialising under their own
      one-key form (RFC 8621 §4.4.1), `Comparator` property names
      (§4.4.2), keyword-carrying comparators, header-filter arity, and
      `isAscending`/`collation` mutation. **21.38% → 100.00%**
- [x] `core/mod.rs`: `RequestParams::new` keeps what it is given.
      **0.00% → 100.00%**
- [x] Verify + commit — 80 in-crate unit tests green (up from 7), full client
      suite 385 green, mock 81 green, clippy clean in all five files,
      **TOTAL 80.25% → 90.19% lines**.

**Not done from the original P8 boxes** (deliberately re-scoped, still open
for P9): `core/session.rs` `URLPart::parse` edge cases and capability
accessors (70.72%, mostly already driven by the integration batches), and
filters/comparators for mailbox / principal / submission / sieve — only the
`email` one was cold enough to justify a unit sweep; the others are at
78–94% from P4–P6.

All tests are pure: no `MockServer`, no sockets, no `async`. Every fixture is
RFC-verbatim text with the section cited in its doc comment.

### P9 — Gap closing to ≥ 80%  ✅

The 80% goal was cleared at P7 and pushed to 90.19% by P8, so P9 was read
off the P8 coverage report as "every `src/` file still under 100% whose
remaining gap is *pure* logic" — the twelve modules where the integration
batches had driven the happy path but not the alternate builder arms, the
`Display` tables, or the serde visitor edges.

- [x] Coverage report analysis — re-measured the P8 end state
      (**88.84%** regions / **86.57%** functions / **90.19%** lines,
      8285 lines) by stashing the increment and re-running the same
      `llvm-cov` command, so every delta below is against a verified
      baseline rather than a transcribed one. The 12 targets were picked
      by missed-line count among files with no network dependency;
      `client.rs` (52 missed) and `client_ws.rs` (59 missed) were left
      alone as out of scope for in-crate unit tests.
- [x] Targeted tests for remaining cold spots — 12 files, 115 new
      `#[cfg(test)] mod tests` unit tests, all appended at end of file:
      `core/session.rs` (13), `core/set.rs` (19), `email/get.rs` (11),
      `email/mod.rs` (15), `email/parse.rs` (6), `email/set.rs` (14),
      `email_submission/set.rs` (7), `mailbox/mod.rs` (7),
      `mailbox/set.rs` (9), `principal/set.rs` (9), `sieve/get.rs` (3),
      `sieve/query.rs` (2). No `MockServer`, no sockets, no `async`;
      every fixture is RFC-verbatim text with the section cited in its
      doc comment (8620 §5.3/§6.1, 8621 §4.1/§4.1.2/§4.1.3/§4.1.4/§5.2/
      §6.1, draft-ietf-jmap-sieve-12 §5.1/§5.3/§5.4).
- [x] Re-run full suite; confirm ≥ 80% line coverage on default features —
      **96.30%** lines, 16.30 pp above the goal.
- [x] Confirm `cargo test` (all existing tests) still green — 500 client
      tests pass (195 lib + 305 integration), 81 `mock-jmap` tests pass,
      `cargo clippy -p jmap-client --all-targets` emits **zero** warnings
      in `src/`, and `git diff --numstat` shows **3195 insertions / 0
      deletions** across the 12 files: no existing non-test line was
      touched.
- [x] Final ledger reconciliation + commit — per-file delta below; new
      contract facts recorded as Findings 90–97.

Six of the twelve targets reached **100.00%** line coverage
(`email/get.rs`, `email/mod.rs`, `email/parse.rs`, `mailbox/mod.rs`,
`sieve/get.rs`, `sieve/query.rs`); the other six sit at ≥ 99.00%. The
residual 11 uncovered lines in the touched files are all structurally
unreachable in a green run — see the delta table notes and Findings
94–96.

**Not touched in P9** (still cold, all network-bound or per-method-variant
and already driven by the integration batches): `client_ws.rs` 69.27%,
`event_source/parser.rs` 86.98%, `sieve/helpers.rs` 91.05% (17 missed
lines), `principal/helpers.rs` 96.12%, `email/helpers.rs` 97.74%,
`core/copy.rs` 78.95%, `core/get.rs` 82.43%, `email/import.rs` 89.01%.
Closing these needs a transport-level seam, not more unit tests.

All tests are pure: no `MockServer`, no sockets, no `async`.

## Progress log

| # | Date | Action | Line cov (src) | Tests | Notes |
|---|------|--------|---------------|-------|-------|
| 0 | 2026-10-28 | Baseline | 6.04% | 9 pass | pre-existing state |
| 1 | 2026-10-28 | P1: `mock-jmap` workspace crate (HTTP core + session + full method registry + blob upload/download) | 6.04% (unchanged) | 9 client pass · 35 mock self-tests pass · 2 mock doctests pass | `cargo check -p mock-jmap`, `cargo test -p mock-jmap`, `cargo test -p jmap-client` all green. Coverage unchanged by design: P1 adds the mock, not client-facing tests; the mock crate is excluded from the metric. Deviations recorded below. |
| 2 | 2026-10-28 | P2: `mock-jmap` EventSource (SSE) + RFC 8887 WebSocket legs, `PushBus` push model, mock control API for driving pushes against open streams | 6.04% (unchanged) | 9 client pass · 41 P2 stream tests pass · 35 P1 mock self-tests pass · 4 mock doctests pass | `cargo test -p mock-jmap`, `cargo test -p jmap-client`, `cargo llvm-cov --all-targets --ignore-filename-regex "mock-jmap\|tests" --summary-only` all green; the summary lists only `src/` rows. Coverage unchanged by design (P2 adds mock capability, not client-facing tests). Deviations recorded below. |
| 3 | 2026-10-28 | P3 (part 1): shared wiremock fixture layer `tests/common/mod.rs` + `tests/core_methods.rs` — 28 core protocol tests (session fetch, request/response round-trips, call ids, result references, createdIds, capability filtering, problem/method/parse error paths, session staleness + refresh, account/timeout/headers) | **16.67%** lines (6953 instr., 1159 covered) — **+9.90 pp / +688 lines** over the 6.77% re-measured baseline | **37 client pass** (7 lib unit + 2 crypto-provider + 28 new) · mock-jmap 41+35+4 pass | Verified: `cargo test -p jmap-client` green, `cargo test -p mock-jmap` green, `cargo clippy -p jmap-client --all-targets` clean for `tests/` (7 warnings, all pre-existing in `mock-jmap`), coverage summary lists only `src/` rows. Baseline re-measured at **6.77%** with the P3 files removed (the ledger's 7.05% predates the P1/P2 branch state); deltas below are against the re-measured figure. `tests/changes_query.rs` (P3 box 2) is still open. |
| 4 | 2026-10-28 | P3 (part 2): `tests/changes_query.rs` — 23 changes/query/queryChanges tests (Mailbox+Email+Thread changes round-trips and RFC §5.2 paging, `updatedProperties`, Mailbox/Email query filter+Comparator+collation+window, Mailbox/Email queryChanges with `AddedItem`, `cannotCalculateChanges`/`tooManyChanges`/`unsupportedSort`/`unsupportedFilter`, `canCalculateChanges=false`, and the lib.rs-doc-example + RFC 8621 §2.6/§4.10 result-reference chains) | **22.26%** lines (6953 instr., 1548 covered) — **+5.59 pp / +389 lines** over P3 part 1's 16.67% | **60 client pass** (7 lib unit + 2 crypto-provider + 28 core + 23 new) · mock-jmap 41+35+4 pass | Verified: `cargo test -p jmap-client` green, `cargo test -p mock-jmap` green, `cargo clippy -p jmap-client --all-targets` clean for `tests/` (the only 7 warnings are the pre-existing `mock-jmap` lib ones), coverage summary lists only `src/` rows. `core/changes.rs` and `core/query_changes.rs` went from 0.00% to ~94%/~94%; `core/query.rs` 25.53% → 71.63%. P3 is now closed. New contract facts recorded as Findings 33–38. |
| 5 | 2026-10-28 | P4: mail-domain wiremock integration tests — `tests/mailbox.rs` (26), `tests/email.rs` (37), `tests/thread.rs` (11), `tests/search_snippet.rs` (11). Drives every `Client::mailbox_*` / `Client::email_*` / `Client::thread_get` / `Client::search_snippet_get` helper plus the matching `Request::` builders against RFC-verbatim 8621/8620 fixtures; each test owns its `MockServer` and asserts the method name went on the wire. | **46.31%** lines (6953 instr., 3220 covered) — **+24.05 pp / +1672 lines** over P3's 22.26% | **145 client pass** (7 lib unit + 2 crypto-provider + 28 core + 23 changes/query + 85 new) · mock-jmap 41+35+4 pass | Verified: `cargo test -p jmap-client` green, `cargo test -p mock-jmap` green, `cargo clippy -p jmap-client --all-targets` clean for `tests/` (the only warnings are the pre-existing `mock-jmap` lib ones), coverage summary lists only `src/` rows. `mailbox/helpers.rs`, `thread/{get,helpers,mod}.rs` and `email/search_snippet.rs` reached 100%. No `src/` file was modified. New contract facts recorded as Findings 39–52. |
| 6 | 2026-10-28 | P5: submission / identity / vacation wiremock integration tests — `tests/email_submission.rs` (27), `tests/identity.rs` (22), `tests/vacation_response.rs` (18). Drives every `Client::email_submission_*` / `Client::identity_*` / `Client::vacation_response_*` helper plus the matching `Request::` builders and `unwrap_*` accessors against RFC-verbatim 8621 §6/§7/§8 fixtures; rich happy paths plus at least one error path per method family, each test asserting the method name went on the wire. | **59.14%** lines (6953 instr., 4112 covered) — **+12.83 pp / +892 lines** over P4's 46.31% | **212 client pass** (7 lib unit + 2 crypto-provider + 28 core + 23 changes/query + 85 mail + 67 new) · mock-jmap 41+35+4 pass | Verified: `cargo test -p jmap-client` green (212), `cargo test -p mock-jmap` green (80), `cargo clippy -p jmap-client --all-targets` clean for `tests/` (all 7 warnings are the pre-existing `mock-jmap` lib ones — none in `src/` or `tests/`), coverage summary lists only `src/` rows. All 13 target modules went from 0.00%; `email_submission/query.rs` and `vacation_response/get.rs` reached 100%. No `src/` file was modified. New contract facts recorded as Findings 53–65. |
| 7 | 2026-10-29 | P6: sieve / principal / push_subscription / blob wiremock integration tests — `tests/principal.rs` (25), `tests/sieve.rs` (22), `tests/push_subscription.rs` (15), `tests/blob.rs` (12). Drives every `Client::sieve_script_*` / `Client::principal_*` / `Client::push_subscription_*` helper plus `Client::{upload,download,blob_copy}`, the matching `Request::` builders and `unwrap_*` accessors, against RFC 8620 §6.1–§6.3/§7.2, draft-ietf-jmap-sieve-12 §2 and RFC 9670 §4 fixtures; rich happy paths plus at least one error path per method family, each test asserting the method name went on the wire. | **75.64%** lines (6953 instr., 5259 covered) — **+16.50 pp / +1147 lines** over P5's 59.14% | **286 client pass** (7 lib unit + 2 crypto-provider + 28 core + 23 changes/query + 85 mail + 67 submission/identity/vacation + 74 new) · mock-jmap 41 P2 + 35 P1 + 4 doctests pass, 1 known flake | Verified: `cargo test -p jmap-client` green (286), `cargo llvm-cov --all-targets --ignore-filename-regex "mock-jmap\|tests" --summary-only` lists only `src/` rows, `cargo test -p mock-jmap` green apart from the pre-existing WS-handshake flake of the P3-followup box (Finding #77). 18 of the 20 target modules went from 0.00%; `blob/copy.rs` and `sieve/validate.rs` reached 100%. No `src/` file was modified. Regions 56.93% → 73.14%, functions 57.85% → 73.97%. New contract facts recorded as Findings 66–77. The flake reproduces at the same rate with and without the P6 files (4/9 vs 2/5 full-suite runs), confirming it is independent of this increment. |
| 8 | 2026-10-29 | P7: push-stream integration tests — `tests/websocket.rs` (15) + `tests/event_source.rs` (11), plus the P3-followup fix of the mock's WS-handshake write. Drives `Client::{connect_ws,send_ws,enable_push_ws,disable_push_ws,ws_ping,event_source}` and the RFC 8887 `WebSocketMessage` / RFC 8620 §7.3 `PushNotification`+`Changes` surfaces against the mock's own WebSocket and SSE legs: requestId correlation, `dataTypes`/`types=` filtering, `closeafter=state`, `ping` keepalive, `Last-Event-ID`/`pushState` resume, `EmailPush` delivery over WS only, `RequestError`→`ProblemDetails` vs a method error, and the not-connected error set. | **80.25%** lines (6953 instr., 5580 covered) — **+4.61 pp / +321 lines** over P6's 75.64% | **312 client pass** (7 lib unit + 2 crypto-provider + 28 core + 23 changes/query + 85 mail + 67 submission/identity/vacation + 74 P6 + 26 new) · mock-jmap 41 P2 + 35 P1 + 1 lib + 4 doctests pass, **0 flakes** | Verified: `cargo test -p jmap-client` green ×3 (312), `cargo test -p mock-jmap` green ×5 on top of the 8 post-fix runs (13 total, 0 flakes), `cargo clippy -p jmap-client --all-targets` clean for the two new files, coverage summary lists only `src/` rows. `event_source/stream.rs` reached **100.00%**, `event_source/mod.rs` 16.67% → **97.22%**, `client_ws.rs` 0.00% → **69.27%**, `event_source/parser.rs` 71.35% → **86.98%**. Regions 73.14% → 76.80%, functions 73.97% → 76.86%. No `src/` file was modified; `mock-jmap/` was (allowed: the flake fix + keepalive/`query` observability hooks). The P3-followup box is closed — see Findings #78/#79. New contract facts recorded as Findings 78–84. |
| 10 | 2026-10-29 | P9: in-crate `#[cfg(test)] mod tests` sweep of the remaining cold *pure* logic — 12 files / 115 tests: `core/set.rs` (19), `core/session.rs` (13), `email/mod.rs` (15), `email/set.rs` (14), `email/get.rs` (11), `principal/set.rs` (9), `mailbox/set.rs` (9), `email_submission/set.rs` (7), `mailbox/mod.rs` (7), `email/parse.rs` (6), `sieve/get.rs` (3), `sieve/query.rs` (2). Drives the generic `SetRequest` create/update/destroy bookkeeping and the whole `SetErrorType` table, the `Email`/`Mailbox`/`Principal`/`EmailSubmission` patch builders and their dotted-path forms, the `take_*` draining accessors, and the three custom serde property visitors — against RFC-verbatim fixtures. Baseline re-measured by stashing the increment and re-running the same `llvm-cov` command. | **96.30%** lines (10254 instr., 9875 covered) — **+6.11 pp / +2403 lines** over P8's 90.19% | **500 client pass** (195 lib unit + 2 crypto-provider + 28 core + 23 changes/query + 85 mail + 67 submission/identity/vacation + 74 P6 + 26 P7 + 115 new) · mock-jmap 81 pass | Verified: `cargo test -p jmap-client` green (500), `cargo test -p mock-jmap` green (81), `cargo clippy -p jmap-client --all-targets` emits **zero** warnings in `src/` (the remaining ones are the 7 pre-existing `mock-jmap` lib warnings plus 5 pre-existing in `tests/{principal,sieve,push_subscription}`), coverage summary lists only `src/` rows. Regions 88.84% → **95.64%**, functions 86.57% → **93.74%**. Diff is **insertions only** (3195 added, 0 deleted). Six of the twelve targets reached **100.00%**; the other six are ≥ 99.00%. New contract facts recorded as Findings 90–97. |

#### P3 (part 1) per-file line-coverage delta

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `client.rs` | 24.54% | **81.29%** | +56.75 | 80 → 265 (+185) |
| `core/session.rs` | 23.76% | **70.72%** | +46.96 | 43 → 128 (+85) |
| `core/get.rs` | 0.00% | **59.46%** | +59.46 | 0 → 44 (+44) |
| `core/error.rs` | 0.00% | **42.50%** | +42.50 | 0 → 34 (+34) |
| `core/request.rs` | 33.33% | **47.24%** | +13.91 | 163 → 231 (+68) |
| `core/set.rs` | 0.00% | **25.09%** | +25.09 | 0 → 67 (+67) |
| `core/query.rs` | 0.00% | **25.53%** | +25.53 | 0 → 36 (+36) |
| `core/response.rs` | 5.13% | **15.62%** | +10.49 | 22 → 67 (+45) |
| `mailbox/get.rs` | 0.00% | 31.03% | +31.03 | 0 → 27 (+27) |
| `mailbox/set.rs` | 0.00% | 28.89% | +28.89 | 0 → 26 (+26) |
| `mailbox/mod.rs` | 0.00% | 26.32% | +26.32 | 0 → 15 (+15) |
| `mailbox/helpers.rs` | 0.00% | 8.11% | +8.11 | 0 → 18 (+18) |
| `lib.rs` | 17.81% | 31.51% | +13.70 | 13 → 23 (+10) |
| `email/mod.rs` | 0.00% | 4.90% | +4.90 | 0 → 15 (+15) |
| `email/helpers.rs` | 0.00% | 1.98% | +1.98 | 0 → 7 (+7) |
| `crypto.rs` | 0.00% | 100.00% | +100.00 | 0 → 6 (+6) |
| **TOTAL** | **6.77%** | **16.67%** | **+9.90** | **471 → 1159 (+688)** |

#### P3 (part 2) per-file line-coverage delta

Against the state after P3 part 1 (`16.67%` / 1159 lines covered).

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `core/changes.rs` | 0.00% | **94.03%** | +94.03 | 0 → 63 (+63) |
| `core/query_changes.rs` | 0.00% | **93.65%** | +93.65 | 0 → 59 (+59) |
| `core/query.rs` | 25.53% | **71.63%** | +46.10 | 36 → 101 (+65) |
| `mailbox/query.rs` | 0.00% | **57.89%** | +57.89 | 0 → 22 (+22) |
| `thread/helpers.rs` | 0.00% | **34.38%** | +34.38 | 0 → 11 (+11) |
| `mailbox/helpers.rs` | 8.11% | **25.68%** | +17.57 | 18 → 57 (+39) |
| `email/helpers.rs` | 1.98% | **12.99%** | +11.01 | 7 → 46 (+39) |
| `email/query.rs` | 0.00% | **11.32%** | +11.32 | 0 → 18 (+18) |
| `mailbox/mod.rs` | 26.32% | **36.84%** | +10.52 | 15 → 21 (+6) |
| `core/request.rs` | 47.24% | **54.40%** | +7.16 | 231 → 266 (+35) |
| `core/get.rs` | 59.46% | **66.22%** | +6.76 | 44 → 49 (+5) |
| `core/response.rs` | 15.62% | **19.81%** | +4.19 | 67 → 85 (+18) |
| `core/error.rs` | 42.50% | **46.25%** | +3.75 | 34 → 37 (+3) |
| `email/mod.rs` | 4.90% | 6.86% | +1.96 | 15 → 21 (+6) |
| **TOTAL** | **16.67%** | **22.26%** | **+5.59** | **1159 → 1548 (+389)** |

Unchanged by this increment: `client.rs` (81.29%), `core/session.rs`
(70.72%), `lib.rs` (31.51%), `crypto.rs` (100%), `mailbox/get.rs` (31.03%).

Still at 0.00% after P3: `core/copy.rs`, `core/mod.rs`,
`email/{get,import,parse,set,search_snippet}.rs`, `event_source/stream.rs`,
`thread/{get,mod}.rs`, and every
`email_submission`/`identity`/`principal`/`push_subscription`/`sieve`
module — i.e. exactly the P4–P6 targets.

#### P4 per-file line-coverage delta

Against the state after P3 (`22.26%` / 1548 lines covered), re-measured with
the four P4 files removed from `tests/`.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `thread/mod.rs` | 0.00% | **100.00%** | +100.00 | 0 → 8 (+8) |
| `thread/get.rs` | 0.00% | **100.00%** | +100.00 | 0 → 6 (+6) |
| `email/search_snippet.rs` | 0.00% | **100.00%** | +100.00 | 0 → 55 (+55) |
| `mailbox/helpers.rs` | 25.68% | **100.00%** | +74.32 | 57 → 222 (+165) |
| `thread/helpers.rs` | 34.38% | **100.00%** | +65.62 | 11 → 32 (+21) |
| `email/helpers.rs` | 12.99% | **97.74%** | +84.75 | 46 → 346 (+300) |
| `mailbox/get.rs` | 31.03% | **96.55%** | +65.52 | 27 → 84 (+57) |
| `blob/upload.rs` | 0.00% | **75.00%** | +75.00 | 0 → 42 (+42) |
| `email/set.rs` | 0.00% | **74.35%** | +74.35 | 0 → 258 (+258) |
| `email/mod.rs` | 6.86% | **74.51%** | +67.65 | 21 → 228 (+207) |
| `email/get.rs` | 0.00% | **73.08%** | +73.08 | 0 → 152 (+152) |
| `email/parse.rs` | 0.00% | **72.73%** | +72.73 | 0 → 56 (+56) |
| `email/import.rs` | 0.00% | **67.03%** | +67.03 | 0 → 61 (+61) |
| `mailbox/mod.rs` | 36.84% | **73.68%** | +36.84 | 21 → 42 (+21) |
| `mailbox/set.rs` | 28.89% | **68.89%** | +40.00 | 26 → 62 (+36) |
| `core/copy.rs` | 0.00% | **61.84%** | +61.84 | 0 → 47 (+47) |
| `mailbox/query.rs` | 57.89% | **89.47%** | +31.58 | 22 → 34 (+12) |
| `core/set.rs` | 25.09% | **56.93%** | +31.84 | 67 → 152 (+85) |
| `email/query.rs` | 11.32% | **21.38%** | +10.06 | 18 → 34 (+16) |
| `core/request.rs` | 54.40% | **62.99%** | +8.59 | 266 → 308 (+42) |
| `core/get.rs` | 66.22% | **74.32%** | +8.10 | 49 → 55 (+6) |
| `lib.rs` | 31.51% | **36.99%** | +5.48 | 23 → 27 (+4) |
| `core/response.rs` | 19.81% | **20.75%** | +0.94 | 85 → 89 (+4) |
| `client.rs` | 81.29% | **82.21%** | +0.92 | 265 → 268 (+3) |
| **TOTAL** | **22.26%** | **46.31%** | **+24.05** | **1548 → 3220 (+1672)** |

Unchanged by this increment: `core/{changes,query,query_changes,error}.rs`,
`core/session.rs`, `crypto.rs`, `event_source/parser.rs`.

Still cold after P4 (P5/P6/P7 targets): `core/mod.rs`,
`event_source/{mod,stream}.rs` (partly), `blob/{copy,download,helpers}.rs`,
`client_ws.rs`, and every
`email_submission`/`identity`/`principal`/`push_subscription`/`sieve`
module. `email/query.rs` (21.38%) and `core/response.rs` (20.75%) remain
low because most of their surface is per-filter / per-method-variant code that
P8's unit sweep is a better fit for than per-request integration tests.

#### P5 per-file line-coverage delta

Against the state after P4 (`46.31%` / 3220 lines covered), re-measured with
the three P5 files removed from `tests/`. Instrumented total unchanged at
6953 lines.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `email_submission/query.rs` | 0.00% | **100.00%** | +100.00 | 0 → 43 (+43) |
| `vacation_response/get.rs` | 0.00% | **100.00%** | +100.00 | 0 → 23 (+23) |
| `vacation_response/helpers.rs` | 0.00% | **96.69%** | +96.69 | 0 → 117 (+117) |
| `identity/helpers.rs` | 0.00% | **95.56%** | +95.56 | 0 → 86 (+86) |
| `email_submission/get.rs` | 0.00% | **95.31%** | +95.31 | 0 → 61 (+61) |
| `email_submission/helpers.rs` | 0.00% | **92.59%** | +92.59 | 0 → 175 (+175) |
| `identity/set.rs` | 0.00% | **91.23%** | +91.23 | 0 → 52 (+52) |
| `vacation_response/set.rs` | 0.00% | **89.80%** | +89.80 | 0 → 44 (+44) |
| `identity/get.rs` | 0.00% | **88.89%** | +88.89 | 0 → 24 (+24) |
| `email_submission/mod.rs` | 0.00% | **84.21%** | +84.21 | 0 → 16 (+16) |
| `identity/mod.rs` | 0.00% | **82.35%** | +82.35 | 0 → 14 (+14) |
| `vacation_response/mod.rs` | 0.00% | **81.25%** | +81.25 | 0 → 13 (+13) |
| `email_submission/set.rs` | 0.00% | **78.29%** | +78.29 | 0 → 101 (+101) |
| `core/request.rs` | 62.99% | **76.28%** | +13.29 | 308 → 373 (+65) |
| `core/set.rs` | 56.93% | **65.17%** | +8.24 | 152 → 174 (+22) |
| `core/response.rs` | 20.75% | **27.27%** | +6.52 | 89 → 117 (+28) |
| `email/set.rs` | 74.35% | **76.37%** | +2.02 | 258 → 265 (+7) |
| `core/error.rs` | 46.25% | **47.50%** | +1.25 | 37 → 38 (+1) |
| **TOTAL** | **46.31%** | **59.14%** | **+12.83** | **3220 → 4112 (+892)** |

Of the +892 lines, 769 are the thirteen P5 target modules and 123 are shared
core plumbing (`core/request.rs` method-call/params glue, `core/set.rs`
`SetRequest` create/update/destroy + `ifInState`, `core/response.rs`
`unwrap_*` accessors, `email/set.rs` patch keys, `core/error.rs`
`SetError`/`MethodError` variants).

Regions moved 44.75% → 56.93% and functions 44.25% → 57.85% over the same
increment.

Still cold after P5 (P6/P7/P8 targets): `core/mod.rs`,
`event_source/{mod,stream}.rs`, `blob/{copy,download,helpers}.rs`,
`client_ws.rs`, and every
`principal`/`push_subscription`/`sieve` module. `email/query.rs` (21.38%)
and `lib.rs` (36.99%) are unchanged by this increment.

#### P6 per-file line-coverage delta

Against the state after P5 (`59.14%` / 4112 lines covered), re-measured with
the four P6 files removed from `tests/`. Instrumented total unchanged at
6953 lines.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `blob/copy.rs` | 0.00% | **100.00%** | +100.00 | 0 → 38 (+38) |
| `blob/helpers.rs` | 0.00% | **100.00%** | +100.00 | 0 → 25 (+25) |
| `sieve/validate.rs` | 0.00% | **100.00%** | +100.00 | 0 → 11 (+11) |
| `blob/download.rs` | 0.00% | **97.67%** | +97.67 | 0 → 42 (+42) |
| `principal/helpers.rs` | 0.00% | **96.12%** | +96.12 | 0 → 297 (+297) |
| `push_subscription/helpers.rs` | 0.00% | **95.12%** | +95.12 | 0 → 78 (+78) |
| `principal/mod.rs` | 0.00% | **93.75%** | +93.75 | 0 → 45 (+45) |
| `principal/query.rs` | 0.00% | **93.75%** | +93.75 | 0 → 45 (+45) |
| `principal/get.rs` | 0.00% | **93.33%** | +93.33 | 0 → 42 (+42) |
| `sieve/helpers.rs` | 0.00% | **91.05%** | +91.05 | 0 → 173 (+173) |
| `push_subscription/set.rs` | 0.00% | **90.57%** | +90.57 | 0 → 48 (+48) |
| `push_subscription/get.rs` | 0.00% | **90.32%** | +90.32 | 0 → 28 (+28) |
| `sieve/set.rs` | 0.00% | **87.18%** | +87.18 | 0 → 34 (+34) |
| `push_subscription/mod.rs` | 0.00% | **81.25%** | +81.25 | 0 → 13 (+13) |
| `sieve/get.rs` | 0.00% | **80.00%** | +80.00 | 0 → 12 (+12) |
| `sieve/query.rs` | 0.00% | **78.57%** | +78.57 | 0 → 11 (+11) |
| `sieve/mod.rs` | 0.00% | **76.92%** | +76.92 | 0 → 10 (+10) |
| `principal/set.rs` | 0.00% | **68.75%** | +68.75 | 0 → 77 (+77) |
| `blob/upload.rs` | 75.00% | **98.21%** | +23.21 | 42 → 55 (+13) |
| `core/request.rs` | 76.28% | **92.02%** | +15.75 | 373 → 450 (+77) |
| `core/query.rs` | 77.30% | **82.98%** | +5.67 | 109 → 117 (+8) |
| `core/set.rs` | 65.17% | **70.41%** | +5.24 | 174 → 188 (+14) |
| `core/get.rs` | 74.32% | **78.38%** | +4.05 | 55 → 58 (+3) |
| `client.rs` | 82.21% | **83.13%** | +0.92 | 268 → 271 (+3) |
| **TOTAL** | **59.14%** | **75.64%** | **+16.50** | **4112 → 5259 (+1147)** |

Of the +1147 lines, 1042 are the twenty P6 target modules (19 of 20 moved;
`blob/mod.rs` was already at 87.50%) and 105 are shared plumbing
(`core/request.rs` method-call/params glue for the new method names,
`core/set.rs` `SetRequest` create/update/destroy + `ifInState`,
`core/{get,query}.rs` request builders, `client.rs` upload/download/copy
dispatch).

Regions moved 56.93% → 73.14% and functions 57.85% → 73.97% over the same
increment.

Still cold after P6 (P7/P8/P9 targets): `client_ws.rs` (0.00%),
`event_source/{mod,stream}.rs`, `core/mod.rs`, `core/response.rs` (27.27%,
mostly the `unwrap_*` accessors of domains no test batch-drives),
`core/copy.rs` (61.84%), `core/error.rs` (47.50%), `lib.rs` (36.99%) and
`email/query.rs`. The 80% goal is within reach of P7 alone: `client_ws.rs`
(192 lines) plus `event_source/*` are the largest single untouched block
left.

#### P7 per-file line-coverage delta

Against the state after P6 (`75.64%` / 5259 lines covered), re-measured with
the two P7 files removed from `tests/`. Instrumented total unchanged at
6953 lines.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `event_source/stream.rs` | 0.00% | **100.00%** | +100.00 | 0 → 77 (+77) |
| `event_source/mod.rs` | 16.67% | **97.22%** | +80.55 | 6 → 35 (+29) |
| `client_ws.rs` | 0.00% | **69.27%** | +69.27 | 0 → 133 (+133) |
| `core/error.rs` | 47.50% | **72.50%** | +25.00 | 38 → 58 (+20) |
| `event_source/parser.rs` | 71.35% | **86.98%** | +15.63 | 137 → 167 (+30) |
| `lib.rs` | 36.99% | **49.32%** | +12.33 | 27 → 36 (+9) |
| `core/response.rs` | 27.27% | **31.70%** | +4.43 | 117 → 136 (+19) |
| `client.rs` | 83.13% | **84.05%** | +0.92 | 271 → 274 (+3) |
| `core/request.rs` | 92.02% | **92.23%** | +0.21 | 450 → 451 (+1) |
| **TOTAL** | **75.64%** | **80.25%** | **+4.61** | **5259 → 5580 (+321)** |

Of the +321 lines, 269 are the four P7 target modules
(`client_ws.rs` +133, `event_source/{stream,parser,mod}.rs` +77/+30/+29)
and 52 are shared plumbing the streams happen to cross: `core/error.rs`
(+20 — the `ProblemDetails`/`ProblemType` path a `RequestError` frame takes,
plus `Error::Internal`/`Error::WebSocket` variants), `core/response.rs`
(+19 — the `Response`/`TaggedMethodResponse` leg the WS `Response` frame
deserialises into), `lib.rs` (+9 — `PushObject::EmailPush` deserialisation
and the `DataType`/`Error` Display arms) and `client.rs`/`core/request.rs`
(+4 — the `event_source_url` accessor and the forced-capability request).

Regions moved 73.14% → 76.80% and functions 73.97% → 76.86% over the same
increment.

**The ≥ 80% line-coverage goal is met** (80.25%) three phases early. What is
left cold is not stream code: `core/response.rs` (31.70%, the `unwrap_*`
accessors of domains no batch drives), `email/query.rs` (21.38%),
`core/copy.rs` (61.84%), `core/error.rs` (72.50%) and the ~40 lines of
`client_ws.rs` that need a real `wss://` endpoint — the
`DummyVerifier`/TLS-config arm (`client_ws.rs:~40` lines) cannot be reached
over the mock's plain `ws://` socket without touching `src/`. P8/P9 are now
polish, not a rescue.

#### P8 per-file line-coverage delta

Against the state after P7 (`80.25%` / 5580 lines covered). The instrumented
total grew from 6953 to 8285 lines because the in-crate `mod tests` blocks
are themselves `src/` code and are counted by the metric; the deltas below
are read off the per-file percentages, which are unaffected by that growth.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `core/mod.rs` | 0.00% | **100.00%** | +100.00 | 0 → 18 (+18) |
| `email/query.rs` | 21.38% | **100.00%** | +78.62 | 72 → 338 (+266) |
| `lib.rs` | 49.32% | **100.00%** | +50.68 | 36 → 209 (+173) |
| `core/error.rs` | 72.50% | **99.58%** | +27.08 | 58 → 235 (+177) |
| `core/response.rs` | 31.70% | **99.61%** | +67.91 | 136 → 1274 (+1138) |
| **TOTAL** | **80.25%** | **90.19%** | **+9.94** | **5580 → 7472 (+1892)** |

Regions 76.80% → 88.84% and functions 76.86% → 86.57% over the same
increment. Of the +1892 covered lines, roughly 1240 are the new test bodies
themselves and the rest is `src/` logic the tests actually exercise — the
per-file percentages are the honest signal, and every one of the five moved
by 27 pp or more.

**Lines that cannot be reached.** `--show-missing-lines` reports **nothing**
for `email/query.rs`, `lib.rs`, `core/mod.rs` and `core/error.rs`. The
residual counts are llvm-cov line-attribution artefacts, not untested logic:

- `core/response.rs` — 5 missed lines, none listed by `--show-missing-lines`.
  They are the never-taken panic-message continuations of multi-line
  `assert_eq!` invocations inside the test module itself (a macro's panic
  path is attributed to a line past the macro's closing `)`).
- `core/error.rs` — 1 missed line, likewise not listed. All four `JMAPError`
  arms and `ProblemType::Other` are exercised; the count is a region-level
  artefact on the closing brace of the nested `match` at `error.rs:185`.

Two earlier unreachable lines were **eliminated** rather than documented: the
`single_method_response_is_untagged_ok_or_error` test originally used
`match … => panic!("must deserialise as the … arm")` guards, which can never
execute in a passing run. Rewriting those as guarded `matches!` expressions
asserts the same field values with no unreachable branch.

#### P9 per-file line-coverage delta

Against the P8 end state, re-measured with the increment stashed
(`88.84%` regions / `86.57%` functions / **90.19%** lines, 8285 instr.,
7472 covered). As in P8, the instrumented total grows (8285 → 10254)
because the in-crate `mod tests` blocks are themselves `src/` code; the
per-file percentages are the honest signal.

| `src/` file | before | after | Δ pp | lines covered |
|---|---|---|---|---|
| `principal/set.rs` | 68.75% | **99.18%** | +30.43 | 77 → 241 (+164) |
| `mailbox/set.rs` | 68.89% | **99.00%** | +30.11 | 62 → 198 (+136) |
| `core/session.rs` | 70.72% | **99.52%** | +28.80 | 128 → 413 (+285) |
| `mailbox/mod.rs` | 73.68% | **100.00%** | +26.32 | 42 → 163 (+121) |
| `email/mod.rs` | 74.51% | **100.00%** | +25.49 | 228 → 628 (+400) |
| `email/get.rs` | 76.44% | **100.00%** | +23.56 | 159 → 504 (+345) |
| `email/parse.rs` | 76.62% | **100.00%** | +23.38 | 59 → 154 (+95) |
| `email/set.rs` | 76.37% | **99.65%** | +23.28 | 265 → 567 (+302) |
| `sieve/query.rs` | 78.57% | **100.00%** | +21.43 | 11 → 49 (+38) |
| `email_submission/set.rs` | 78.29% | **99.11%** | +20.82 | 101 → 223 (+122) |
| `sieve/get.rs` | 80.00% | **100.00%** | +20.00 | 12 → 40 (+28) |
| `core/set.rs` | 79.78% | **99.66%** | +19.88 | 213 → 580 (+367) |
| **TOTAL** | **90.19%** | **96.30%** | **+6.11** | **7472 → 9875 (+2403)** |

Regions 88.84% → **95.64%** (+6.80) and functions 86.57% → **93.74%**
(+7.17) over the same increment. Every one of the twelve files moved by
almost 20 pp or more, and six of them closed to 100.00%.

**Lines that cannot be reached.** `--show-missing-lines` lists **11** lines
across the twelve touched files, none of them untested logic:

- `email/set.rs:252-253`, `email_submission/set.rs:71-72`,
  `mailbox/set.rs:116-117`, `principal/set.rs:142-143` — `SetObject::new()`
  for the `<Get>` flavour is a literal `unimplemented!()`. Reaching it means
  panicking, which fails the run (Finding #94).
- `core/session.rs:499` — the `Ok(..) => panic!(..)` arm of the test
  module's own `parse_err` helper; taken only if a URL that must not parse
  actually does.
- `core/session.rs:521` — the never-taken continuation of a multi-line
  `matches!` assertion inside the test module (the same macro-attribution
  artefact recorded for P8).
- `core/set.rs:403` — the `?` error-propagation path of the multi-line
  `write!` in `Display for SetError`; a `Formatter` over a `String` never
  returns `fmt::Error`.

Two previously-cold lines were **eliminated** rather than documented during
the pass: `SetObject::set_id`'s default no-op body (`core/set.rs:25`), now
driven by `the_default_set_id_is_a_noop`, and the three `Visitor::expecting`
bodies in `email/mod.rs` plus both `requires_account_id` arms in
`email/mod.rs` and `mailbox/mod.rs`, which needed a *type*-mismatched input
rather than an unknown string (Finding #95).

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
    (see "Mock push API" above); `wait_for_subscribers()` alone is not enough,
    because it proves the socket is attached, not that push is enabled.

_Discovered writing the P3 wiremock tests against the client's public API._

23. **`Client` does not implement `Debug`.** `Client::new()…connect().await
    .unwrap_err()` will not compile. Assert connect failures with
    `match … { Ok(_) => panic!(…), Err(err) => err }` instead.
24. **`Response::method_response_by_pos(i)` returns `T`, not `Option<T>`.**
    It `remove`s from the internal vec, so an out-of-range index panics in the
    Vec, not in an `unwrap()`. There is no fallible positional lookup; use
    `method_response_by_id` when the id is unknown.
25. **`TaggedMethodResponse` is not `Clone` (only `Debug`).** A test that wants
    to look at a response *and* consume it must read what it needs through
    `method_response_by_id(&self)` first, then take ownership with
    `method_response_by_pos`.
26. **`SetResponse::created(id)` mutates `not_created`.** The lookup `remove`s
    from whichever map holds the id, so calling `created()` and then
    `unwrap_create_errors()` reports success even though the record failed.
    Check the aggregate error *before* the per-record lookup (as
    `set_error_in_not_created_surfaces_as_error_set` does). Same for
    `updated()` / `not_updated` and `destroyed()` / `not_destroyed`.
27. **An unrecognised method-error `type` is a *parse* error, not
    `Error::Method`.** `MethodErrorType` has no catch-all variant, so
    `["error", {"type": "urn:ietf:params:jmap:error:notFound"}, "s0"]` fails
    deserialisation of the whole `Response` object and surfaces as
    `Error::Parse`. RFC 8620 §3.6.2 requires an unknown type to be treated as
    `serverFail`; the client cannot. P4–P6 must only use `MethodErrorType`
    variants the enum actually names.
28. **`urn:ietf:params:jmap:error:unauthorized` is not a `JMAPError`.**
    `JMAPError` registers exactly four URIs (`unknownCapability`, `notJSON`,
    `notRequest`, `limit`); everything else, including the 401 `unauthorized`
    a real server returns for bad credentials, lands in `ProblemType::Other(uri)`
    with the URI kept verbatim.
29. **RFC 8620 §2.1's example session spells `maxConcurrentRequest`
    (singular).** That is a typo in the published example: §2's prose and the
    client's `CoreCapabilities` field are `maxConcurrentRequests`. A fixture
    copied literally from the example fails to deserialise.
30. **`MailCapabilities.maxMailboxDepth` is a non-`Option<usize>`.**
    `src/email/mod.rs` declares it required, so the RFC-legal `null` value (and
    a missing member) is rejected. Fixtures must send an integer.
31. **`client.build().send_get_mailbox()` sends nothing.** The typed `send_*`
    helpers transmit only the calls already added to the builder, so without a
    preceding `request.get_mailbox()` the body is `methodCalls: []` — and a
    canned single-call response still deserialises, so the test would pass while
    asserting nothing about the call. Every typed-send test must add the call
    explicitly (`tests/core_methods.rs` wraps this as `send_mailbox_get`).
32. **Pre-existing `mock-jmap` flake (not P3 work).**
    `mock-jmap/tests/p2_streams.rs::websocket_refuses_a_connection_that_offers_no_subprotocol`
    intermittently reads an *empty* rejection body and fails its
    `missingSubprotocol` assertion (observed 1 run in 3; the other 41 stream
    tests stayed green). The rejection body write and the client's read race, so
    this is a timing artefact, not a semantic difference. Follow-up: make the
    handshake flush and await the body write, or assert only the 400 status
    there. Out of P3's scope (P3 touches `tests/` in `jmap-client` only),
    recorded here so the orchestrator can schedule it.

_Discovered writing the P3 changes/query/queryChanges wiremock tests._

33. **`MockServer::reset()` clears the recorded requests, not just the mocks.**
    A multi-phase test that reads the spy log after a reset sees only the
    post-reset requests, so a `jmap_request_bodies(…).len() == 2` assertion
    fails against a one-entry log. Snapshot with `single_jmap_request` *before*
    resetting, or assert one phase at a time. (The `tests/common/mod.rs`
    helpers are unaffected; this is wiremock's own semantics.)
34. **There is no `upToPosition` in `/queryChanges`.** RFC 8620 §5.6 makes
    `upToId` a *request* argument only; the response carries
    `{accountId, oldQueryState, newQueryState, total?, removed, added}` and
    `QueryChangesResponse` matches that exactly. Anything expecting an
    `upToPosition` response member is inventing a member.
35. **`QueryChangesRequest.calculateTotal` is always serialised.** It is a
    plain `bool` (default `false`), not `Option<bool>` like
    `QueryRequest::calculate_total`, so `calculateTotal: false` is on the wire
    even when the test never sets it. RFC 8620 §5.6 gives the same default, so
    the request stays conformant — but the member is never absent. Same story
    for Mailbox's `sortAsTree`/`filterAsTree`, which ride along on both
    `Mailbox/query` and `Mailbox/queryChanges`.
36. **The client does not validate `maxChanges`.**
    `ChangesRequest::max_changes(0)` serialises `"maxChanges": 0`, which
    RFC 8620 §5.2 says a server MUST reject with `invalidArguments` ("If
    supplied by the client, the value MUST be a positive integer greater than
    0"). The crate's own doc example in `src/lib.rs` uses `0`. The test
    asserts the serialisation; a real server would answer `invalidArguments`.
37. **A `()` per-type changes extension deserialises fine.** For
    Email/Thread/Identity/… `ChangesObject::ChangesResponse = ()`, and
    serde's `FlatMapDeserializer::deserialize_unit` accepts the empty
    flattened map, so a standard `/changes` response with no extra members
    parses. Only Mailbox has a real extension (`updatedProperties`,
    RFC 8621 §2.2), reachable through
    `response.arguments().updated_properties()` — it is *not* a member of the
    core `ChangesResponse`.
38. **`QueryResponse`/`QueryChangesResponse` are not re-exported from
    `core::response`.** `src/core/response.rs` imports them privately to build
    its `MethodResponse` variants. Tests must name
    `jmap_client::core::query::QueryResponse` and
    `jmap_client::core::query_changes::QueryChangesResponse`; only the
    generic `ChangesResponse<O>` aliases (`MailboxChangesResponse`,
    `EmailChangesResponse`, `ThreadChangesResponse`) are public in
    `core::response`.

39. **`Mailbox<Set>` emits an explicit JSON `null` for a cleared member.**
    The `skip_serializing_if` guards (`string_not_set`, `role_not_set`) only
    suppress the `Some("")` / `Some(Role::None)` seeds that `Mailbox::new()`
    installs. Once a setter assigns `None` the member rides along as `null`,
    so `mailbox_create(name, None, Role::None)` serialises
    `{"name":…,"parentId":null,"role":null}` and `mailbox_move(id, None)`
    serialises `{"<id>":{"parentId":null}}`. RFC 8620 §5.3 explicitly allows
    `null` in a patch object to mean "clear", so this is conformant — but a
    test that expects the key to be *absent* will fail.
40. **`Email/copy` answers one call with TWO method responses.** RFC 8621
    §4.7 says the server returns the `Email/copy` result *and* an implicit
    `Email/set` response under the same call id, copy result first.
    `Request::send_single` pops the **last** response, so `Client::email_copy`
    against a spec-following server tries to parse the `Email/set` body as
    `CopyResponse` and fails with `Error::Parse`. The happy-path test mounts
    only the `Email/copy` response; the two-response shape is pinned by
    `email_copy_with_the_rfc_implicit_set_response_is_a_parse_error`.
41. **`Email/copy`'s failure member is `notCreated`, not `notCopied`.**
    RFC 8621 §4.7 reuses the `/set` error vocabulary. Anything expecting a
    `notCopied` member is inventing one.
42. **There is no `mailboxExists` SetErrorType.** RFC 8621 §2.5 defines
    `mailboxHasChild` and `mailboxHasEmail` for the destroy guards; the
    client's `SetErrorType` names exactly those. Per Finding 27, only variants
    the enum actually names may be asserted.
43. **No dedicated seen/flagged/draft helpers.** The task brief assumed
    `email_mark_seen`-style methods; the crate exposes only the generic
    keyword setters. Use `email_set_keyword(id, "$seen" | "$flagged" |
    "$draft", bool)`, which emits the dotted
    `keywords/$<name>` patch key.
44. **No `Client::mailbox_query_changes` and no `Client::thread_changes`.**
    `queryChanges` is reachable only at the `Request::` level
    (`Request::query_mailbox_changes` / `query_email_changes`), and Thread
    changes only through `Request::changes_thread` +
    `send_changes_thread`. Tests that want them must build the request
    explicitly.
45. **`Client::email_copy` cannot set `onSuccessDestroyOriginal`.** The
    client helper does not expose it; only `Request::copy_email` does, so a
    move-between-accounts test must go through the builder.
46. **Header aliases are exact, case-sensitive strings — and `Message-ID` is
    spelled with an all-caps `ID`.** `EmailProperties` aliases `messageId`
    as `header:Message-ID:asMessageIds`, `cc` as `header:Cc:asAddresses`,
    `bcc` as `header:Bcc:asAddresses`, `sentAt` as `header:Date:asDate`,
    and so on (all only when the `debug` feature is off). A server that
    answers `header:Message-Id:asMessageIds` — mixed case — does **not**
    populate `Email::message_id`; the value falls through to the flattened
    `Email::headers` map as `HeaderValue::AsTextAll`. Pinned by
    `header_aliases_are_case_sensitive_so_other_spellings_land_in_the_header_map`.
47. **`header:To:asGroupedAddresses` has no alias.** Unlike
    `header:To:asAddresses`, the grouped form is not aliased to `to`, so it
    always lands in the flattened `headers` map as
    `HeaderValue::AsGroupedAddresses`. Supplying both `to` and
    `header:To:asAddresses` in one record is a duplicate-field error.
48. **`Client::thread_get` is `take_list().pop()`.** A spec-following server
    that answers every requested id returns the *last* record, not the one
    whose id was passed in. `thread_get` is therefore only safe for
    single-id fetches; multi-id reads must use
    `Request::get_thread().ids([..])` and index `response.list()`. Pinned by
    `thread_get_pops_the_last_list_entry_when_the_server_returns_several`.
49. **`Request::send_single<T>` deserialises EVERY response in the
    envelope as `T`.** It parses the whole `methodResponses` array as
    `Vec<SingleMethodResponse<T>>` *before* popping the last entry, so a
    heterogeneous batch (e.g. `Email/query` followed by `SearchSnippet/get`)
    fails with `Error::Parse` on the leading response even when the target
    response is perfectly well formed. Multi-method batches must use
    `Request::send()` and unwrap per method via
    `TaggedMethodResponse::unwrap_*` / `Response::method_response_by_id`.
    Pinned by `send_single_cannot_deserialise_a_heterogeneous_batch`.
50. **`Email/import`'s `created` record carries only
    `{id, blobId, threadId, size}`.** RFC 8621 §4.8. The mailbox placement
    and keywords exist only on the *request*, so `email.mailbox_ids()` on the
    returned `Email` is empty — assert the request body, not the record.
51. **Creation-id prefixes differ per request type.** `SetRequest::create()`
    yields `c0`, `c1`, … while `EmailImport::create_id()` yields `i0`,
    `i1`, … and `CopyRequest::create(id)` keys by the caller-supplied id.
    A test that hard-codes `c0` for an import will never find its record.
52. **`Email` and `EmailBodyPart` flatten unknown keys into
    `AHashMap<Header, HeaderValue>`.** Any member that is not a known
    property and not a `header:<Name>:as<Form>[:all]` string makes
    `Header::parse` fail, which fails the *whole* response with
    `Error::Parse`. Canned fixtures must therefore be exactly the RFC shape —
    a stray `"partId"` on an `Email` record, or a typo'd property name,
    takes down the entire `Email/get`.

_Discovered writing the P5 submission/identity/vacation wiremock tests._

53. **`EmailSubmission` has no delivery-timestamp member.** RFC 8621 §7
    spells the object exactly `{id, identityId, emailId, threadId, envelope,
    sendAt, undoStatus, deliveryStatus, dsnBlobIds, mdnBlobIds}`: there is
    no `deliveredAt`, `undeliveredEvents`, `deliveryAttempts` or
    `bounceEmailId`, and no per-recipient timestamp anywhere. Delivery
    state is `deliveryStatus: { "<recipient>": {smtpReply, delivered,
    displayed} }` with `Delivered ∈ {queued, yes, no, unknown}` and
    `Displayed ∈ {unknown, yes}`; a *bounce* is a `delivered: "no"` entry
    carrying the SMTP reply string, not a linked Email. The P5 box's
    "delivery time" is `sendAt` (when the server will/did send) plus that
    status map.
54. **`EmailSubmission/set` answers one call with TWO method responses and
    `Client::email_submission_create` reads the wrong one.** Same class as
    Finding #40. RFC 8621 §7.5.1: "there are two responses due to the
    implicit `Email/set` call, but both have the same method call id", the
    submission result first. `send_single` pops the **last** (Finding #49),
    so the helper ends up holding the implicit `Email/set` body, which has
    no `created` member, and returns
    `Error::Internal("Id c0 not found.")` against a spec-following server.
    Pinned by
    `email_submission_create_with_the_rfc_implicit_email_set_response_lands_on_the_wrong_body`;
    the correct read is `Request::send()` plus
    `method_response_by_pos(0).unwrap_set_email_submission()`, after which
    the implicit `Email/set` is still readable.
55. **No `Client::email_submission_update` and no
    `Client::email_submission_query_changes`.** A submission is mutable only
    through `Client::email_submission_change_status(id, UndoStatus)` (emits
    `{"<id>": {"undoStatus": …}}`); arbitrary patches and
    `EmailSubmission/queryChanges` exist at the `Request::` level only
    (`set_email_submission`, `query_email_submission_changes`).
56. **`Client::email_submission_get` and `Client::identity_get` are
    `take_list().pop()`.** The Finding-#48 trap repeated in both domains:
    they return the *last* record the server sent, not the one whose id was
    passed, so they are safe for single-id fetches only. Multi-id reads must
    go through `Request::get_email_submission().ids([..])` or
    `Request::get_identity().ids([..])` and index `response.list()`.
    Pinned by `*_pops_the_last_record_when_the_server_returns_several` in
    both test files.
57. **`Identity`, `EmailSubmission` and `VacationResponse` do not implement
    `PartialEq`.** `assert_eq!(client.identity_update(id, patch).await
    .unwrap(), None)` fails to compile (trait bound unsatisfied); assert with
    `.is_none()` instead. Only the nested enums derive equality
    (`UndoStatus`, `DeliveryStatus`, `Delivered`, `Displayed`, `Property`).
58. **There is no "most likely identity" helper in the crate.** RFC 8621 §6
    describes a client-side selection rule: prefer the Identity whose `email`
    matches the sender exactly, else the `*@domain` wildcard identity. The
    crate exposes nothing equivalent, so callers must reimplement it.
    `tests/identity.rs` carries a local `most_likely_identity()` plus
    `identity_selection_prefers_an_exact_address_over_a_wildcard_identity`
    to pin the RFC semantics and document the gap.
59. **`Identity` has no `sortOrder`, `draftId` or `queueId`.** The object is
    `{id, name, email, replyTo, bcc, textSignature, htmlSignature,
    mayDelete}` (RFC 8621 §6). `mayDelete` is a server-computed, read-only
    flag: a test that wants "cannot delete" sets `mayDelete: false` in the
    fixture and drives the `forbidden` SetError path; it cannot set the flag
    itself.
60. **Identity rides on the Mail capability; the client adds no
    identity-specific URI.** `Request::{get,set,changes}_identity` add only
    `urn:ietf:params:jmap:mail` (RFC 8621 §1.3.4 registers Identity under
    Mail). A test must not expect an identity-specific entry in `using`, and
    a request that `using`s only Core while calling `Identity/get` is a shape
    the client never produces.
61. **`VacationResponse` is a singleton with a hard-coded id.** RFC 8621 §8:
    at most one object per account and "It MUST have the id `singleton`".
    The client hard-codes it: `vacation_response_get` requests
    `ids: ["singleton"]` and maps the result through `take_list().pop()`,
    which is why its return type is `Option<VacationResponse>`: an empty
    `list` is the RFC-legal "no vacation response configured" answer, not an
    error. `vacation_response_destroy()` takes no id for the same reason.
62. **`Request::set_vacation_response` builds its params from
    `Method::GetVacationResponse`.** `src/vacation_response/helpers.rs:152`
    passes `self.params(Method::GetVacationResponse)` into
    `Arguments::vacation_response_set` while registering the call as
    `Method::SetVacationResponse`. Harmless today: `SetRequest::new(params)`
    never inspects the method, so the name on the wire is correct. But any
    future change that keys off the params' method would silently mis-route
    the set call. Recorded, not fixed (content-protection rule).
63. **`vacation_response_enable` with an unset body member serialises it as
    explicit `null`.** `enable("subject", None::<String>, Some(html))` sends
    `{"isEnabled": true, "subject": …, "textBody": null, "htmlBody": …}`.
    Same mechanism as Finding #39: the `skip_serializing_if` guards only
    suppress the sentinel `Some(…)` seeds, so an assigned `None` rides along.
    RFC 8620 §5.3 defines `null` in a patch as "clear this member", so the
    request stays conformant, but the key is present, and a server that treats
    `null` as "ignore" would leave the old body in place.
64. **`Filter::and(..)` cannot infer its element type from `.into()` calls.**
    Building the conditions inline yields E0283 (ambiguous trait bounds); the
    collection must be annotated, e.g.
    `let conditions: Vec<Filter<SubmissionFilter>> = vec![…];`. Same for the
    other domains' operator groups.
65. **`Email<Set>` dotted-patch removal is boolean `false`, not `null`.**
    The per-key setters are `mailbox_id(&str, bool)` and `keyword(&str, bool)`
    (there is no `mailbox_ids_set` / `keywords_set`), and they insert
    `mailboxIds/<id>: false` / `keywords/<k>: false`. That is exactly what
    RFC 8621 §4.3 defines for those dotted keys, so it is correct, but it is
    *not* the generic RFC 8620 §5.3 "`null` clears the property" form, which
    these setters cannot produce. `mailbox_ids([])` sends an empty map
    instead of a removal patch.

_Discovered writing the P6 sieve/principal/push_subscription/blob wiremock
tests._

66. **`PushSubscription/get` fixtures must carry a `state` the RFC forbids.**
    The generic `GetResponse<T>` declares `state: String` with no default,
    but RFC 8620 §7.2.1: "It also does *not* return a `state` argument."
    A response copied verbatim from the RFC fails with
    `missing field "state"`, so the fixtures add `"state": "u1"` and say
    why. Only Push is affected — `SieveScript/get` and every mail-domain
    `/get` do define `state`, so their examples parse untouched.
67. **`Principal.acl` rides as `"acl"`, not RFC 9670 §4's `shareWith` —
    and the property name disagrees with the field.** `PrincipalProperty::
    ShareWith` serialises/`Display`s as `"shareWith"`, so a property-subset
    request asks the server for `shareWith`, but the response is parsed into
    the field named `acl` (`src/principal/mod.rs:76`). Requesting the
    property and reading the value therefore never connect: a server that
    answers `{"shareWith": {...}}` leaves `Principal::acl()` empty. Fixtures
    must use `"acl"`.
68. **`SieveScript` has no `order` member.** draft-ietf-jmap-sieve-12 §2.1
    defines the object as `{id, name, isActive, blobId}` (published as
    RFC 9661 §2.1); ordering is implied by `isActive`, not stored. The P6
    box's "set" list has no ordering knob, and
    `sieve_script_record_has_no_order_member` pins the four-member shape so
    a future `order` addition shows up as a test failure rather than a
    silent fixture drift.
69. **Time zone spelling: the crate says `timezone`, RFC 9670 says
    `timeZone`.** `Principal::timezone` has no `#[serde(rename)]`, so it
    reads and writes `"timezone"`; `PrincipalProperty::Timezone` renames to
    `"timezone"` too. An RFC-conformant server sending `"timeZone"` is
    silently ignored (unknown keys are dropped, not an error), and the same
    request for property `"timezone"` is a member name the server does not
    know. `principal_enum_wire_names` pins the crate's spelling.
70. **`capabilities` is a flat `Vec<String>`; RFC 9670 §4 makes it a map.**
    The RFC types it as `{ "<capability-uri>": { … } }` with per-capability
    server data. The crate models `Option<Vec<String>>`, so any object the
    server puts under a capability URI is lost. Fixtures use a JSON array.
71. **`DKIM` members have `#[serde(rename)]` but no `skip_serializing_if`,
    so `None` rides the wire as explicit `null`.**
    `domain_enable_dkim("P1", "key-material", "sel1", None)` sends
    `{"dkimSelector":"sel1","dkimExpiration":null}`. Unlike the
    `string_not_set`-guarded `Principal` fields (Finding #39's mechanism),
    there is no sentinel here: an absent expiration is indistinguishable
    from a cleared one, and RFC 8620 §5.3 reads that `null` as "clear this
    member".
72. **`ACL`'s serde names carry a `may` prefix that `Display` drops, and one
    variant changes meaning.** Wire `mayRename`/`mayDelete`/`mayShare`
    versus Display `rename`/`delete`/`administer`; the variant is named
    `Administer` while the wire key is `mayShare`
    (`src/principal/mod.rs:132`). `format!("{}", ACL::Administer)` is
    therefore *not* the JSON key for it — tests must compare against the
    serde name when asserting the wire shape.
73. **`DataType` is a closed enum with no `#[serde(other)]` catch-all.**
    RFC 8620 §7.2.3's example pushes `"types": ["Todo"]`, which fails to
    deserialise and takes down the whole `PushSubscription/get` parse. The
    fixture substitutes `Mailbox` and keeps the rest of the RFC example.
    Any server type outside the fifteen listed variants (Email … FileNode)
    is unrepresentable.
74. **`Client::sieve_script_deactivate()` sends
    `onSuccessDeactivateScript: true`, a member the sieve draft never
    defines.** draft-ietf-jmap-sieve-12 §2.2 spells deactivation as
    `onSuccessActivateScript: null` on a `SieveScript/set`; there is no
    `onSuccessDeactivateScript` in the draft or in RFC 9661. A
    spec-following server ignores the unknown member, so the call silently
    does nothing. `sieve_script_deactivate_sends_the_deactivation_argument`
    pins what the crate actually sends so the divergence stays visible.
75. **Sieve script uploads always go out as `application/octet-stream`,
    never `application/sieve`.** All three helpers call
    `self.upload(None, script, None)`
    (`src/sieve/helpers.rs:37,64,168`), and `upload` falls back to
    `content_type.unwrap_or("application/octet-stream")`
    (`src/blob/upload.rs:76`). The draft §2.2.1 example uploads the script
    with `Content-Type: application/sieve`; a push server or a strict
    upload endpoint that sniffs the type will not see a sieve script.
76. **No `Client::push_subscription_get` and no
    `Client::principal_query_changes`.** Push subscriptions are not tied to
    an account (RFC 8620 §7.2.1: "it does *not* take or return an
    `accountId` argument"), so there is no `Client`-level read helper — the
    get leg goes through `Request::get_push_subscription()` +
    `send_get_push_subscription()`. Same shape for
    `Principal/queryChanges`: `Request::query_principal_changes()` only.
    Both are exercised at the `Request::` level in the P6 files.
77. **The P3-followup `mock-jmap` WS-handshake flake is independent of the
    client test files.**
    `websocket_refuses_a_connection_that_offers_no_subprotocol`
    (`mock-jmap/tests/p2_streams.rs:784`) fails intermittently with
    `the rejection explains itself, got: ` (empty body): the mock writes the
    rejection and closes, and the client's read sometimes wins the race.
    Measured over full `cargo test -p mock-jmap` runs: **4 failures in 9
    runs with the P6 files present, 2 in 5 with them stashed away** — the
    same rate either way, and the P6 files live in a different crate and
    never touch the mock. The single failing test passes when run on its own
    (`--test p2_streams websocket_refuses` → 2 passed). Still open per the
    P3-followup box; not fixed here because `mock-jmap/` is protected by
    the content-protection rule. P7 must either fix the mock's flush or
    tolerate this one test.
 78. **A refused WS handshake reports only the bytes that were already in the
    read buffer when the head parsed — so a rejection body must be written in
    one call.** tungstenite 0.29 `src/handshake/client.rs` answers a rejected
    upgrade with `*e.body_mut() = Some(tail)`, where `tail` is whatever is
    left in the buffer at the instant the response *head* completes parsing.
    The mock's old `write_response` did `write_all(head)` then
    `write_all(body)`, which the OS is free to ship as two segments; when the
    head parsed first the tail was empty and
    `websocket_refuses_a_connection_that_offers_no_subprotocol` saw
    `got: ` — an empty rejection body. This is *not* the usual "closed before
    flushing" race: the bytes were flushed, they just arrived after the parse.
    Fixed in `mock-jmap/src/http.rs` by composing status line + headers +
    body into a single `write_all` before one `flush`, and pinned by
    `http::tests::write_response_emits_head_and_body_in_one_write`.
    Measured after the fix: **13 consecutive full `cargo test -p mock-jmap`
    runs, 0 flakes** (against P6's 4-in-9 / 2-in-5), and 20 targeted
    `websocket_refuses` runs, 0 failures. Client-side lesson: never assert on
    a rejection body the server wrote in more than one syscall.
 79. **`EventParser::push_bytes` replaces the buffer, it does not append —
    so one SSE event must be exactly one chunk.**
    `src/event_source/parser.rs:55` is `self.bytes = Some(bytes)`, and
    `needs_bytes()` gates the next read. An event split across two reads
    therefore loses its first half outright (the second `push_bytes`
    overwrites it), and the client silently drops a notification rather than
    reporting a parse error. The mock's `PushEvent::to_sse` is built to match:
    one event, one line of `data:`, written by a single `write_chunk` — the
    same single-write rule as Finding #78, for a different reason. Any future
    mock work that batches SSE events, or any real proxy that re-chunks the
    stream, breaks the client's parser invisibly.
 80. **`connect_ws` throws away the `pushState` of every push frame, so a WS
    push notification cannot be used to resume.** RFC 8887 §4.3.5.1 puts
    `pushState` on the `StateChange` push frame precisely so the client can
    reconnect from it, and the mock sends it
    (`PushEvent::to_ws_frame` inserts `pushState`). But
    `WebSocketMessage::PushNotification(push.push)` in `src/client_ws.rs`
    unwraps the `PushObject` and discards the sibling member, so the state
    never reaches the caller and cannot be fed back into
    `enable_push_ws(_, _, Some(state))`. A client that wants to resume must
    poll `Session`/`Core/get` instead of listening to its own push stream.
    `tests/websocket.rs` asserts the notification *content* only; the lost
    `pushState` is asserted nowhere because nothing can observe it.
 81. **SSE keepalives are invisible to the caller: the parser drops
    `event: ping` unless the `debug` feature is on.**
    `filter_notification` matches `EventType::Ping` and, without
    `#[cfg(feature = "debug")]`, falls through the `match` without returning —
    the event vanishes. A test (or an application) watching the stream cannot
    tell "keepalive arrived" from "connection wedged". P7 works around it
    mock-side: `PushBus::record_keepalive()` is called *after* the frame is
    written, and `MockHandle::keepalive_count()` is what
    `a_ping_keepalive_is_written_and_the_stream_keeps_delivering` waits on.
    With `debug` on, the same frame surfaces as a
    `PushNotification::StateChange` whose account key is the literal
    `"ping"` with an empty type map — a shape no caller is written to expect.
 82. **`Client::event_source`'s parameters are not what the RFC's names
    suggest, and the defaults are load-bearing.** The signature is
    `(Option<impl IntoIterator<Item = DataType>>, bool, Option<u32>,
    Option<&str>)`: `close_after_state` is a **bool** that maps to the literal
    `state`/`no` in the URL, and `ping` is whole **seconds** — so the
    smallest keepalive a caller can ask for is 1 s (`ping=0` means "server
    picks", and the mock floors it at 20 ms while RFC 8620 §7.3 suggests
    300 s). `types=None` emits `types=*`. The whole query is asserted
    verbatim in two P7 tests:
    `types=Mailbox,Email&closeafter=state&ping=45` and
    `types=*&closeafter=no&ping=0`. Note the type names are the `DataType`
    **Display** strings (`Mailbox`, `Email`), not the `urn:ietf:params:jmap:*`
    URNs used in `using:` — a server that matches on URNs here sees nothing.
 83. **`Changes::account_changes()` drains, and `is_empty()` reads the type
    maps rather than the account keys.**
    `account_changes` is `self.changes.remove(account_id)`
    (`src/event_source/mod.rs:57`): the second call for the same account
    returns `None`, and after draining every account `is_empty()` is `true`
    even though the notification was full when it arrived. Conversely
    `is_empty()` is `!self.changes.values().any(|c| !c.is_empty())`, so an
    account mapped to an *empty* map counts as no change at all —
    `changed_accounts()` can list an account that `is_empty()` ignores.
    Both are deliberate-looking but surprising; the two sync tests in
    `tests/event_source.rs` pin the exact semantics so a future refactor
    cannot quietly turn the drain into a getter.
 84. **The client cannot send `Core/echo`, so `Mailbox/get` is the WebSocket
    synchronisation frame — and a forced unadvertised capability leaves the
    socket usable.** RFC 8887 §4.3.4 defines `Core/echo` as the natural
    round-trip probe, but `core::request::Arguments` has no `Echo` variant, so
    `tests/websocket.rs` uses a `Mailbox/get` as the ordered echo that proves
    a preceding `WebSocketPushEnable`/`Disable` was processed (Finding #22).
    Separately, a `RequestError` frame is reachable by forcing a capability the
    session does not advertise — `Request::add_capability(URI::Contacts)` —
    and it surfaces as `Error::Problem` with `unknownCapability` while the
    socket stays open and keeps answering later requests. That is the
    contrast the tests draw against a method-level error inside a normal
    `Response`: one kills nothing, and neither closes the connection.

_Discovered writing the P8 in-crate unit tests against `core/response.rs`,
`core/error.rs`, `email/query.rs` and `lib.rs`._

85. **`Response` carries `request_id`, not the RFC 8887 `requestId`.**
    `src/core/response.rs` declares the field without a `#[serde(rename)]`, so
    the JSON key is the snake_case `request_id`. The WebSocket envelope
    (`client_ws::WebSocketResponse`) *does* rename to `requestId`, which is
    what RFC 8887 §4.4.1 actually specifies. The two envelopes therefore
    disagree with each other and the HTTP one disagrees with the RFC. A unit
    test pins the current key so a future rename is a deliberate act rather
    than a silent drift; nothing in the HTTP path can consume a `requestId`
    written by a spec-following server.
86. **`Error::Transport`'s `Display` is only reachable by building a request,
    and that needs a crypto provider installed first.** `reqwest::Error::new`
    is `pub(crate)`, so the only offline way to obtain a `Transport` error is
    `Client::new().get("://not-a-valid-url").build()`. That call panics
    unless a rustls process-default provider exists, and this crate builds
    reqwest with `rustls-no-provider`. The test therefore calls
    `rustls::crypto::aws_lc_rs::default_provider().install_default()` first
    and is gated `#[cfg(feature = "websockets")]` to match the default
    feature set. Without the install step the test aborts with "no process-level
    CryptoProvider was found" rather than failing an assertion.
87. **`SetResponse` has no `not_updated(id)` accessor.** A `notUpdated` entry
    is surfaced through `updated(id)`, which returns the `SetError` as an
    `Err` rather than exposing the record — and there is no read-only probe
    that leaves the maps intact. Worse, `created()` / `updated()` /
    `destroyed()` each **remove** the entry from whichever map holds it
    (Finding #26 generalised), so a second lookup of the same id yields
    `Error::Internal("Id … not found.")` and `take_updated_ids()` afterwards
    returns an empty `Vec`. Tests must read each id exactly once and in a
    deliberate order.
88. **A payload-less method-response envelope is the only way to reach the
    `visit_seq` missing-response guards — and it works uniformly.** Each of
    the 40 arms in `TaggedMethodResponseVisitor::visit_seq` ends with
    `.ok_or_else(|| serde::de::Error::custom("Expected a method response"))`,
    which no well-formed fixture can trigger. Deserialising `["<method name>"]`
    with the second element omitted hits exactly that closure, and because the
    `match` is exhaustive over `Method` with no catch-all, one loop over all
    40 variants covers every arm once. The same trick with a two-element array
    reaches the `"Expected method call id"` guard. This took `core/response.rs`
    from 96.07% to 99.45% in a single test.
89. **`tungstenite::Error::Protocol` renders with a doubled prefix.**
    `Error::WebSocket(tungstenite::Error::Protocol(ProtocolError::InvalidOpcode(9)))`
    displays as
    `"WebSockets error: WebSocket protocol error: Encountered invalid opcode: 9"`
    — the crate's own `"WebSockets error: "` and tungstenite's
    `"WebSocket protocol error: "` stacked. Any code that tries to strip or
    match the prefix has to account for both. Pinned by the
    `from_tungstenite_error_wraps_as_websocket_error` unit test.

90. **`Role::deserialize` reads a *borrowed* `&str`, so `from_value` cannot
    produce one.** `serde_json::from_value::<Role>(json!("inbox"))` fails with
    `invalid type: string "inbox", expected a borrowed string`, because the
    visitor calls `visit_str` against a `&'de str` lifetime that a `Value`
    cannot supply. Every `Role` fixture must go through
    `serde_json::from_str`. This bites hardest inside a larger record: a
    `Mailbox/set` response containing `"role": "inbox"` fails to
    deserialise wholesale under `from_value`, with the error pointing at the
    outer `.unwrap()` rather than the `role` member.
91. **`serde_json::from_str` appends a position suffix to custom visitor
    errors; `from_value` does not.** The same `Property` visitor rejection
    renders as `"unknown property"` from `from_value` but
    `"unknown property at line 1 column 14"` from `from_str`. Tests that
    assert on the exact message must pick one loader and stick to it; the
    P9 modules use `from_value` for custom-error assertions and reserve
    `from_str` for the `Role` case of Finding #90.
92. **`myRights` is modelled as the `mayXxx` object, not the RFC 8621 §7.1
    letter array.** RFC 8621 §7.1 spells rights as
    `["a","w","i","r","p","o"]`, but `MailboxRights` is a struct of nine
    `mayReadItems` / `mayAddItems` / … booleans. Feeding the RFC's array
    form fails with `invalid type: string "a", expected a boolean`. The
    `SetResponse<Mailbox<Get>>` fixture in `core/set.rs` therefore uses the
    object form and says so in its doc comment.
93. **`Address`, `EmailAddress` and `EmailAddressGroup` have no
    `skip_serializing_if` on their optional members.** An address with no
    display name serialises as `{"name": null, "email": "…"}`, and a
    submission `Address` always carries `"parameters": null`. RFC 8621
    §4.1 / §6.1 treat those members as absent, not null. Any fixture
    comparing whole serialised objects has to expect the explicit `null`,
    and a server that omits them will not round-trip byte-for-byte.
94. **`SetObject::new()` is `unimplemented!()` for every `<Get>` flavour.**
    `Email<Get>`, `EmailSubmission<Get>`, `Mailbox<Get>` and
    `Principal<Get>` each carry a `fn new(_) { unimplemented!() }` arm
    (two lines apiece, eight total) that no green run can reach: calling it
    panics, and nothing in the generic `SetRequest` machinery ever asks a
    `<Get>` object to construct itself. The `<Get>` records the tests need
    are built with `serde_json::from_value` instead.
95. **`Visitor::expecting()` is reached only by a *type*-mismatched input,
    never by an unknown string.** Feeding `"x-vendor-property"` to
    `Property`'s visitor goes through `visit_str` and returns the crate's
    own error; `expecting()` is never called. Feeding `json!(42)` makes
    serde fall back to `expecting()` to build
    `"invalid type: integer \`42\`, expected a valid JMAP e-mail property"`.
    Covering the three visitors in `email/mod.rs` needed the numeric input,
    which also pins the human-readable text each one promises.
96. **`from_timestamp` silently maps out-of-range input to the Unix epoch.**
    `DateTime::from_timestamp(ts, 0).unwrap_or_default()` means
    `from_timestamp(i64::MAX)` returns `1970-01-01T00:00:00Z` rather than
    an error, and `date_not_set` then reports that value as "not set". A
    caller passing a garbage timestamp gets the epoch, and a caller clearing
    a date by passing `0` is indistinguishable from one passing a real
    epoch value.
97. **`SetObject::set_id`'s default is a no-op, and the `<Get>` flavours
    inherit it.** Only the `<Set>` types override `set_id`; `Mailbox<Get>`
    and friends take the trait's `fn set_id(&mut self, _id) {}`, so writing
    an id back to a fetched record is silently dropped. That is the correct
    behaviour for a non-patch type, but it means `set_id` is not a uniform
    "assign the id" operation across the trait — a generic helper cannot
    rely on it.

