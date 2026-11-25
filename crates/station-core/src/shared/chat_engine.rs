use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

pub const MAX_MSGS: usize = 300;
pub const MAX_PHOTOS: usize = 6;
pub const MAX_KEYS: usize = 600;
pub const REACTIONS: [&str; 6] = ["❤️", "👍", "😆", "😮", "😭", "😡"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMember {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub project: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReplyRef {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    pub t: String,
    #[serde(default)]
    pub by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPoll {
    pub q: String,
    pub opts: Vec<PollOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRemind {
    pub title: String,
    pub at: u64,
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub yes: Vec<String>,
    #[serde(default)]
    pub no: Vec<String>,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub at: u64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub reacts: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<ChatPoll>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remind: Option<ChatRemind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<ChatReplyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recalled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPersisted {
    pub v: u32,
    pub msgs: Vec<ChatMessage>,
    pub pinned: Option<String>,
    pub days: HashMap<String, String>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChatEngine {
    pub msgs: Vec<ChatMessage>,
    pub pinned: Option<String>,
    pub days: HashMap<String, String>,
    pub keys: Vec<String>,
    pub typing: Vec<String>,
    busy: HashMap<String, String>,
    cool: HashMap<String, u64>,
    seq: u64,
}

fn staff_key(from: &str) -> Option<&'static str> {
    match from {
        "reception" => Some("__reception"),
        "hr" => Some("__hr"),
        "cleaner" => Some("__cleaner"),
        "guard" => Some("__guard"),
        "director" => Some("__director"),
        "courier" => Some("__courier"),
        "accountant" => Some("__accountant"),
        "claude" => Some("__claude"),
        "codex" => Some("__codex"),
        "opencode" => Some("__opencode"),
        "antigravity" => Some("__antigravity"),
        "zcode" => Some("__zcode"),
        _ => None,
    }
}

fn short(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max_chars {
        let mut out: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    } else {
        s.to_owned()
    }
}

impl ChatEngine {
    pub fn from_persisted(data: Option<ChatPersisted>, now_ms: u64) -> Self {
        let mut engine = Self {
            msgs: Vec::new(),
            pinned: None,
            days: HashMap::new(),
            keys: Vec::new(),
            typing: Vec::new(),
            busy: HashMap::new(),
            cool: HashMap::new(),
            seq: 0,
        };
        if let Some(d) = data
            && d.v == 1
        {
            engine.msgs = d.msgs;
            engine.pinned = d.pinned;
            engine.days = d.days;
            engine.keys = d.keys;
        }
        if engine.msgs.is_empty() {
            engine.system("Group chat created", now_ms);
        }
        engine
    }

    pub fn to_persisted(&self) -> ChatPersisted {
        let start = self.msgs.len().saturating_sub(MAX_MSGS);
        let mut msgs = self.msgs[start..].to_vec();
        let mut photos = 0usize;
        for m in msgs.iter_mut().rev() {
            if m.photo.is_some() {
                photos += 1;
                if photos > MAX_PHOTOS {
                    m.photo = None;
                }
            }
        }
        let k_start = self.keys.len().saturating_sub(MAX_KEYS);
        ChatPersisted {
            v: 1,
            msgs,
            pinned: self.pinned.clone(),
            days: self.days.clone(),
            keys: self.keys[k_start..].to_vec(),
        }
    }

    fn next_id(&mut self, now_ms: u64) -> String {
        self.seq += 1;
        format!("{now_ms:x}_{:x}", self.seq)
    }

    pub fn fresh(&mut self, key: Option<&str>) -> bool {
        let Some(k) = key.filter(|s| !s.is_empty()) else {
            return true;
        };
        if self.keys.iter().any(|x| x == k) {
            return false;
        }
        self.keys.push(k.to_owned());
        if self.keys.len() > MAX_KEYS + 50 {
            let drain = self.keys.len() - MAX_KEYS;
            self.keys.drain(..drain);
        }
        true
    }

    pub fn add_msg(&mut self, mut m: ChatMessage) -> ChatMessage {
        const KINDS: [&str; 10] = [
            "text", "sticker", "photo", "poll", "remind", "sys", "work", "loop", "ci", "deny",
        ];
        if !KINDS.contains(&m.kind.as_str()) {
            m.kind = "text".to_owned();
        }
        self.msgs.push(m.clone());
        if self.msgs.len() > MAX_MSGS + 30 {
            let drain = self.msgs.len() - MAX_MSGS;
            self.msgs.drain(..drain);
        }
        m
    }

    pub fn system(&mut self, text: &str, now_ms: u64) -> ChatMessage {
        let id = self.next_id(now_ms);
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "sys".to_owned(),
            from: None,
            name: None,
            text: text.to_owned(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply: None,
            work: None,
            r#loop: None,
            ci: None,
            deny: None,
            tag: None,
            recalled: false,
        })
    }

    pub fn say(
        &mut self,
        from: &str,
        name: &str,
        text: &str,
        now_ms: u64,
        long: bool,
        reply: Option<ChatReplyRef>,
    ) -> ChatMessage {
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        let limit = if long { 4000 } else { 600 };
        let clipped: String = text.chars().take(limit).collect();
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "text".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: clipped,
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply,
            work: None,
            r#loop: None,
            ci: None,
            deny: None,
            tag: None,
            recalled: false,
        })
    }

    pub fn work_card(&mut self, from: &str, name: &str, work: Value, now_ms: u64) -> ChatMessage {
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "work".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: String::new(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply: None,
            work: Some(work),
            r#loop: None,
            ci: None,
            deny: None,
            tag: None,
            recalled: false,
        })
    }

    pub fn set_work(&mut self, id: &str, patch: Value) -> Option<&ChatMessage> {
        let m = self.msgs.iter_mut().find(|m| m.id == id && m.kind == "work")?;
        if let (Some(Value::Object(cur)), Value::Object(p)) = (&mut m.work, patch) {
            for (k, v) in p {
                cur.insert(k, v);
            }
        }
        Some(m)
    }

    pub fn loop_card(&mut self, from: &str, name: &str, lp: Value, now_ms: u64) -> ChatMessage {
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "loop".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: String::new(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply: None,
            work: None,
            r#loop: Some(lp),
            ci: None,
            deny: None,
            tag: None,
            recalled: false,
        })
    }

    pub fn deny_card(&mut self, from: &str, name: &str, deny: Value, now_ms: u64) -> ChatMessage {
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "deny".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: String::new(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply: None,
            work: None,
            r#loop: None,
            ci: None,
            deny: Some(deny),
            tag: None,
            recalled: false,
        })
    }

    pub fn set_deny(&mut self, id: &str, state: &str) -> Option<&ChatMessage> {
        let m = self.msgs.iter_mut().find(|m| m.id == id && m.kind == "deny")?;
        if let Some(Value::Object(cur)) = &mut m.deny {
            cur.insert("state".to_owned(), Value::String(state.to_owned()));
            cur.insert("until".to_owned(), Value::from(0u64));
        }
        Some(m)
    }

    pub fn ci_card(&mut self, from: &str, name: &str, ci: Value, now_ms: u64) -> ChatMessage {
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        self.add_msg(ChatMessage {
            id,
            at: now_ms,
            kind: "ci".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: String::new(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: None,
            remind: None,
            reply: None,
            work: None,
            r#loop: None,
            ci: Some(ci),
            deny: None,
            tag: None,
            recalled: false,
        })
    }

    pub fn set_ci(&mut self, id: &str, patch: Value) -> Option<&ChatMessage> {
        let m = self.msgs.iter_mut().find(|m| m.id == id && m.kind == "ci")?;
        if let (Some(Value::Object(cur)), Value::Object(p)) = (&mut m.ci, patch) {
            for (k, v) in p {
                cur.insert(k, v);
            }
        }
        Some(m)
    }

    pub fn by_id(&self, id: &str) -> Option<&ChatMessage> {
        self.msgs.iter().find(|m| m.id == id)
    }

    pub fn react(&mut self, id: &str, by: &str, emoji: &str) -> bool {
        if !REACTIONS.contains(&emoji) {
            return false;
        }
        let Some(m) = self.msgs.iter_mut().find(|m| m.id == id && !m.recalled) else {
            return false;
        };
        let mut toggled_off = false;
        let mut empty_keys = Vec::new();
        for (e, list) in &mut m.reacts {
            if let Some(pos) = list.iter().position(|x| x == by) {
                list.remove(pos);
                if list.is_empty() {
                    empty_keys.push(e.clone());
                }
                if e == emoji {
                    toggled_off = true;
                }
            }
        }
        for k in empty_keys {
            m.reacts.remove(&k);
        }
        if !toggled_off {
            m.reacts.entry(emoji.to_owned()).or_default().push(by.to_owned());
        }
        true
    }

    pub fn poll(
        &mut self,
        from: &str,
        name: &str,
        q: &str,
        options: &[String],
        now_ms: u64,
    ) -> Option<ChatMessage> {
        let opts: Vec<PollOption> = options
            .iter()
            .map(|t| short(t.trim(), 60))
            .filter(|t| !t.is_empty())
            .take(6)
            .map(|t| PollOption { t, by: Vec::new() })
            .collect();
        if opts.len() < 2 {
            return None;
        }
        let q_short = short(q, 120);
        let id = self.next_id(now_ms);
        let key = staff_key(from).unwrap_or(from).to_owned();
        let msg = self.add_msg(ChatMessage {
            id: id.clone(),
            at: now_ms,
            kind: "poll".to_owned(),
            from: Some(key),
            name: Some(name.to_owned()),
            text: q_short.clone(),
            reacts: BTreeMap::new(),
            sticker: None,
            photo: None,
            poll: Some(ChatPoll { q: q_short, opts }),
            remind: None,
            reply: None,
            work: None,
            r#loop: None,
            ci: None,
            deny: None,
            tag: None,
            recalled: false,
        });
        self.pinned = Some(id);
        Some(msg)
    }

    pub fn vote(&mut self, id: &str, by: &str, idx: usize) -> bool {
        let Some(m) = self.msgs.iter_mut().find(|m| m.id == id && m.kind == "poll") else {
            return false;
        };
        let Some(poll) = &mut m.poll else {
            return false;
        };
        if idx >= poll.opts.len() {
            return false;
        }
        let had = poll.opts[idx].by.iter().any(|x| x == by);
        for o in &mut poll.opts {
            o.by.retain(|x| x != by);
        }
        if !had {
            poll.opts[idx].by.push(by.to_owned());
        }
        true
    }

    pub fn rsvp(&mut self, id: &str, by: &str, yes: bool) -> bool {
        let Some(m) = self.msgs.iter_mut().find(|m| m.id == id && m.kind == "remind") else {
            return false;
        };
        let Some(rem) = &mut m.remind else {
            return false;
        };
        rem.yes.retain(|x| x != by);
        rem.no.retain(|x| x != by);
        if yes {
            rem.yes.push(by.to_owned());
        } else {
            rem.no.push(by.to_owned());
        }
        true
    }

    pub fn recall(&mut self, id: &str) -> bool {
        let Some(m) = self
            .msgs
            .iter_mut()
            .find(|m| m.id == id && m.from.as_deref() == Some("me") && !m.recalled)
        else {
            return false;
        };
        m.recalled = true;
        m.text.clear();
        m.sticker = None;
        m.photo = None;
        m.reacts.clear();
        if self.pinned.as_deref() == Some(id) {
            self.pinned = None;
        }
        true
    }

    pub fn pin(&mut self, id: &str) -> bool {
        if self.msgs.iter().any(|m| m.id == id) {
            self.pinned = Some(id.to_owned());
            true
        } else {
            false
        }
    }

    pub fn working(&mut self, key: &str, name: &str, on: bool) {
        if on {
            self.busy.insert(key.to_owned(), name.to_owned());
        } else {
            self.busy.remove(key);
        }
        self.typing = self.busy.values().cloned().collect();
    }

    pub fn cooldown_ok(&mut self, key: &str, ttl_ms: u64, now_ms: u64) -> bool {
        if self.cool.get(key).copied().unwrap_or(0) > now_ms {
            return false;
        }
        self.cool.insert(key.to_owned(), now_ms.saturating_add(ttl_ms));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_engine_reactions_polls_and_recall_work() {
        let mut ch = ChatEngine::from_persisted(None, 1000);
        assert_eq!(ch.msgs.len(), 1);
        let msg = ch.say("me", "Boss", "hello team", 2000, false, None);
        assert!(ch.react(&msg.id, "s1", "❤️"));
        assert!(!ch.react(&msg.id, "s1", "invalid"));
        assert_eq!(ch.msgs.last().unwrap().reacts.get("❤️").unwrap(), &vec!["s1"]);
        // Same emoji toggles off
        assert!(ch.react(&msg.id, "s1", "❤️"));
        assert!(ch.msgs.last().unwrap().reacts.is_empty());

        let poll = ch
            .poll("me", "Boss", "Lunch?", &["Pho".into(), "Com tam".into()], 3000)
            .unwrap();
        assert!(ch.vote(&poll.id, "s1", 0));
        assert!(ch.vote(&poll.id, "s1", 1));
        let p = ch.msgs.iter().find(|m| m.id == poll.id).unwrap().poll.as_ref().unwrap();
        assert!(p.opts[0].by.is_empty());
        assert_eq!(p.opts[1].by, vec!["s1"]);

        assert!(ch.recall(&msg.id));
        assert!(ch.msgs.iter().find(|m| m.id == msg.id).unwrap().recalled);
    }
}
