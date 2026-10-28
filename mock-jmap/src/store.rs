//! The mock's in-memory data store.
//!
//! Records are kept as canonical JMAP JSON objects ([`serde_json::Value`]) so a
//! handler can hand them straight to the response writer without a second model
//! of every domain type. Coherence between types (a mailbox's counters, a
//! thread's `emailIds`, an email's `mailboxIds`) is established by the
//! fixtures in [`Store::new`] and maintained by the Set handlers.
//!
//! Every [`Collection`] carries a monotonically increasing state plus a change
//! history, which is what makes `*/changes` and `*/queryChanges` answer with
//! advancing `oldState`/`newState` values instead of static strings.

use serde_json::{json, Map, Value};

use crate::session::ACCOUNT_ID;

/// One state transition of a collection.
#[derive(Debug, Clone)]
struct HistoryEntry {
    state: u64,
    created: Vec<String>,
    updated: Vec<String>,
    destroyed: Vec<String>,
}

/// An ordered set of JMAP objects of one type, with change tracking.
#[derive(Debug, Clone)]
pub struct Collection {
    prefix: &'static str,
    items: Vec<(String, Value)>,
    next_seq: u64,
    state: u64,
    history: Vec<HistoryEntry>,
}

impl Collection {
    /// Creates an empty collection whose generated ids start with `prefix`.
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            items: Vec::new(),
            next_seq: 1,
            state: 1,
            history: Vec::new(),
        }
    }

    /// Inserts a record under an explicit, deterministic id (fixtures only).
    pub fn seed(&mut self, id: &str, value: Value) {
        self.items.push((id.to_string(), value));
    }

    /// Creates a record with a generated id and returns the id.
    pub fn create(&mut self, mut value: Value) -> String {
        let id = format!("{}{}", self.prefix, self.next_seq);
        self.next_seq += 1;
        if let Some(map) = value.as_object_mut() {
            map.insert("id".to_string(), json!(id));
        }
        self.items.push((id.clone(), value));
        id
    }

    /// Looks a record up by id.
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.items.iter().find(|(key, _)| key == id).map(|(_, v)| v)
    }

    /// All ids, in insertion order.
    pub fn ids(&self) -> Vec<String> {
        self.items.iter().map(|(id, _)| id.clone()).collect()
    }

    /// All records, in insertion order.
    pub fn all(&self) -> Vec<Value> {
        self.items.iter().map(|(_, value)| value.clone()).collect()
    }

    /// Records matching `predicate`, as `(id, value)` pairs.
    pub fn filter(&self, predicate: impl Fn(&Value) -> bool) -> Vec<(String, Value)> {
        self.items
            .iter()
            .filter(|(_, value)| predicate(value))
            .cloned()
            .collect()
    }

    /// `true` when a record with `id` exists.
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Current state as a string.
    pub fn state_str(&self) -> String {
        self.state.to_string()
    }

    /// Applies a JMAP patch (set fields, `null` unsets) to an existing record
    /// and returns the updated record.
    pub fn update(&mut self, id: &str, patch: &Value) -> Option<Value> {
        let entry = self.items.iter_mut().find(|(key, _)| key == id)?;
        apply_patch(&mut entry.1, patch);
        Some(entry.1.clone())
    }

    /// Replaces a record wholesale.
    pub fn replace(&mut self, id: &str, value: Value) -> Option<Value> {
        let entry = self.items.iter_mut().find(|(key, _)| key == id)?;
        entry.1 = value;
        Some(entry.1.clone())
    }

    /// Removes a record; `true` when it existed.
    pub fn destroy(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|(key, _)| key != id);
        self.items.len() != before
    }

    /// Records a completed mutation group and returns the new state.
    ///
    /// The state advances by one per Set call, which is what lets a test assert
    /// that `oldState`/`newState` really move.
    pub fn record_change(
        &mut self,
        created: Vec<String>,
        updated: Vec<String>,
        destroyed: Vec<String>,
    ) -> u64 {
        if created.is_empty() && updated.is_empty() && destroyed.is_empty() {
            return self.state;
        }
        self.state += 1;
        self.history.push(HistoryEntry {
            state: self.state,
            created,
            updated,
            destroyed,
        });
        self.state
    }

    /// Seeds the initial "everything was created at state 1" history entry.
    pub fn record_initial(&mut self) {
        self.history.push(HistoryEntry {
            state: self.state,
            created: self.ids(),
            updated: Vec::new(),
            destroyed: Vec::new(),
        });
    }

    /// The changes since `since_state`.
    ///
    /// `Err(())` means the state is unknown to the mock (unparseable or ahead of
    /// the current state), which callers turn into `cannotCalculateChanges`.
    pub fn changes_since(
        &self,
        since_state: &str,
    ) -> Result<(Vec<String>, Vec<String>, Vec<String>), ()> {
        let since = since_state.parse::<u64>().map_err(|_| ())?;
        if since > self.state {
            return Err(());
        }
        let mut created = Vec::new();
        let mut updated = Vec::new();
        let mut destroyed = Vec::new();
        for entry in self.history.iter().filter(|entry| entry.state > since) {
            for id in &entry.created {
                push_unique(&mut created, id);
            }
            for id in &entry.updated {
                push_unique(&mut updated, id);
            }
            for id in &entry.destroyed {
                push_unique(&mut destroyed, id);
            }
        }
        // An object created and later updated in the window is simply "created".
        updated.retain(|id| !created.contains(id) && !destroyed.contains(id));
        created.retain(|id| !destroyed.contains(id));
        Ok((created, updated, destroyed))
    }

    /// Ids that were destroyed since `since_state` (used by `*/queryChanges`).
    pub fn destroyed_since(&self, since_state: u64) -> Vec<String> {
        let mut out = Vec::new();
        for entry in self
            .history
            .iter()
            .filter(|entry| entry.state > since_state)
        {
            for id in &entry.destroyed {
                push_unique(&mut out, id);
            }
        }
        out
    }
}

