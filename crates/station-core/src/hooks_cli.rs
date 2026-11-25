use crate::atomic::{backup_file, write_bytes_atomic, write_json_atomic};
use crate::config::{home_dir, resolve_claude_dir};
use crate::store::otel_setup::{add_otel, remove_otel};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "StopFailure",
    "PreCompact",
];

pub const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "Interrupt",
    "PreCompact",
];

pub const ZCODE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "Stop",
];

pub const AGY_EVENTS: &[&str] = &[
    "SessionStart",
    "PreInvocation",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];

#[derive(Debug, Clone)]
pub struct InstallReport {
    pub agent: String,
    pub settings_path: PathBuf,
    pub added: usize,
    pub backup: Option<PathBuf>,
}

fn fwd(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn default_settings_path(agent: &str) -> Result<PathBuf> {
    let home = home_dir();
    match agent {
        "claude" => Ok(resolve_claude_dir(std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref())
            .join("settings.json")),
        "codex" => {
            let base = std::env::var("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".codex"));
            Ok(base.join("hooks.json"))
        }
        "opencode" => {
            let base = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".config"));
            Ok(base.join("opencode").join("plugins").join("my-office.js"))
        }
        "zcode" => Ok(home.join(".zcode").join("cli").join("config.json")),
        "antigravity" => Ok(home.join(".gemini").join("config").join("hooks.json")),
        other => bail!("Unsupported agent: {other}"),
    }
}

fn read_json_or_empty(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("invalid JSON in {}", path.display()))
}

fn write_settings_with_backup(path: &Path, cfg: &Value) -> Result<Option<PathBuf>> {
    let backup = backup_file(path)?;
    write_json_atomic(path, cfg)?;
    Ok(backup)
}

fn hook_bin_cmd(bin_path: &Path, route: &str, port: u16, extra: &str) -> String {
    let exe = fwd(bin_path);
    if extra.is_empty() {
        format!("\"{exe}\" hook http://127.0.0.1:{port}{route}")
    } else {
        format!("\"{exe}\" hook http://127.0.0.1:{port}{route} {extra}")
    }
}

fn is_our_cmd(cmd: &str, port: u16) -> bool {
    cmd.contains(&format!("127.0.0.1:{port}/"))
}

fn status_line_cmd(bin_path: &Path, port: u16) -> String {
    format!("\"{}\" status-line --port {port}", fwd(bin_path))
}

fn is_our_status_line(val: &Value, port: u16) -> bool {
    val.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains("status-line") && c.contains(&format!("--port {port}")))
}

fn statusline_prev_path(settings_path: &Path, data_dir: Option<&Path>) -> PathBuf {
    match data_dir {
        Some(dir) => dir.join("statusline-prev.json"),
        None => settings_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("statusline-prev.json"),
    }
}

fn entry_has_our_route(entry: &Value, port: u16, route: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|arr| {
            arr.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| is_our_cmd(c, port) && c.contains(route))
            })
        })
}

fn is_our_codex_hook(h: &Value, port: u16) -> bool {
    h.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| is_our_cmd(c, port) && c.contains("--agent=codex"))
}

fn is_our_zcode_hook(h: &Value, port: u16) -> bool {
    h.get("type").and_then(Value::as_str) == Some("process")
        && h.get("args").and_then(Value::as_array).is_some_and(|args| {
            let strs: Vec<&str> = args.iter().filter_map(Value::as_str).collect();
            strs.iter().any(|s| s.contains(&format!("127.0.0.1:{port}/")))
                && strs.iter().any(|s| *s == "--agent=zcode")
        })
}

