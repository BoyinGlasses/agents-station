use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SUPPORTED_AGENTS: [&str; 5] = ["claude", "codex", "opencode", "zcode", "antigravity"];

fn codex_tool(name: &str) -> Option<&'static str> {
    match name {
        "shell_command" | "exec_command" => Some("Bash"),
        "apply_patch" => Some("Edit"),
        _ => None,
    }
}

fn opencode_tool(name: &str) -> Option<&'static str> {
    match name {
        "bash" => Some("Bash"),
        "read" => Some("Read"),
        "edit" => Some("Edit"),
        "write" => Some("Write"),
        "grep" => Some("Grep"),
        "glob" => Some("Glob"),
        "webfetch" => Some("WebFetch"),
        "websearch" => Some("WebSearch"),
        "task" => Some("Task"),
        _ => None,
    }
}

fn antigravity_tool(name: &str) -> Option<&'static str> {
    match name {
        "run_command" => Some("Bash"),
        "view_file" | "view_file_outline" => Some("Read"),
        "grep_search" => Some("Grep"),
        "find_by_name" => Some("Glob"),
        "replace_file_content" | "multi_replace_file_content" => Some("Edit"),
        "write_to_file" => Some("Write"),
        "read_url_content" => Some("WebFetch"),
        "search_web" => Some("WebSearch"),
        _ => None,
    }
}