fn push_unique(list: &mut Vec<String>, id: &str) {
    if !list.iter().any(|existing| existing == id) {
        list.push(id.to_string());
    }
}

/// A stored blob: raw bytes plus the media type it was uploaded with.
#[derive(Debug, Clone)]
pub struct BlobRecord {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// Blob storage backing the upload/download routes and `Blob/copy`.
#[derive(Debug, Clone, Default)]
pub struct BlobStore {
    items: Vec<(String, BlobRecord)>,
    next_seq: u64,
}

impl BlobStore {
    /// Stores `bytes` and returns the generated blob id.
    pub fn put(&mut self, bytes: Vec<u8>, content_type: impl Into<String>) -> String {
        let id = format!("blob_{}", self.next_seq);
        self.next_seq += 1;
        self.items.push((
            id.clone(),
            BlobRecord {
                bytes,
                content_type: content_type.into(),
            },
        ));
        id
    }

    /// Looks a blob up by id.
    pub fn get(&self, id: &str) -> Option<&BlobRecord> {
        self.items.iter().find(|(key, _)| key == id).map(|(_, v)| v)
    }

    /// Copies a blob under a fresh id; `None` when the source is unknown.
    pub fn copy(&mut self, id: &str) -> Option<String> {
        let blob = self.get(id)?.clone();
        Some(self.put(blob.bytes, blob.content_type))
    }

