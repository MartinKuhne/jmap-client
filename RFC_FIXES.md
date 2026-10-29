# RFC compliance fixes — branch `fix/rfc-compliance`

Branched from `coverage/rfc-mock-tests` @ `141b784`.

The coverage work surfaced a set of places where `src/` behaves in a way the JMAP
RFCs say is wrong. This branch fixes them. The RFCs are the authority:
RFC 8620 (Core), RFC 8621 (Mail), RFC 8887 (WebSocket), RFC 9670 (Principals),
draft-ietf-jmap-sieve-12.

## Rules for this branch

* Every fix cites the normative RFC text it satisfies.
* Tests that **pinned the buggy behaviour** are rewritten to assert the
  RFC-correct behaviour — never deleted, never weakened.
* A fixture that had to lie to work around a bug is corrected at the same time
  as the bug (see `tests/common/mod.rs`).
* Prefer a backwards-compatible fix when one exists; when the correct fix is a
  breaking change, say so explicitly in the commit message.
* One batch per commit. Full suite + clippy green before each commit.

## Batch plan

| Batch | Bugs | Fix | Breaking? |
|---|---|---|---|
| B1 | #112 #27 #7 #36 | unknown method-error type swallowed as an empty success; content-type byte-compared; doc example emits invalid `maxChanges: 0` | enum extension only |
| B1b | #112 root cause | untagged `SingleMethodResponse` still absorbs a malformed error (`type` missing/non-string) as an all-`None` `SetResponse` | no |
| B2 | #40 #54 #66 | `send_single` pops the *last* response instead of the matching one; `GetResponse.state` required though `PushSubscription/get` never returns it | no |
| B3 | #79 #108 | SSE parser replaces its buffer per chunk and never resets after an oversized-event error | no |
| B4 | #74 #119 #75 | sieve deactivation sends a member that does not exist in the draft; sieve upload content type | yes (wire + builder) |
| B5 | #2 #30 #122 #3 | `maxMailboxDepth` not nullable; `submissionExtensions` modelled as an array not a map; session-level vs account-level capability limits | yes |
| B6 | #69 #67 #70 | RFC 9670 principal spellings: `timeZone`, `shareWith`, `capabilities` as a map | yes (wire) |
| B7 | #73 #80 | `DataType` closed enum kills push/subscription parsing; WS `pushState` parsed then discarded | yes |
| B8 | #104 | `result_reference` can reference its own call id | no |

## Checked and deliberately NOT treated as bugs

* **Server-side MUSTs** (#120, #121): "a server MUST NOT return `url`/`keys`"
  constrains servers; the client being permissive on read is allowed.
* **#15/#16** `EmailPush`/`CalendarAlert` over SSE: neither appears in RFC 8620
  or 8621; §7.1 defines push data as a `StateChange` object only. The mock is
  the thing that invents them, not the client.
* **#85** `requestId` is a WebSocket-envelope member, not an HTTP one.
* **#92** `myRights` is an object of `mayXxx` booleans in RFC 8621 §2.1; the
  client matches.
* **#60/#118** `using` is built by filtering a fixed candidate list against what
  the session advertises, so the client never opts in to a capability the server
  withheld. Correct.
* **#114** draft-sieve §2.4 really does specify a body-level error here.

## Status log

* **B1 done.** `MethodErrorType` gains an `Other(String)` catch-all (hand-written
  `Deserialize`; `#[serde(other)]` is rejected on a non-unit variant) plus
  `is_server_fail_equivalent()` so RFC 8620 §3.6.2's "treat unknown as
  serverFail" MUST is actually reachable. `handle_error` now compares the media
  type rather than the whole `Content-Type` byte string, so
  `application/problem+json; charset=utf-8` yields `Error::Problem` instead of a
  bare `Error::Server`. The `max_changes(0)` doc example (invalid per §5.2) is
  corrected in `src/lib.rs`, `examples/result_reference.rs` and `README.md`.
  **Breaking:** exhaustive matches on `MethodErrorType` without a `_` arm stop
  compiling. 617 passed / 0 failed.
* **B1b done** (#112 root cause). `SingleMethodResponse` is no longer `#[serde(untagged)]`.
  A hand-written `Deserialize` (a `Visitor` over `SeqAccess`) decides the arm from
  the *first element* of the Invocation array — `"error"` → `Error`, anything else
  → `Ok` — and only then deserialises the payload, so a malformed method error can
  never be absorbed as an all-`None` `SetResponse`. RFC 8620 §3.2 fixes an
  Invocation at `[name, arguments, callId]`; §3.6.2 defines the error form as
  `[ "error", { "type": "unknownMethod" }, "call-id" ]` and says "The response
  name is "error", and it MUST have a type property." An untagged enum cannot
  express that: it tries the `Error` arm and, on *any* failure of that arm,
  silently retries the `Ok` arm — which binds element 0 as a bare `String` — so
  `["error", {}, "0"]` came back as a successful `SetResponse` with every member
  absent. A missing or empty array, a non-string method name, an `error` response
  whose arguments are not a `MethodError` object, and a call id that is not a
  string are all hard errors now. The enum's public shape is unchanged
  (non-breaking); a `call_id()` accessor was added so callers can match on the
  request id without destructuring.
  `tests/core_methods.rs::a_malformed_method_error_on_a_set_response_is_never_a_silent_success`
  drives all five malformed shapes end to end through `send_set_mailbox()`.
* **B2 done** (#40 #54 #66). `Request::send_single` returns the **first** response
  whose call id equals the request's, not the last one in the array. RFC 8620 §3.2:
  a method "may return 1 or more responses, as it may make implicit calls to other
  methods; all responses initiated by this method call get the same method call id
  in the response", and §3.4 requires outputs to be added "in the same order that
  the methods are processed" — so the primary answer is first and the implicit one
  after it. RFC 8621 §7.5 makes `EmailSubmission/set` issue a single implicit
  `Email/set` whose response "MUST be returned after the EmailSubmission/set
  response", and §7.5.1 shows both answers under one call id; RFC 8620 §5.4
  (`/copy` with `onSuccessDestroyOriginal`, which RFC 8621 §4.7 adopts for
  `Email/copy`) has the same shape. Reading the tail handed the caller the
  implicit body and turned a copy or submission that *succeeded* into a parse
  error. Only the selected response's arguments are deserialised into `T` now, so
  an implicit secondary body of a different type can no longer poison the envelope
  before the selection happens. `GetResponse.state` is `#[serde(default)]`:
  RFC 8620 §7.2.1 says of `PushSubscription/get` that "It also does *not* return a
  "state" argument", and the §7.2.3 example response carries only `list` and
  `notFound`, so requiring the member made a compliant response fail to parse; the
  `tests/push_subscription.rs` fixture that injected a `"state"` to work around
  this is corrected. **Not breaking** for the documented single-call usage; the
  tests that pinned the old behaviour (`tests/email.rs`,
  `tests/email_submission.rs`, `tests/search_snippet.rs`) are rewritten to assert
  the RFC behaviour, none deleted, and `tests/sieve.rs` gains the
  extra-response-under-one-call-id case. 624 passed / 0 failed.