fn is_our_agy_block(block: &Value, port: u16) -> bool {
    let Some(obj) = block.as_object() else {
        return false;
    };
    if obj.len() != AGY_EVENTS.len() {
        return false;
    }
    AGY_EVENTS.iter().all(|ev| {
        let Some(handlers) = obj.get(*ev).and_then(Value::as_array) else {
            return false;
        };
        if handlers.len() != 1 {
            return false;
        }
        let h = if ev.contains("ToolUse") {
            if handlers[0].get("matcher").and_then(Value::as_str) != Some("*") {
                return false;
            }
            handlers[0]
                .get("hooks")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
        } else {
            handlers.first()
        };
        h.and_then(|x| x.get("command"))
            .and_then(Value::as_str)
            .is_some_and(|c| {
                is_our_cmd(c, port)
                    && c.contains("--agent=antigravity")
                    && c.contains(&format!("--event={ev}"))
                    && c.contains("--response-json")
            })
    })
}

const OPENCODE_MARKER: &str = "// agents-station opencode bridge";

fn opencode_plugin_source(port: u16) -> String {
    format!(
        r#"{OPENCODE_MARKER}
const send = async (port, sid, hook_event_name, extra = {{}}) => {{
  if (!sid) return;
  try {{
    await fetch(`http://127.0.0.1:${{port}}/event`, {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ agent: 'opencode', session_id: sid, hook_event_name, ...extra }}),
      signal: AbortSignal.timeout(500),
    }});
  }} catch {{}}
}};
export const createOfficePlugin = (port = {port}) => async ({{ directory }}) => ({{
  event: async ({{ event }}) => {{
    const p = event?.properties || {{}}, sid = p.sessionID || p.info?.id;
    const cwd = p.info?.directory || directory || '';
    if (event?.type === 'session.created') await send(port, sid, 'SessionStart', {{ cwd }});
    else if (event?.type === 'session.deleted') await send(port, sid, 'SessionEnd', {{ cwd }});
    else if (event?.type === 'session.idle') await send(port, sid, 'Stop', {{ cwd }});
  }},
  'tool.execute.before': async (input, output) => send(port, input.sessionID, 'PreToolUse', {{ tool_name: input.tool, tool_input: output?.args }}),
  'tool.execute.after': async (input) => send(port, input.sessionID, 'PostToolUse', {{ tool_name: input.tool, tool_input: input.args }}),
}});
export default createOfficePlugin({port});
"#
    )
}

fn is_our_opencode_plugin(text: &str, port: u16) -> bool {
    text.starts_with(OPENCODE_MARKER)
        || (text.contains("createOfficePlugin") && text.contains(&format!("({port})")))
}

pub fn install_agent_hooks(
    agent: &str,
    settings_override: Option<&Path>,
    bin_path: &Path,
    port: u16,
) -> Result<InstallReport> {
    install_agent_hooks_with_data_dir(agent, settings_override, None, bin_path, port)
}

pub fn install_agent_hooks_with_data_dir(
    agent: &str,
    settings_override: Option<&Path>,
    data_dir: Option<&Path>,
    bin_path: &Path,
    port: u16,
) -> Result<InstallReport> {
    let settings_path = match settings_override {
        Some(p) => p.to_path_buf(),
        None => default_settings_path(agent)?,
    };

    match agent {
        "claude" => install_claude(&settings_path, data_dir, bin_path, port),
        "codex" => install_codex(&settings_path, bin_path, port),
        "opencode" => install_opencode(&settings_path, port),
        "zcode" => install_zcode(&settings_path, bin_path, port),
        "antigravity" => install_antigravity(&settings_path, bin_path, port),
        other => bail!("Unsupported agent: {other}"),
    }
}