    /// All blob ids.
    pub fn ids(&self) -> Vec<String> {
        self.items.iter().map(|(id, _)| id.clone()).collect()
    }
}

/// The whole mock dataset.
#[derive(Debug, Clone)]
pub struct Store {
    pub mailboxes: Collection,
    pub threads: Collection,
    pub emails: Collection,
    pub identities: Collection,
    pub submissions: Collection,
    pub vacation: Collection,
    pub sieve_scripts: Collection,
    pub principals: Collection,
    pub push_subscriptions: Collection,
    pub blobs: BlobStore,
    session_state: u64,
}

impl Store {
    /// Builds the store with the RFC-realistic fixture set.
    pub fn new() -> Self {
        let mut store = Self::empty();
        store.seed_fixtures();
        store
    }

    /// Builds an empty store (no fixtures).
    pub fn empty() -> Self {
        Self {
            mailboxes: Collection::new("mb"),
            threads: Collection::new("th"),
            emails: Collection::new("em"),
            identities: Collection::new("id"),
            submissions: Collection::new("sub"),
            vacation: Collection::new("vr"),
            sieve_scripts: Collection::new("sc"),
            principals: Collection::new("pr"),
            push_subscriptions: Collection::new("push"),
            blobs: BlobStore::default(),
            session_state: 1,
        }
    }

    /// Current session state as a string.
    pub fn session_state_str(&self) -> String {
        self.session_state.to_string()
    }

    /// Marks the account as modified; the session state advances so clients
    /// notice they hold a stale session (RFC 8620 section 3.4).
    pub fn touch(&mut self) {
        self.session_state += 1;
    }

    /// The collection for a JMAP type name (`"Mailbox"`, `"Email"`, ...).
    pub fn collection(&self, type_name: &str) -> Option<&Collection> {
        match type_name {
            "Mailbox" => Some(&self.mailboxes),
            "Thread" => Some(&self.threads),
            "Email" => Some(&self.emails),
            "Identity" => Some(&self.identities),
            "EmailSubmission" => Some(&self.submissions),
            "VacationResponse" => Some(&self.vacation),
            "SieveScript" => Some(&self.sieve_scripts),
            "Principal" => Some(&self.principals),
            "PushSubscription" => Some(&self.push_subscriptions),
            _ => None,
        }
    }

    /// Mutable variant of [`Store::collection`].
    pub fn collection_mut(&mut self, type_name: &str) -> Option<&mut Collection> {
        match type_name {
            "Mailbox" => Some(&mut self.mailboxes),
            "Thread" => Some(&mut self.threads),
            "Email" => Some(&mut self.emails),
            "Identity" => Some(&mut self.identities),
            "EmailSubmission" => Some(&mut self.submissions),
            "VacationResponse" => Some(&mut self.vacation),
            "SieveScript" => Some(&mut self.sieve_scripts),
            "Principal" => Some(&mut self.principals),
            "PushSubscription" => Some(&mut self.push_subscriptions),
            _ => None,
        }
    }

