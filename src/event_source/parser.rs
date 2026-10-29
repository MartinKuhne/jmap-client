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

use super::Changes;
use crate::{
    event_source::{CalendarAlert, PushNotification},
    PushObject,
};

const MAX_EVENT_SIZE: usize = 1024 * 1024;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum EventType {
    Ping,
    #[default]
    State,
    CalendarAlert,
}

#[derive(Default, Debug)]
pub struct Event {
    pub event: EventType,
    pub id: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Debug, Copy, Clone, Default)]
enum EventParserState {
    #[default]
    Init,
    Comment,
    Field,
    Value,
}

#[derive(Default, Debug)]
pub struct EventParser {
    state: EventParserState,
    field: Vec<u8>,
    value: Vec<u8>,
    bytes: Option<Vec<u8>>,
    pos: usize,
    result: Event,
}

impl EventParser {
    pub fn push_bytes(&mut self, bytes: Vec<u8>) {
        // RFC 8620 §7.1: a frame may be cut at any byte, and the read
        // boundary is not an event boundary. Whatever of the previous read
        // has not been consumed yet is kept and the new bytes are appended
        // to it, so a frame split across reads is reassembled rather than
        // dropped. A read that was consumed to the end (or an error that
        // left `pos` past the buffer) starts a fresh buffer.
        let buffered = match self.bytes.take() {
            Some(mut pending) if self.pos < pending.len() => {
                pending.copy_within(self.pos.., 0);
                pending.extend_from_slice(&bytes);
                pending
            }
            _ => bytes,
        };
        self.pos = 0;
        self.bytes = Some(buffered);
    }

    pub fn needs_bytes(&self) -> bool {
        self.bytes.is_none()
    }

    /// Drops everything the parser holds mid-stream: the buffered bytes,
    /// the half-read field and value, and the event being assembled.
    ///
    /// Every error path calls this, so a failed frame is reported once and
    /// can neither repeat itself nor wedge the frames that follow it.
    fn reset(&mut self) {
        self.state = EventParserState::Init;
        self.field.clear();
        self.value.clear();
        self.result = Event::default();
        self.bytes = None;
        self.pos = 0;
    }

    pub fn filter_notification(&mut self) -> Option<crate::Result<PushNotification>> {
        #[allow(clippy::never_loop)]
        #[allow(clippy::while_let_on_iterator)]
        while let Some(event) = self.next() {
            match event {
                Ok(Event {
                    event: EventType::State,
                    data,
                    id,
                    ..
                }) => {
                    return match serde_json::from_slice::<PushObject>(&data) {
                        Ok(PushObject::StateChange { changed }) => {
                            Some(Ok(PushNotification::StateChange(Changes {
                                id: if !id.is_empty() {
                                    Some(String::from_utf8(id).unwrap_or_default())
                                } else {
                                    None
                                },
                                changes: changed,
                            })))
                        }
                        Ok(unexpected) => Some(Err(crate::Error::Internal(format!(
                            "Unexpected PushObject variant: {:?}",
                            unexpected
                        )))),
                        Err(err) => Some(Err(err.into())),
                    };
                }
                Ok(Event {
                    event: EventType::CalendarAlert,
                    data,
                    ..
                }) => {
                    return match serde_json::from_slice::<CalendarAlert>(&data) {
                        Ok(calendar_alert) => {
                            Some(Ok(PushNotification::CalendarAlert(calendar_alert)))
                        }
                        Err(err) => Some(Err(err.into())),
                    };
                }
                Ok(Event {
                    event: EventType::Ping,
                    #[cfg(feature = "debug")]
                    id,
                    ..
                }) => {
                    #[cfg(feature = "debug")]
                    return Some(Ok(PushNotification::StateChange(Changes {
                        id: if !id.is_empty() {
                            Some(String::from_utf8(id).unwrap_or_default())
                        } else {
                            None
                        },
                        changes: ahash::AHashMap::from_iter([(
                            "ping".to_string(),
                            ahash::AHashMap::new(),
                        )]),
                    })));
                }
                Err(err) => return Some(Err(err)),
            }
        }
        None
    }
}