pub fn normalize_tool(agent: &str, name: &str, input: Value) -> (String, Value) {
    let mut map = match input {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let command = map
        .get("command")
        .or_else(|| map.get("cmd"))
        .or_else(|| map.get("CommandLine"))
        .cloned();
    let mapped_name = if command.is_some() {
        "Bash".to_owned()
    } else {
        let m = match agent {
            "codex" => codex_tool(name),
            "opencode" => opencode_tool(name),
            "antigravity" => antigravity_tool(name),
            _ => None,
        };
        m.unwrap_or(name).to_owned()
    };
    if let Some(cmd_val) = command {
        let cmd_str = match cmd_val {
            Value::Array(arr) => arr
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
            Value::String(s) => s,
            other => other.to_string(),
        };
        map.insert("command".to_owned(), Value::String(cmd_str));
    }
    let file_path = map
        .get("filePath")
        .or_else(|| map.get("AbsolutePath"))
        .or_else(|| map.get("TargetFile"))
        .cloned();
    if let Some(fp) = file_path {
        map.insert("file_path".to_owned(), fp);
    }
    (mapped_name, Value::Object(map))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEvent {
    pub agent: String,
    pub session_id: String,
    pub hook_event_name: String,
    pub cwd: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Value,
    pub prompt: Option<String>,
    pub error: Option<String>,
    pub raw: Value,
}

pub fn normalize_agent_event(mut payload: Value) -> Option<NormalizedEvent> {
    let mut working = payload.as_object()?.clone();
    let agent_raw = working
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_owned();

    if agent_raw == "antigravity"
        && let Some(conv_id) = working.get("conversationId").and_then(Value::as_str).map(str::to_owned)
        && !conv_id.is_empty()
    {
        let args = working
            .get("toolCall")
            .and_then(|tc| tc.get("args"))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let tc_name = working
            .get("toolCall")
            .and_then(|tc| tc.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mapped_tool = antigravity_tool(&tc_name).unwrap_or(&tc_name).to_owned();

        let mut ti = args.clone();
        if let Some(v) = args.get("CommandLine").cloned() {
            ti.insert("command".to_owned(), v);
        }
        if let Some(v) = args
            .get("AbsolutePath")
            .or_else(|| args.get("TargetFile"))
            .cloned()
        {
            ti.insert("file_path".to_owned(), v);
        }
        if let Some(v) = args.get("SearchQuery").cloned() {
            ti.insert("query".to_owned(), v);
        }
        if let Some(v) = args.get("Url").cloned() {
            ti.insert("url".to_owned(), v);
        }

        let ws0 = working
            .get("workspacePaths")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .cloned();
        let last_input = working.get("lastUserInput").cloned();
        let prev_ev = working
            .get("hook_event_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let has_err = working
            .get("error")
            .is_some_and(|e| match e {
                Value::Null | Value::Bool(false) => false,
                Value::String(s) => !s.is_empty(),
                _ => true,
            });
        let next_ev = if prev_ev == "PreInvocation" {
            "UserPromptSubmit".to_owned()
        } else if prev_ev == "PostToolUse" && has_err {
            "PostToolUseFailure".to_owned()
        } else {
            prev_ev
        };

        working.insert("session_id".to_owned(), Value::String(conv_id));
        if let Some(c) = ws0 {
            working.insert("cwd".to_owned(), c);
        }
        if !mapped_tool.is_empty() {
            working.insert("tool_name".to_owned(), Value::String(mapped_tool));
        }
        working.insert("tool_input".to_owned(), Value::Object(ti));
        if let Some(p) = last_input {
            working.insert("prompt".to_owned(), p);
        }
        working.insert("hook_event_name".to_owned(), Value::String(next_ev));
    }

    let sid = working
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?
        .to_owned();
    let ev_name = working
        .get("hook_event_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?
        .to_owned();

    if agent_raw != "claude" && !matches!(agent_raw.as_str(), "codex" | "opencode" | "zcode" | "antigravity") {
        return None;
    }

    if agent_raw != "claude" {
        if let Some(tr_path) = working.remove("transcript_path")
            && agent_raw == "codex"
            && tr_path.is_string()
        {
            working.insert("codex_transcript_path".to_owned(), tr_path);
        }
        let prefixed_sid = format!("{agent_raw}:{sid}");
        working.insert("session_id".to_owned(), Value::String(prefixed_sid));

        if ev_name == "Interrupt" {
            working.insert("hook_event_name".to_owned(), Value::String("Stop".to_owned()));
        }

        if let Some(tn) = working.get("tool_name").and_then(Value::as_str).map(str::to_owned) {
            let mapped = match agent_raw.as_str() {
                "codex" => codex_tool(&tn),
                "opencode" => opencode_tool(&tn),
                _ if tn == "ApplyPatch" => Some("Edit"),
                _ => None,
            };
            if let Some(m) = mapped {
                working.insert("tool_name".to_owned(), Value::String(m.to_owned()));
            }
        }

        if agent_raw == "opencode"
            && let Some(Value::Object(ti)) = working.get_mut("tool_input")
            && let Some(fp) = ti.get("filePath").cloned()
        {
            ti.insert("file_path".to_owned(), fp);
        }
    }

    payload = Value::Object(working);
    let final_sid = payload["session_id"].as_str()?.to_owned();
    let final_ev = payload["hook_event_name"].as_str()?.to_owned();
    let cwd = payload.get("cwd").and_then(Value::as_str).map(str::to_owned);
    let tool_name = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let tool_input = payload
        .get("tool_input")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let prompt = payload
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let error = payload.get("error").map(|e| match e {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });

    Some(NormalizedEvent {
        agent: agent_raw,
        session_id: final_sid,
        hook_event_name: final_ev,
        cwd,
        tool_name,
        tool_input,
        prompt,
        error,
        raw: payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_all_five_agents_and_rejects_unknown() {
        let claude = normalize_agent_event(json!({
            "session_id": "abc",
            "hook_event_name": "PreToolUse",
            "tool_name": "Read"
        }))
        .unwrap();
        assert_eq!(claude.session_id, "abc");
        assert_eq!(claude.agent, "claude");

        let codex = normalize_agent_event(json!({
            "agent": "codex",
            "session_id": "c1",
            "hook_event_name": "Interrupt",
            "tool_name": "shell_command",
            "transcript_path": "/tmp/codex.jsonl"
        }))
        .unwrap();
        assert_eq!(codex.session_id, "codex:c1");
        assert_eq!(codex.hook_event_name, "Stop");
        assert_eq!(codex.tool_name.as_deref(), Some("Bash"));
        assert_eq!(codex.raw["codex_transcript_path"], "/tmp/codex.jsonl");
        assert!(codex.raw.get("transcript_path").is_none());

        let oc = normalize_agent_event(json!({
            "agent": "opencode",
            "session_id": "o1",
            "hook_event_name": "PreToolUse",
            "tool_name": "edit",
            "tool_input": { "filePath": "src/main.rs" }
        }))
        .unwrap();
        assert_eq!(oc.session_id, "opencode:o1");
        assert_eq!(oc.tool_name.as_deref(), Some("Edit"));
        assert_eq!(oc.tool_input["file_path"], "src/main.rs");

        let ag = normalize_agent_event(json!({
            "agent": "antigravity",
            "conversationId": "conv-9",
            "workspacePaths": ["D:/mystation/agents-station"],
            "hook_event_name": "PreInvocation",
            "lastUserInput": "ship it",
            "toolCall": { "name": "run_command", "args": { "CommandLine": "cargo test" } }
        }))
        .unwrap();
        assert_eq!(ag.session_id, "antigravity:conv-9");
        assert_eq!(ag.hook_event_name, "UserPromptSubmit");
        assert_eq!(ag.cwd.as_deref(), Some("D:/mystation/agents-station"));
        assert_eq!(ag.tool_name.as_deref(), Some("Bash"));
        assert_eq!(ag.tool_input["command"], "cargo test");
        assert_eq!(ag.prompt.as_deref(), Some("ship it"));

        let ag_fail = normalize_agent_event(json!({
            "agent": "antigravity",
            "conversationId": "conv-9",
            "hook_event_name": "PostToolUse",
            "error": "exit 1"
        }))
        .unwrap();
        assert_eq!(ag_fail.hook_event_name, "PostToolUseFailure");

        assert!(
            normalize_agent_event(json!({
                "agent": "unknown_agent",
                "session_id": "u1",
                "hook_event_name": "Stop"
            }))
            .is_none()
        );
    }
}