    fn seed_fixtures(&mut self) {
        // --- blobs -------------------------------------------------------
        let blob_em_1 = self.blobs.put(
            b"From: Alice Example <alice@example.com>\r\n\
               To: test@example.org\r\n\
               Subject: Welcome to JMAP\r\n\
               Date: Mon, 05 Jan 2026 09:00:00 +0000\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               Your new JMAP account is ready. Say hello!\r\n"
                .to_vec(),
            "message/rfc822",
        );
        let blob_em_2 = self.blobs.put(
            b"From: Bob Example <bob@example.com>\r\n\
               To: test@example.org\r\n\
               Subject: Re: Welcome to JMAP\r\n\
               Date: Mon, 05 Jan 2026 10:30:00 +0000\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               Thanks for the warm welcome.\r\n"
                .to_vec(),
            "message/rfc822",
        );
        let blob_em_3 = self.blobs.put(
            b"From: test@example.org\r\n\
               To: bob@example.com\r\n\
               Subject: Project status\r\n\
               Date: Tue, 06 Jan 2026 08:15:00 +0000\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               The project is on track.\r\n"
                .to_vec(),
            "message/rfc822",
        );
        let blob_em_4 = self.blobs.put(
            b"From: test@example.org\r\n\
               To: bob@example.com\r\n\
               Subject: Draft proposal\r\n\
               Date: Wed, 07 Jan 2026 12:00:00 +0000\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               Work in progress proposal.\r\n"
                .to_vec(),
            "message/rfc822",
        );
        let blob_em_5 = self.blobs.put(
            b"From: Carol Example <carol@example.com>\r\n\
               To: test@example.org\r\n\
               Subject: Archive me\r\n\
               Date: Thu, 08 Jan 2026 16:45:00 +0000\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               \r\n\
               Nothing to see here.\r\n"
                .to_vec(),
            "message/rfc822",
        );
        let blob_sc_1 = self.blobs.put(
            b"require [\"fileinto\"];\r\n\
               if header :contains \"Subject\" \"SPAM\" {\r\n\
                   fileinto \"Junk\";\r\n\
               }\r\n"
                .to_vec(),
            "text/plain",
        );
        let blob_sc_2 = self.blobs.put(
            b"vacation :days 7 \"I am on holiday.\";\r\n".to_vec(),
            "text/plain",
        );

        // --- mailboxes ---------------------------------------------------
        self.mailboxes.seed(
            "mb_inbox",
            mailbox("mb_inbox", "Inbox", None, "inbox", 100, 2, 1, 2, 1),
        );
        self.mailboxes.seed(
            "mb_projects",
            mailbox(
                "mb_projects",
                "Projects",
                Some("mb_inbox"),
                "None",
                200,
                0,
                0,
                0,
                0,
            ),
        );
        self.mailboxes.seed(
            "mb_drafts",
            mailbox("mb_drafts", "Drafts", None, "drafts", 300, 1, 1, 1, 1),
        );
        self.mailboxes.seed(
            "mb_sent",
            mailbox("mb_sent", "Sent", None, "sent", 400, 1, 0, 1, 0),
        );
        self.mailboxes.seed(
            "mb_archive",
            mailbox("mb_archive", "Archive", None, "archive", 500, 1, 0, 1, 0),
        );

        // --- emails ------------------------------------------------------
        self.emails.seed(
            "em_1",
            email(
                "em_1",
                &blob_em_1,
                "th_1",
                &["mb_inbox"],
                json!({ "$seen": true }),
                "Welcome to JMAP",
                "alice@example.com",
                "test@example.org",
                "2026-01-05T09:00:00Z",
                312,
            ),
        );
        self.emails.seed(
            "em_2",
            email(
                "em_2",
                &blob_em_2,
                "th_1",
                &["mb_inbox"],
                json!({}),
                "Re: Welcome to JMAP",
                "bob@example.com",
                "test@example.org",
                "2026-01-05T10:30:00Z",
                288,
            ),
        );
        self.emails.seed(
            "em_3",
            email(
                "em_3",
                &blob_em_3,
                "th_2",
                &["mb_sent"],
                json!({ "$seen": true }),
                "Project status",
                "test@example.org",
                "bob@example.com",
                "2026-01-06T08:15:00Z",
                264,
            ),
        );
        self.emails.seed(
            "em_4",
            email(
                "em_4",
                &blob_em_4,
                "th_3",
                &["mb_drafts"],
                json!({ "$seen": true, "$draft": true }),
                "Draft proposal",
                "test@example.org",
                "bob@example.com",
                "2026-01-07T12:00:00Z",
                252,
            ),
        );
        self.emails.seed(
            "em_5",
            email(
                "em_5",
                &blob_em_5,
                "th_4",
                &["mb_archive"],
                json!({ "$seen": true, "$flagged": true }),
                "Archive me",
                "carol@example.com",
                "test@example.org",
                "2026-01-08T16:45:00Z",
                240,
            ),
        );

        // --- threads -----------------------------------------------------
        self.threads.seed("th_1", thread("th_1", &["em_1", "em_2"]));
        self.threads.seed("th_2", thread("th_2", &["em_3"]));
        self.threads.seed("th_3", thread("th_3", &["em_4"]));
        self.threads.seed("th_4", thread("th_4", &["em_5"]));

        // --- identities --------------------------------------------------
        self.identities.seed(
            "id_1",
            json!({
                "id": "id_1",
                "name": "Test User",
                "email": "test@example.org",
                "replyTo": [],
                "bcc": [],
                "textSignature": "-- \nTest User",
                "htmlSignature": "<p>-- <br/>Test User</p>",
                "mayDelete": true
            }),
        );
        self.identities.seed(
            "id_2",
            json!({
                "id": "id_2",
                "name": "Alias User",
                "email": "alias@example.org",
                "replyTo": [{ "name": "Test User", "email": "test@example.org" }],
                "bcc": [],
                "textSignature": "-- \nAlias User",
                "htmlSignature": "<p>-- <br/>Alias User</p>",
                "mayDelete": true
            }),
        );

        // --- submissions -------------------------------------------------
        self.submissions.seed(
            "sub_1",
            json!({
                "id": "sub_1",
                "identityId": "id_1",
                "emailId": "em_3",
                "threadId": "th_2",
                "envelope": {
                    "mailFrom": { "email": "test@example.org", "parameters": null },
                    "rcptTo": [{ "email": "bob@example.com", "parameters": null }]
                },
                "sendAt": "2026-01-06T08:15:01Z",
                "undoStatus": "final",
                "deliveryStatus": {
                    "bob@example.com": {
                        "smtpReply": "2.0.0 <bob@example.com> Queued mail for delivery",
                        "delivered": "yes",
                        "displayed": "yes"
                    }
                },
                "dsnBlobIds": [],
                "mdnBlobIds": []
            }),
        );

        // --- vacation response -------------------------------------------
        self.vacation.seed(
            "singleton",
            json!({
                "id": "singleton",
                "isEnabled": false,
                "fromDate": "2026-01-01T00:00:00Z",
                "toDate": "2026-12-31T23:59:59Z",
                "subject": "Out of office",
                "textBody": "I am currently out of the office.",
                "htmlBody": "<p>I am currently out of the office.</p>"
            }),
        );

        // --- sieve scripts -----------------------------------------------
        self.sieve_scripts.seed(
            "sc_1",
            json!({ "id": "sc_1", "name": "filter", "blobId": blob_sc_1, "isActive": true }),
        );
        self.sieve_scripts.seed(
            "sc_2",
            json!({ "id": "sc_2", "name": "vacation", "blobId": blob_sc_2, "isActive": false }),
        );

        // --- principals --------------------------------------------------
        self.principals.seed(
            "pr_1",
            json!({
                "id": "pr_1",
                "type": "individual",
                "name": "Test User",
                "description": "The primary mock principal",
                "email": "test@example.org",
                "timezone": "Etc/UTC",
                "capabilities": ["urn:ietf:params:jmap:principal", "urn:ietf:params:jmap:principals:owner"],
                "aliases": ["alias@example.org"],
                "quota": 1_073_741_824,
                "members": []
            }),
        );
        self.principals.seed(
            "pr_2",
            json!({
                "id": "pr_2",
                "type": "group",
                "name": "Team",
                "description": "A mock group principal",
                "email": "team@example.org",
                "capabilities": ["urn:ietf:params:jmap:principal"],
                "members": ["pr_1"]
            }),
        );

        // --- push subscriptions ------------------------------------------
        self.push_subscriptions.seed(
            "push_1",
            json!({
                "id": "push_1",
                "deviceClientId": "device-1-mock",
                "url": "https://push.example.org/push/abc123",
                "keys": {
                    "p256dh": "BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkvMeAtAJLFzeOzIqYBD8sa1lEYF4D8_m9zNRWWl0GgEP45i_WZktCg",
                    "auth": "p72itMC0B6uoxm9f"
                },
                "verificationCode": "mock-verification-code",
                "expires": "2027-01-01T00:00:00Z",
                "types": ["Email", "Mailbox"]
            }),
        );

        // Every collection starts at state 1 with everything created there.
        for collection in [
            &mut self.mailboxes,
            &mut self.threads,
            &mut self.emails,
            &mut self.identities,
            &mut self.submissions,
            &mut self.vacation,
            &mut self.sieve_scripts,
            &mut self.principals,
            &mut self.push_subscriptions,
        ] {
            collection.record_initial();
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn mailbox(
    id: &str,
    name: &str,
    parent_id: Option<&str>,
    role: &str,
    sort_order: u32,
    total_emails: usize,
    unread_emails: usize,
    total_threads: usize,
    unread_threads: usize,
) -> Value {
    let mut obj = Map::new();
    obj.insert("id".into(), json!(id));
    obj.insert("name".into(), json!(name));
    if let Some(parent_id) = parent_id {
        obj.insert("parentId".into(), json!(parent_id));
    }
    obj.insert(
        "role".into(),
        if role == "None" {
            Value::Null
        } else {
            json!(role)
        },
    );
    obj.insert("sortOrder".into(), json!(sort_order));
    obj.insert("totalEmails".into(), json!(total_emails));
    obj.insert("unreadEmails".into(), json!(unread_emails));
    obj.insert("totalThreads".into(), json!(total_threads));
    obj.insert("unreadThreads".into(), json!(unread_threads));
    obj.insert("isSubscribed".into(), json!(true));
    obj.insert(
        "myRights".into(),
        json!({
            "mayReadItems": true,
            "mayAddItems": true,
            "mayRemoveItems": true,
            "maySetSeen": true,
            "maySetKeywords": true,
            "mayCreateChild": true,
            "mayRename": true,
            "mayDelete": true,
            "maySubmit": true
        }),
    );
    Value::Object(obj)
}

fn email(
    id: &str,
    blob_id: &str,
    thread_id: &str,
    mailbox_ids: &[&str],
    keywords: Value,
    subject: &str,
    from: &str,
    to: &str,
    received_at: &str,
    size: usize,
) -> Value {
    let mut ids = Map::new();
    for mailbox_id in mailbox_ids {
        ids.insert((*mailbox_id).to_string(), json!(true));
    }
    json!({
        "id": id,
        "blobId": blob_id,
        "threadId": thread_id,
        "mailboxIds": ids,
        "keywords": keywords,
        "size": size,
        "receivedAt": received_at,
        "messageId": [format!("{id}@example.org")],
        "subject": subject,
        "from": [{ "name": from, "email": from }],
        "to": [{ "name": to, "email": to }],
        "cc": [],
        "bcc": [],
        "replyTo": [],
        "sender": [{ "name": from, "email": from }],
        "preview": format!("{subject} (mock preview)"),
        "textBody": ["text-1"],
        "bodyValues": {
            "text-1": {
                "value": format!("{subject} - mock body"),
                "isEncodingProblem": false,
                "isTruncated": false
            }
        },
        "attachments": []
    })
}

fn thread(id: &str, email_ids: &[&str]) -> Value {
    json!({
        "id": id,
        "emailIds": email_ids.iter().map(|id| (*id).to_string()).collect::<Vec<String>>()
    })
}

/// Applies a JMAP patch object onto `target`: every member is set, and a `null`
/// member removes the property (RFC 8620 section 5.1 / RFC 7396 semantics).
pub fn apply_patch(target: &mut Value, patch: &Value) {
    let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) else {
        return;
    };
    for (key, value) in patch {
        if key.starts_with('#') {
            // Back references are resolved before the handler runs; a literal
            // '#' key here is not a patchable property.
            continue;
        }
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// The account id every fixture belongs to.
pub fn account_id() -> &'static str {
    ACCOUNT_ID
}