impl Iterator for EventParser {
    type Item = crate::Result<Event>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.bytes.as_ref()?;
        // The oversized-event error cannot be returned from inside the loop:
        // the loop borrows the buffer, and the state has to be reset before
        // the error leaves. So it is flagged, the scan stops, and the reset
        // and the error are done together once the borrow is over.
        let mut oversized = false;

        for byte in bytes.get(self.pos..)? {
            self.pos += 1;

            match self.state {
                EventParserState::Init => match byte {
                    b':' => {
                        self.state = EventParserState::Comment;
                    }
                    b'\r' | b' ' => (),
                    b'\n' => {
                        return Some(Ok(std::mem::take(&mut self.result)));
                    }
                    _ => {
                        self.state = EventParserState::Field;
                        self.field.push(*byte);
                    }
                },
                EventParserState::Comment => {
                    if *byte == b'\n' {
                        self.state = EventParserState::Init;
                    }
                }
                EventParserState::Field => match byte {
                    b'\r' => (),
                    b'\n' => {
                        self.state = EventParserState::Init;
                        self.field.clear();
                    }
                    b':' => {
                        self.state = EventParserState::Value;
                    }
                    _ => {
                        if self.field.len() >= MAX_EVENT_SIZE {
                            oversized = true;
                            break;
                        }

                        self.field.push(*byte);
                    }
                },
                EventParserState::Value => match byte {
                    b'\r' => (),
                    b' ' if self.value.is_empty() => (),
                    b'\n' => {
                        self.state = EventParserState::Init;
                        match &self.field[..] {
                            b"id" => {
                                self.result.id.extend_from_slice(&self.value);
                            }
                            b"data" => {
                                self.result.data.extend_from_slice(&self.value);
                            }
                            b"event" => match &self.value[..] {
                                b"calendarAlert" => {
                                    self.result.event = EventType::CalendarAlert;
                                }
                                b"ping" => {
                                    self.result.event = EventType::Ping;
                                }
                                _ => {
                                    self.result.event = EventType::State;
                                }
                            },
                            _ => {
                                //ignore
                            }
                        }
                        self.field.clear();
                        self.value.clear();
                    }
                    _ => {
                        if (self.field.len() + self.value.len()) >= MAX_EVENT_SIZE {
                            oversized = true;
                            break;
                        }

                        self.value.push(*byte);
                    }
                },
            }
        }

        if oversized {
            // The frame is undeliverable and its remaining bytes are
            // worthless: drop the buffer along with the half-read field,
            // value and event, so this error is handed out exactly once and
            // the next read starts from a clean parser. Whatever else was
            // buffered behind the oversized frame goes with it — a stream
            // that sends a 1 MiB event has already broken what the limit
            // protects.
            self.reset();
            return Some(Err(crate::Error::Internal(
                "EventSource response is too long.".to_string(),
            )));
        }

        self.bytes = None;
        self.pos = 0;

        None
    }
}

#[cfg(test)]
mod tests {

    use super::{Event, EventType};

    #[derive(Debug, PartialEq, Eq)]
    struct EventString {
        event: EventType,
        id: String,
        data: String,
    }

    impl From<Event> for EventString {
        fn from(event: Event) -> Self {
            Self {
                event: event.event,
                id: String::from_utf8(event.id).unwrap(),
                data: String::from_utf8(event.data).unwrap(),
            }
        }
    }

