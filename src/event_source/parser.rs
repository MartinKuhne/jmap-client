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
        self.bytes = Some(bytes);
    }

    pub fn needs_bytes(&self) -> bool {
        self.bytes.is_none()
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
                            return Some(Err(crate::Error::Internal(
                                "EventSource response is too long.".to_string(),
                            )));
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
                            return Some(Err(crate::Error::Internal(
                                "EventSource response is too long.".to_string(),
                            )));
                        }

                        self.value.push(*byte);
                    }
                },
            }
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