fn install_claude(
    settings_path: &Path,
    data_dir: Option<&Path>,
    bin_path: &Path,
    port: u16,
) -> Result<InstallReport> {
    let mut cfg = read_json_or_empty(settings_path)?;
    let hooks = cfg
        .as_object_mut()
        .context("settings root must be JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks must be JSON object")?;

    let mut added = 0;
    let event_cmd = hook_bin_cmd(bin_path, "/event", port, "--async");
    for ev in CLAUDE_EVENTS {
        let list = hooks
            .entry((*ev).to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("event hook list must be array")?;
        if !list.iter().any(|e| entry_has_our_route(e, port, "/event")) {
            list.push(json!({
                "hooks": [{ "type": "command", "command": event_cmd, "timeout": 5, "async": true }]
            }));
            added += 1;
        }
    }

    let perm_cmd = hook_bin_cmd(bin_path, "/hook/permission", port, "--wait 600");
    {
        let list = hooks
            .entry("PermissionRequest".to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        if !list
            .iter()
            .any(|e| entry_has_our_route(e, port, "/hook/permission"))
        {
            list.push(json!({
                "hooks": [{ "type": "command", "command": perm_cmd, "timeout": 605 }]
            }));
            added += 1;
        }
    }

    let ask_cmd = hook_bin_cmd(bin_path, "/hook/question", port, "--wait 600");
    let loop_cmd = hook_bin_cmd(bin_path, "/hook/loop", port, "--fast");
    {
        let list = hooks
            .entry("PreToolUse".to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        if !list
            .iter()
            .any(|e| entry_has_our_route(e, port, "/hook/question"))
        {
            list.push(json!({
                "matcher": "AskUserQuestion|ExitPlanMode",
                "hooks": [{ "type": "command", "command": ask_cmd, "timeout": 605 }]
            }));
            added += 1;
        }
        if !list
            .iter()
            .any(|e| entry_has_our_route(e, port, "/hook/loop"))
        {
            list.push(json!({
                "hooks": [{ "type": "command", "command": loop_cmd, "timeout": 2 }]
            }));
            added += 1;
        }
    }

    let start_cmd = hook_bin_cmd(bin_path, "/hook/start", port, "--wait 3");
    {
        let list = hooks
            .entry("SessionStart".to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        if !list
            .iter()
            .any(|e| entry_has_our_route(e, port, "/hook/start"))
        {
            list.push(json!({
                "matcher": "compact",
                "hooks": [{ "type": "command", "command": start_cmd, "timeout": 5 }]
            }));
            added += 1;
        }
    }

    let deny_cmd = hook_bin_cmd(bin_path, "/hook/denied", port, "--wait 50");
    {
        let list = hooks
            .entry("PermissionDenied".to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        if !list
            .iter()
            .any(|e| entry_has_our_route(e, port, "/hook/denied"))
        {
            list.push(json!({
                "hooks": [{ "type": "command", "command": deny_cmd, "timeout": 45 }]
            }));
            added += 1;
        }
    }

    if !cfg
        .get("statusLine")
        .is_some_and(|s| is_our_status_line(s, port))
    {
        let existing_padding = cfg
            .get("statusLine")
            .and_then(|s| s.get("padding"))
            .cloned();
        if let Some(prev) = cfg.get("statusLine")
            && prev
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|c| !c.trim().is_empty())
        {
            let prev_path = statusline_prev_path(settings_path, data_dir);
            write_json_atomic(&prev_path, prev)?;
        }
        let mut status_obj = serde_json::Map::new();
        status_obj.insert("type".to_owned(), Value::String("command".to_owned()));
        status_obj.insert(
            "command".to_owned(),
            Value::String(status_line_cmd(bin_path, port)),
        );
        if let Some(pad) = existing_padding {
            status_obj.insert("padding".to_owned(), pad);
        }
        if let Some(root) = cfg.as_object_mut() {
            root.insert("statusLine".to_owned(), Value::Object(status_obj));
        }
        added += 1;
    }

    added += add_otel(&mut cfg, port, true);

    let backup = if added > 0 || !settings_path.exists() {
        write_settings_with_backup(settings_path, &cfg)?
    } else {
        None
    };

    Ok(InstallReport {
        agent: "claude".to_owned(),
        settings_path: settings_path.to_path_buf(),
        added,
        backup,
    })
}

fn install_codex(settings_path: &Path, bin_path: &Path, port: u16) -> Result<InstallReport> {
    let mut cfg = read_json_or_empty(settings_path)?;
    let hooks = cfg
        .as_object_mut()
        .context("codex hooks root must be JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks must be JSON object")?;

    let mut added = 0;
    let ev_cmd = hook_bin_cmd(bin_path, "/event", port, "--async --agent=codex");
    for ev in CODEX_EVENTS {
        let list = hooks
            .entry((*ev).to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        let has = list.iter().any(|e| {
            e.get("hooks").and_then(Value::as_array).is_some_and(|a| {
                a.iter()
                    .any(|h| is_our_codex_hook(h, port) && h["command"].as_str().is_some_and(|c| c.contains("/event")))
            })
        });
        if !has {
            list.push(json!({
                "hooks": [{ "type": "command", "command": ev_cmd, "timeout": 3 }]
            }));
            added += 1;
        }
    }
    let perm_cmd = hook_bin_cmd(bin_path, "/hook/permission", port, "--agent=codex");
    let perm_list = hooks
        .entry("PermissionRequest".to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap();
    let has_perm = perm_list.iter().any(|e| {
        e.get("hooks").and_then(Value::as_array).is_some_and(|a| {
            a.iter().any(|h| {
                is_our_codex_hook(h, port)
                    && h["command"]
                        .as_str()
                        .is_some_and(|c| c.contains("/hook/permission"))
            })
        })
    });
    if !has_perm {
        perm_list.push(json!({
            "hooks": [{ "type": "command", "command": perm_cmd, "timeout": 55 }]
        }));
        added += 1;
    }

    let backup = if added > 0 || !settings_path.exists() {
        write_settings_with_backup(settings_path, &cfg)?
    } else {
        None
    };

    Ok(InstallReport {
        agent: "codex".to_owned(),
        settings_path: settings_path.to_path_buf(),
        added,
        backup,
    })
}

fn install_opencode(settings_path: &Path, port: u16) -> Result<InstallReport> {
    let text = opencode_plugin_source(port);
    let mut backup = None;
    let mut added = 0;
    if settings_path.exists() {
        let old = std::fs::read_to_string(settings_path)?;
        if !is_our_opencode_plugin(&old, port) {
            bail!("Refusing to overwrite existing plugin: {}", settings_path.display());
        }
        if old != text {
            backup = backup_file(settings_path)?;
            write_bytes_atomic(settings_path, text.as_bytes())?;
            added = 1;
        }
    } else {
        write_bytes_atomic(settings_path, text.as_bytes())?;
        added = 1;
    }
    Ok(InstallReport {
        agent: "opencode".to_owned(),
        settings_path: settings_path.to_path_buf(),
        added,
        backup,
    })
}

fn install_zcode(settings_path: &Path, bin_path: &Path, port: u16) -> Result<InstallReport> {
    let mut cfg = read_json_or_empty(settings_path)?;
    let hooks = cfg
        .as_object_mut()
        .context("zcode config must be JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks must be JSON object")?;

    let enabled = hooks.get("enabled").and_then(Value::as_bool);
    let has_existing = hooks
        .get("events")
        .and_then(Value::as_object)
        .is_some_and(|evs| {
            evs.values()
                .any(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        });
    if enabled == Some(false) || (enabled != Some(true) && has_existing) {
        bail!("ZCode hooks are disabled; enabling them would also enable existing hooks.");
    }

    hooks.insert("enabled".to_owned(), Value::Bool(true));
    let events = hooks
        .entry("events")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks.events must be JSON object")?;

    let exe = bin_path.to_string_lossy().into_owned();
    let url = format!("http://127.0.0.1:{port}/event");
    let mut added = 0;
    for ev in ZCODE_EVENTS {
        let list = events
            .entry((*ev).to_owned())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        let exists = list.iter().any(|e| {
            e.get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().any(|h| is_our_zcode_hook(h, port)))
        });
        if !exists {
            list.push(json!({
                "hooks": [{
                    "type": "process",
                    "command": exe,
                    "args": ["hook", url, "--async", "--agent=zcode"],
                    "timeoutMs": 2500
                }]
            }));
            added += 1;
        }
    }

    let backup = if added > 0 || !settings_path.exists() {
        write_settings_with_backup(settings_path, &cfg)?
    } else {
        None
    };

    Ok(InstallReport {
        agent: "zcode".to_owned(),
        settings_path: settings_path.to_path_buf(),
        added,
        backup,
    })
}

fn install_antigravity(settings_path: &Path, bin_path: &Path, port: u16) -> Result<InstallReport> {
    let mut cfg = read_json_or_empty(settings_path)?;
    let root = cfg
        .as_object_mut()
        .context("antigravity hooks.json must be JSON object")?;

    if let Some(existing) = root.get("my-office")
        && !is_our_agy_block(existing, port)
    {
        bail!(
            "Refusing to overwrite existing my-office hook in {}",
            settings_path.display()
        );
    }

    let mut hook = serde_json::Map::new();
    for ev in AGY_EVENTS {
        let cmd = hook_bin_cmd(
            bin_path,
            "/event",
            port,
            &format!("--async --agent=antigravity --event={ev} --response-json"),
        );
        let handler = json!({ "type": "command", "command": cmd, "timeout": 3 });
        if ev.contains("ToolUse") {
            hook.insert(
                (*ev).to_owned(),
                json!([{ "matcher": "*", "hooks": [handler] }]),
            );
        } else {
            hook.insert((*ev).to_owned(), json!([handler]));
        }
    }

    let next_val = Value::Object(hook);
    let changed = root.get("my-office") != Some(&next_val);
    let backup = if changed || !settings_path.exists() {
        root.insert("my-office".to_owned(), next_val);
        write_settings_with_backup(settings_path, &cfg)?
    } else {
        None
    };

    Ok(InstallReport {
        agent: "antigravity".to_owned(),
        settings_path: settings_path.to_path_buf(),
        added: if changed { AGY_EVENTS.len() } else { 0 },
        backup,
    })
}

pub fn is_agent_hooked(agent: &str, settings_override: Option<&Path>, port: u16) -> bool {
    let Ok(path) = settings_override
        .map(Path::to_path_buf)
        .ok_or(())
        .or_else(|_| default_settings_path(agent))
    else {
        return false;
    };

    if agent == "opencode" {
        return std::fs::read_to_string(&path)
            .ok()
            .is_some_and(|t| is_our_opencode_plugin(&t, port));
    }

    let Ok(cfg) = read_json_or_empty(&path) else {
        return false;
    };

    match agent {
        "claude" => CLAUDE_EVENTS.iter().all(|ev| {
            cfg.get("hooks")
                .and_then(|h| h.get(*ev))
                .and_then(Value::as_array)
                .is_some_and(|list| list.iter().any(|e| entry_has_our_route(e, port, "/event")))
        }),
        "codex" => CODEX_EVENTS.iter().all(|ev| {
            cfg.get("hooks")
                .and_then(|h| h.get(*ev))
                .and_then(Value::as_array)
                .is_some_and(|list| {
                    list.iter().any(|e| {
                        e.get("hooks")
                            .and_then(Value::as_array)
                            .is_some_and(|a| a.iter().any(|h| is_our_codex_hook(h, port)))
                    })
                })
        }),
        "zcode" => {
            cfg.get("hooks")
                .and_then(|h| h.get("enabled"))
                .and_then(Value::as_bool)
                == Some(true)
                && ZCODE_EVENTS.iter().all(|ev| {
                    cfg.get("hooks")
                        .and_then(|h| h.get("events"))
                        .and_then(|e| e.get(*ev))
                        .and_then(Value::as_array)
                        .is_some_and(|list| {
                            list.iter().any(|e| {
                                e.get("hooks")
                                    .and_then(Value::as_array)
                                    .is_some_and(|a| a.iter().any(|h| is_our_zcode_hook(h, port)))
                            })
                        })
                })
        }
        "antigravity" => cfg
            .get("my-office")
            .is_some_and(|b| is_our_agy_block(b, port)),
        _ => false,
    }
}

pub fn uninstall_agent_hooks(
    agent: &str,
    settings_override: Option<&Path>,
    port: u16,
) -> Result<bool> {
    uninstall_agent_hooks_with_data_dir(agent, settings_override, None, port)
}

pub fn uninstall_agent_hooks_with_data_dir(
    agent: &str,
    settings_override: Option<&Path>,
    data_dir: Option<&Path>,
    port: u16,
) -> Result<bool> {
    let path = match settings_override {
        Some(p) => p.to_path_buf(),
        None => default_settings_path(agent)?,
    };
    if !path.exists() {
        return Ok(false);
    }

    if agent == "opencode" {
        let text = std::fs::read_to_string(&path)?;
        if is_our_opencode_plugin(&text, port) {
            std::fs::remove_file(&path)?;
            return Ok(true);
        }
        return Ok(false);
    }

    let mut cfg = read_json_or_empty(&path)?;
    let mut removed = 0usize;

    match agent {
        "claude" => {
            if let Some(hooks) = cfg.get_mut("hooks").and_then(Value::as_object_mut) {
                let keys: Vec<String> = hooks.keys().cloned().collect();
                for k in keys {
                    if let Some(list) = hooks.get_mut(&k).and_then(Value::as_array_mut) {
                        for entry in list.iter_mut() {
                            if let Some(arr) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                                let before = arr.len();
                                arr.retain(|h| {
                                    !h.get("command")
                                        .and_then(Value::as_str)
                                        .is_some_and(|c| is_our_cmd(c, port))
                                });
                                removed += before - arr.len();
                            }
                        }
                        list.retain(|e| {
                            e.get("hooks")
                                .and_then(Value::as_array)
                                .is_none_or(|a| !a.is_empty())
                        });
                    }
                    if hooks.get(&k).and_then(Value::as_array).is_some_and(|a| a.is_empty()) {
                        hooks.remove(&k);
                    }
                }
            }
            if cfg
                .get("statusLine")
                .is_some_and(|s| is_our_status_line(s, port))
            {
                let prev_path = statusline_prev_path(&path, data_dir);
                let prev = std::fs::read_to_string(&prev_path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .filter(|v| {
                        v.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| !c.trim().is_empty())
                    });
                if let Some(root) = cfg.as_object_mut() {
                    if let Some(prev_val) = prev {
                        root.insert("statusLine".to_owned(), prev_val);
                    } else {
                        root.remove("statusLine");
                    }
                }
                let _ = std::fs::remove_file(&prev_path);
                removed += 1;
            }
            removed += remove_otel(&mut cfg);
        }
        "codex" => {
            if let Some(hooks) = cfg.get_mut("hooks").and_then(Value::as_object_mut) {
                let keys: Vec<String> = hooks.keys().cloned().collect();
                for k in keys {
                    if let Some(list) = hooks.get_mut(&k).and_then(Value::as_array_mut) {
                        for entry in list.iter_mut() {
                            if let Some(arr) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                                let before = arr.len();
                                arr.retain(|h| !is_our_codex_hook(h, port));
                                removed += before - arr.len();
                            }
                        }
                        list.retain(|e| {
                            e.get("hooks")
                                .and_then(Value::as_array)
                                .is_none_or(|a| !a.is_empty())
                        });
                    }
                    if hooks.get(&k).and_then(Value::as_array).is_some_and(|a| a.is_empty()) {
                        hooks.remove(&k);
                    }
                }
            }
        }
        "zcode" => {
            if let Some(events) = cfg
                .get_mut("hooks")
                .and_then(|h| h.get_mut("events"))
                .and_then(Value::as_object_mut)
            {
                let keys: Vec<String> = events.keys().cloned().collect();
                for k in keys {
                    if let Some(list) = events.get_mut(&k).and_then(Value::as_array_mut) {
                        for entry in list.iter_mut() {
                            if let Some(arr) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                                let before = arr.len();
                                arr.retain(|h| !is_our_zcode_hook(h, port));
                                removed += before - arr.len();
                            }
                        }
                        list.retain(|e| {
                            e.get("hooks")
                                .and_then(Value::as_array)
                                .is_none_or(|a| !a.is_empty())
                        });
                    }
                    if events.get(&k).and_then(Value::as_array).is_some_and(|a| a.is_empty()) {
                        events.remove(&k);
                    }
                }
            }
        }
        "antigravity" => {
            if let Some(obj) = cfg.as_object_mut()
                && obj
                    .get("my-office")
                    .is_some_and(|b| is_our_agy_block(b, port))
            {
                obj.remove("my-office");
                removed = AGY_EVENTS.len();
            }
        }
        other => bail!("Unsupported agent: {other}"),
    }

    if removed > 0 {
        let _ = write_settings_with_backup(&path, &cfg)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn windows_startup_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join("AppData").join("Roaming"));
    appdata
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
}

pub fn install_windows_service_in(
    startup_dir: &Path,
    bin_path: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
) -> Result<PathBuf> {
    let q = |s: &str| s.replace('"', "\"\"");
    let file = startup_dir.join("Agents Station.vbs");
    let workdir = bin_path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_owned());
    let script = [
        "' Agents Station: starts the background server at sign-in without a window.",
        "Set sh = CreateObject(\"WScript.Shell\")",
        "Set env = sh.Environment(\"Process\")",
        &format!("env(\"OFFICE_HOST\") = \"{}\"", q(host)),
        &format!("env(\"OFFICE_PORT\") = \"{port}\""),
        &format!("env(\"OFFICE_LOG\") = \"{}\"", q(&log_path.to_string_lossy())),
        &format!("sh.CurrentDirectory = \"{}\"", q(&workdir)),
        &format!(
            "sh.Run \"\"\"{}\"\" serve\", 0, False",
            q(&bin_path.to_string_lossy())
        ),
        "",
    ]
    .join("\r\n");

    write_bytes_atomic(&file, script.as_bytes())?;
    Ok(file)
}

pub fn install_windows_service(
    bin_path: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
) -> Result<PathBuf> {
    install_windows_service_in(&windows_startup_dir(), bin_path, host, port, log_path)
}

pub fn uninstall_windows_service_in(startup_dir: &Path) -> Result<bool> {
    let mut removed = false;
    for name in ["Agents Station.vbs", "My Office.vbs", "Claude Office.vbs"] {
        let p = startup_dir.join(name);
        if p.exists() {
            std::fs::remove_file(p)?;
            removed = true;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_agents_install_check_backup_preserve_keys_and_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let bin = Path::new("D:/mystation/agents-station/agents-station.exe");

        for agent in ["claude", "codex", "opencode", "zcode", "antigravity"] {
            let ext = if agent == "opencode" { "js" } else { "json" };
            let settings = dir.path().join(format!("{agent}.{ext}"));

            if agent != "opencode" {
                std::fs::write(&settings, r#"{"unrelatedKey": 42}"#).unwrap();
            }

            assert!(!is_agent_hooked(agent, Some(&settings), 4310));
            let rep = install_agent_hooks(agent, Some(&settings), bin, 4310).unwrap();
            assert!(rep.added > 0, "expected added hooks for {agent}");
            assert!(is_agent_hooked(agent, Some(&settings), 4310));

            if agent != "opencode" {
                assert!(rep.backup.as_ref().is_some_and(|b| b.exists()));
                let saved: Value =
                    serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
                assert_eq!(saved["unrelatedKey"], 42);
                if agent == "antigravity" {
                    assert!(saved["my-office"]["PreToolUse"].is_array());
                    assert_eq!(saved["my-office"]["PreToolUse"][0]["matcher"], "*");
                }
            }

            assert!(uninstall_agent_hooks(agent, Some(&settings), 4310).unwrap());
            assert!(!is_agent_hooked(agent, Some(&settings), 4310));

            if agent != "opencode" {
                let after: Value =
                    serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
                assert_eq!(after["unrelatedKey"], 42);
            }
        }
    }

    #[test]
    fn windows_service_writes_windowless_vbs_without_powershell() {
        let dir = tempfile::tempdir().unwrap();
        let vbs = install_windows_service_in(
            dir.path(),
            Path::new("D:/app/agents-station.exe"),
            "127.0.0.1",
            4310,
            Path::new("D:/app/data/server.log"),
        )
        .unwrap();
        let text = std::fs::read_to_string(&vbs).unwrap();
        assert!(text.contains("agents-station.exe"));
        assert!(text.contains("serve"));
        assert!(!text.to_ascii_lowercase().contains("powershell"));
        assert!(uninstall_windows_service_in(dir.path()).unwrap());
        assert!(!vbs.exists());
    }

    #[test]
    fn install_claude_wires_status_line_saves_prev_and_restores_on_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let settings = dir.path().join("settings.json");
        let bin = Path::new("D:/mystation/agents-station/agents-station.exe");

        std::fs::write(
            &settings,
            r#"{"statusLine": {"type": "command", "command": "echo old-status", "padding": 2}}"#,
        )
        .unwrap();

        let rep = install_agent_hooks_with_data_dir(
            "claude",
            Some(&settings),
            Some(&data_dir),
            bin,
            4310,
        )
        .unwrap();
        assert!(rep.added > 0);

        let prev_file = data_dir.join("statusline-prev.json");
        assert!(prev_file.exists(), "previous statusLine must be saved");
        let prev: Value =
            serde_json::from_str(&std::fs::read_to_string(&prev_file).unwrap()).unwrap();
        assert_eq!(prev["command"], "echo old-status");

        let cfg: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(cfg["statusLine"]["type"], "command");
        assert_eq!(cfg["statusLine"]["padding"], 2);
        let cmd = cfg["statusLine"]["command"].as_str().unwrap();
        assert!(cmd.contains("status-line"), "cmd={cmd}");
        assert!(cmd.contains("--port 4310"), "cmd={cmd}");

        // Second install is idempotent and must not overwrite statusline-prev.json.
        let rep2 = install_agent_hooks_with_data_dir(
            "claude",
            Some(&settings),
            Some(&data_dir),
            bin,
            4310,
        )
        .unwrap();
        assert_eq!(rep2.added, 0);
        let prev_after: Value =
            serde_json::from_str(&std::fs::read_to_string(&prev_file).unwrap()).unwrap();
        assert_eq!(prev_after["command"], "echo old-status");

        // Uninstall restores previous statusLine and removes statusline-prev.json.
        assert!(
            uninstall_agent_hooks_with_data_dir("claude", Some(&settings), Some(&data_dir), 4310)
                .unwrap()
        );
        let restored: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored["statusLine"]["command"], "echo old-status");
        assert!(!prev_file.exists());

        // Fresh install without prior statusLine removes statusLine completely on uninstall.
        let settings2 = dir.path().join("settings2.json");
        std::fs::write(&settings2, "{}").unwrap();
        install_agent_hooks_with_data_dir("claude", Some(&settings2), Some(&data_dir), bin, 4310)
            .unwrap();
        assert!(
            uninstall_agent_hooks_with_data_dir("claude", Some(&settings2), Some(&data_dir), 4310)
                .unwrap()
        );
        let cleaned: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings2).unwrap()).unwrap();
        assert!(cleaned.get("statusLine").is_none());
    }
}