    #[test]
    fn parse() {
        let mut parser = super::EventParser::default();
        let mut results = Vec::new();

        for frame in [
            Vec::from("event: state\nid:  0\ndata: test\n\n"),
            Vec::from("event: ping\nid:123\ndata: ping pa"),
            Vec::from("yload"),
            Vec::from("\n\n"),
            Vec::from(":comment\n\n"),
            Vec::from("data: YHOO\n"),
            Vec::from("data: +2\n"),
            Vec::from("data: 10\n\n"),
            Vec::from(": test stream\n"),
            Vec::from("data: first event\n"),
            Vec::from("id: 1\n\n"),
            Vec::from("data:second event\n"),
            Vec::from("id\n\n"),
            Vec::from("data:  third event\n\n"),
            Vec::from("data:hello\n\ndata: world\n\n"),
        ] {
            parser.push_bytes(frame);

            #[allow(clippy::while_let_on_iterator)]
            while let Some(event) = parser.next() {
                results.push(EventString::from(event.unwrap()));
            }
        }

        assert_eq!(
            results,
            vec![
                EventString {
                    event: EventType::State,
                    id: "0".to_string(),
                    data: "test".to_string()
                },
                EventString {
                    event: EventType::Ping,
                    id: "123".to_string(),
                    data: "ping payload".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "YHOO+210".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "1".to_string(),
                    data: "first event".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "second event".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "third event".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "hello".to_string()
                },
                EventString {
                    event: EventType::State,
                    id: "".to_string(),
                    data: "world".to_string()
                }
            ]
        );
    }

    use super::EventParser;
    use crate::{
        event_source::{CalendarAlert, Changes, PushNotification},
        DataType, Error, PushObject,
    };
    use ahash::AHashMap;

    /// A parser with one SSE chunk already buffered.
    fn parser_with(chunk: &str) -> EventParser {
        let mut parser = EventParser::default();
        parser.push_bytes(chunk.as_bytes().to_vec());
        parser
    }

    /// RFC 8620 §7.1 — the `StateChange` payload every test below feeds
    /// through the parser: one account, one changed data type.
    const STATE_CHANGE: &str =
        r#"{"@type":"StateChange","changed":{"u33084183":{"Mailbox":"51a0c6b1-1"}}}"#;

    fn one_mailbox_change() -> AHashMap<String, AHashMap<DataType, String>> {
        AHashMap::from_iter([(
            "u33084183".to_string(),
            AHashMap::from_iter([(DataType::Mailbox, "51a0c6b1-1".to_string())]),
        )])
    }

    /// `needs_bytes` is the parser's "give me more" flag: true while
    /// nothing is buffered, false while a chunk is in hand, and true
    /// again once that chunk has been read to the end (Finding #79).
    #[test]
    fn needs_bytes_tracks_whether_a_chunk_is_buffered() {
        let mut parser = EventParser::default();
        assert!(parser.needs_bytes(), "a fresh parser has nothing buffered");

        parser.push_bytes(b"data: {}".to_vec());
        assert!(
            !parser.needs_bytes(),
            "a buffered chunk satisfies the parser"
        );

        // The chunk holds no blank line, so no event dispatches; reading
        // it to the end releases the buffer.
        assert!(parser.next().is_none());
        assert!(parser.needs_bytes(), "the chunk is released once consumed");
    }

    /// RFC 8620 §7.1 — the SSE `id:` field becomes the notification's
    /// resume token, and its absence is reported as `None`, not as an
    /// empty string.
    #[test]
    fn a_state_change_reports_its_sse_id_only_when_one_was_sent() {
        let mut parser = parser_with(&format!("data: {STATE_CHANGE}\n\n"));
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(None, one_mailbox_change())),
            "no `id:` field means no resume token"
        );

        let mut parser = parser_with(&format!("id: 42\ndata: {STATE_CHANGE}\n\n"));
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("42".to_string()),
                one_mailbox_change()
            )),
            "the id is carried through verbatim"
        );
    }

    /// Finding #16 — `filter_notification` understands only
    /// `StateChange` and `CalendarAlert`. Any other `PushObject`, such
    /// as the `EmailPush` the mock happily sends over SSE, surfaces as
    /// an internal error that names the variant that arrived.
    #[test]
    fn an_unexpected_push_object_variant_is_an_internal_error() {
        let payload = r#"{"@type":"EmailPush","accountId":"u33084183","email":{"id":"M1"}}"#;
        let parsed = serde_json::from_str::<PushObject>(payload);
        let decoded = matches!(parsed, Ok(PushObject::EmailPush { .. }));
        assert!(decoded, "the payload is a perfectly valid PushObject");

        // an EmailPush over SSE must not look like a state change
        let message = parser_with(&format!("id: 5\ndata: {payload}\n\n"))
            .filter_notification()
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("Unexpected PushObject variant"),
            "the error says what went wrong: {message}"
        );
        assert!(message.contains("EmailPush"), "and what arrived: {message}");
    }

    /// A `data:` payload that is not valid JSON surfaces as a parse
    /// error instead of being skipped over.
    #[test]
    fn malformed_json_in_a_state_event_is_a_parse_error() {
        // a truncated JSON payload must not parse
        let error = parser_with("data: {\"@type\":\"StateChange\",\"changed\":\n\n")
            .filter_notification()
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("Parse error: "),
            "the SSE layer keeps the serde_json failure: {error}"
        );
    }

    /// The `CalendarAlert` arm: `event: calendarAlert` selects it and
    /// the payload decodes into the alert object.
    #[test]
    fn a_calendar_alert_event_decodes_into_the_alert_object() {
        let mut parser = parser_with(
            "event: calendarAlert\n\
             id: 7\n\
             data: {\"accountId\":\"u33084183\",\"calendarEventId\":\"ce-1\",\
             \"uid\":\"uid-42\",\"recurrenceId\":\"2026-10-29T08:00:00Z\",\"alertId\":\"a-9\"}\n\n",
        );
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::CalendarAlert(CalendarAlert {
                account_id: "u33084183".to_string(),
                calendar_event_id: "ce-1".to_string(),
                uid: "uid-42".to_string(),
                recurrence_id: Some("2026-10-29T08:00:00Z".to_string()),
                alert_id: "a-9".to_string(),
            })
        );

        // `recurrenceId` is optional on the alert itself
        let mut parser = parser_with(
            "event: calendarAlert\n\
             data: {\"accountId\":\"u33084183\",\"calendarEventId\":\"ce-2\",\
             \"uid\":\"uid-43\",\"recurrenceId\":null,\"alertId\":\"a-10\"}\n\n",
        );
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::CalendarAlert(CalendarAlert {
                account_id: "u33084183".to_string(),
                calendar_event_id: "ce-2".to_string(),
                uid: "uid-43".to_string(),
                recurrence_id: None,
                alert_id: "a-10".to_string(),
            })
        );
    }

    /// The other half of the `CalendarAlert` arm: a payload that does
    /// not decode is a parse error, not a silently dropped event.
    #[test]
    fn malformed_json_in_a_calendar_alert_is_a_parse_error() {
        // a calendar alert with a non-string account must not parse
        let error = parser_with("event: calendarAlert\ndata: {\"accountId\":42}\n\n")
            .filter_notification()
            .unwrap()
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("Parse error: "),
            "the alert's own serde failure is what surfaces: {error}"
        );
    }

    /// An SSE field the parser does not know — `retry:` is the canonical
    /// one — is dropped without disturbing the rest of the event.
    #[test]
    fn an_unknown_field_is_dropped_and_the_rest_of_the_event_survives() {
        let mut parser = parser_with(&format!("retry: 3000\ndata: {STATE_CHANGE}\n\n"));
        let event = parser.next().unwrap().unwrap();
        assert_eq!(event.event, EventType::State, "the default event type");
        assert!(event.id.is_empty(), "`retry:` never reaches the id");
        assert_eq!(event.data, STATE_CHANGE.as_bytes());
    }

    /// A bare CR is dropped in every state, so CRLF framing parses
    /// exactly like LF framing — including a CR inside a field name.
    #[test]
    fn carriage_returns_are_dropped_so_crlf_parses_like_lf() {
        let mut parser = parser_with(&format!("id\r: 7\r\ndata: {STATE_CHANGE}\r\n\r\n"));
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("7".to_string()),
                one_mailbox_change()
            )),
            "the CR inside `id\\r:` is ignored and the event still dispatches"
        );
    }

    /// The parser refuses to buffer more than `MAX_EVENT_SIZE` (1 MiB)
    /// in a field name, which is what stops a runaway stream from
    /// eating memory — and the error comes back out of
    /// `filter_notification` rather than ending the iteration.
    #[test]
    fn an_oversized_field_name_is_rejected() {
        const MAX_EVENT_SIZE: usize = 1024 * 1024;

        let mut parser = EventParser::default();
        parser.push_bytes(vec![b'x'; MAX_EVENT_SIZE + 1]);
        // a field longer than the event limit must fail
        let error = parser.next().unwrap().unwrap_err().to_string();
        assert_eq!(
            error, "Internal error: EventSource response is too long.",
            "the limit is reported, not swallowed"
        );

        let mut parser = EventParser::default();
        parser.push_bytes(vec![b'x'; MAX_EVENT_SIZE + 1]);
        let reached = matches!(
            parser.filter_notification().unwrap(),
            Err(Error::Internal(_))
        );
        assert!(reached, "the same error reaches the notification filter");
    }

    /// The value side of the same limit: `field.len() + value.len()` is
    /// what is measured, so a long value under a short field name trips
    /// it just the same.
    #[test]
    fn an_oversized_field_value_is_rejected() {
        const MAX_EVENT_SIZE: usize = 1024 * 1024;

        let mut chunk = b"data:".to_vec();
        chunk.resize(MAX_EVENT_SIZE + 5, b'y');
        let mut parser = EventParser::default();
        parser.push_bytes(chunk);

        // a value longer than the event limit must fail
        let error = parser
            .filter_notification()
            .unwrap()
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Internal error: EventSource response is too long.");
    }

    /// The two halves of `frame`, cut at the first byte of `marker`: the way
    /// a transport read can cut an event in the middle of its payload.
    fn split_frame(frame: &str, marker: &str) -> (Vec<u8>, Vec<u8>) {
        let cut = frame.find(marker).unwrap();
        let bytes = frame.as_bytes();
        (bytes[..cut].to_vec(), bytes[cut..].to_vec())
    }

    /// RFC 8620 §7.1 — a `StateChange` frame may be cut at any byte by the
    /// transport: the read boundary is not an event boundary. Feeding the
    /// frame in two reads must still produce exactly one complete event.
    #[test]
    fn a_state_change_split_across_two_reads_is_parsed_once_and_completely() {
        // Cut in the middle of the JSON payload, where the parser is
        // mid-value with a half-read `data:` field.
        let (head, tail) = split_frame(&format!("id: 7\ndata: {STATE_CHANGE}\n\n"), "StateChange");

        let mut parser = EventParser::default();
        parser.push_bytes(head);
        assert!(
            parser.filter_notification().is_none(),
            "a half frame must not dispatch anything"
        );
        assert!(
            parser.needs_bytes(),
            "having read the half frame to the end, the parser asks for more"
        );

        parser.push_bytes(tail);
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("7".to_string()),
                one_mailbox_change()
            )),
            "the two reads make one complete notification"
        );
        assert!(
            parser.filter_notification().is_none(),
            "and the notification is delivered exactly once"
        );
    }

    /// `push_bytes` is public API, and a caller that hands over the next
    /// read before the previous one has been drained must not have the
    /// unread bytes thrown away: what has not been consumed is kept.
    #[test]
    fn bytes_that_have_not_been_consumed_are_kept_across_pushes() {
        let (head, tail) = split_frame(&format!("id: 7\ndata: {STATE_CHANGE}\n\n"), "StateChange");

        let mut parser = EventParser::default();
        parser.push_bytes(head);
        // No `next()` in between: the first read is still entirely pending.
        parser.push_bytes(tail);

        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("7".to_string()),
                one_mailbox_change()
            )),
            "the second read completes the first instead of replacing it"
        );
        assert!(
            parser.filter_notification().is_none(),
            "and nothing is delivered twice"
        );
    }

    /// The same retention mid-event: a read that ends inside a field
    /// leaves that field half-read, and the following read continues it.
    #[test]
    fn a_push_after_a_partial_read_continues_the_half_read_field() {
        let (head, tail) = split_frame(&format!("id: 7\ndata: {STATE_CHANGE}\n\n"), "StateChange");

        let mut parser = EventParser::default();
        parser.push_bytes(head);
        // Drain the partial read; it holds no blank line, so nothing is
        // dispatched and the half-read `data:` field stays in place.
        assert!(parser.next().is_none());

        parser.push_bytes(tail);
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("7".to_string()),
                one_mailbox_change()
            )),
            "the value picks up where the previous read stopped"
        );
    }

    /// An oversized event is one error, not a permanent one: once the
    /// offending frame is past, the next well-formed frame must be
    /// delivered. Without a reset the limit-tripping `field`/`value`/`state`
    /// and a `pos` past the end of the buffer are left behind, so every later
    /// frame either trips the same limit again or is never read at all.
    #[test]
    fn the_parser_recovers_after_an_oversized_event() {
        const MAX_EVENT_SIZE: usize = 1024 * 1024;

        let mut parser = EventParser::default();
        parser.push_bytes(vec![b'x'; MAX_EVENT_SIZE + 1]);
        assert_eq!(
            parser
                .filter_notification()
                .unwrap()
                .unwrap_err()
                .to_string(),
            "Internal error: EventSource response is too long.",
            "the oversized frame is reported"
        );

        // A well-formed frame straight after the bad one must get through.
        parser.push_bytes(
            format!("id: 8\ndata: {STATE_CHANGE}\n\n")
                .as_bytes()
                .to_vec(),
        );
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("8".to_string()),
                one_mailbox_change()
            )),
            "the parser recovers instead of repeating the error"
        );
    }

    /// The other half of recovery: the error must not be re-emitted for
    /// the bytes that are still buffered behind it, and the parser must
    /// not stay wedged once the frame is over.
    #[test]
    fn an_oversized_event_error_is_reported_once_and_the_parser_is_reset() {
        const MAX_EVENT_SIZE: usize = 1024 * 1024;

        let mut chunk = b"data:".to_vec();
        chunk.resize(MAX_EVENT_SIZE + 5, b'y');
        let mut parser = EventParser::default();
        parser.push_bytes(chunk);

        assert_eq!(
            parser.next().unwrap().unwrap_err().to_string(),
            "Internal error: EventSource response is too long."
        );
        assert!(
            parser.needs_bytes(),
            "the failed frame is dropped, so the parser asks for a new one"
        );
        assert!(
            parser.next().is_none(),
            "the same error is not handed out again for the same bytes"
        );
    }

    /// Finding #81 — without the `debug` feature a `ping` event is
    /// swallowed whole: the parser consumes it and reports nothing at
    /// all, so a caller cannot tell a keepalive from a wedged stream.
    #[test]
    fn a_ping_event_is_swallowed_unless_debug_is_on() {
        let mut parser = parser_with("event: ping\nid: 9\ndata: keepalive\n\n");

        #[cfg(not(feature = "debug"))]
        assert!(
            parser.filter_notification().is_none(),
            "a ping produces no notification without the `debug` feature"
        );

        #[cfg(feature = "debug")]
        assert_eq!(
            parser.filter_notification().unwrap().unwrap(),
            PushNotification::StateChange(Changes::new(
                Some("9".to_string()),
                AHashMap::from_iter([("ping".to_string(), AHashMap::new())])
            )),
            "with `debug` on the ping arrives as a state change keyed `ping`"
        );
    }
}
