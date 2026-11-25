use crate::auth::constant_time_eq;
use crate::http::AppState;
use crate::http::guard::{
    OptionalPeer, ViaTunnel, enforce_same_origin_post_with_transport, is_authorized,
    is_loopback_peer,
};
use crate::state::sessions::now_ms;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render_phone_html(state: &AppState) -> String {
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let mut sessions: Vec<_> = office.sessions.values().collect();
    sessions.sort_by(|a, b| a.desk.cmp(&b.desk).then_with(|| a.id.cmp(&b.id)));
    let mut perms: Vec<_> = office.pending_perms.values().collect();
    perms.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));

    let mut cards = String::new();
    for s in sessions {
        cards.push_str("<div class=\"card\"><b>");
        cards.push_str(&escape_html(&s.name));
        cards.push_str("</b> · ");
        cards.push_str(&escape_html(&s.status));
        if let Some(prompt) = s.last_prompt.as_deref().filter(|p| !p.is_empty()) {
            cards.push_str("<div class=\"prompt\">");
            cards.push_str(&escape_html(prompt));
            cards.push_str("</div>");
        }
        cards.push_str("</div>");
    }
    for p in perms {
        cards.push_str("<div class=\"card perm\"><b>");
        cards.push_str(&escape_html(&p.name));
        cards.push_str("</b> · ");
        cards.push_str(&escape_html(&p.label));
        if !p.detail.is_empty() {
            cards.push_str("<div class=\"detail\">");
            cards.push_str(&escape_html(&p.detail));
            cards.push_str("</div>");
        }
        cards.push_str("</div>");
    }
    if cards.is_empty() {
        cards.push_str("<div class=\"card\">No active sessions</div>");
    }
    PHONE_HTML_TEMPLATE.replace("<!--APP-->", &cards)
}

pub async fn get_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !is_authorized(
        peer,
        via_tunnel,
        &headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Redirect::to("/pin").into_response();
    }
    Html(render_phone_html(&state)).into_response()
}

pub async fn get_phone(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !is_authorized(
        peer,
        via_tunnel,
        &headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Redirect::to("/pin").into_response();
    }
    Html(render_phone_html(&state)).into_response()
}

pub async fn get_pin() -> Html<&'static str> {
    Html(PIN_HTML)
}

pub async fn post_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Err(code) = enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    ) {
        return code.into_response();
    }
    let pin = body.get("pin").and_then(Value::as_str).unwrap_or("").trim();
    let keep = body.get("keep").and_then(Value::as_bool).unwrap_or(true);
    let expected = state.auth.current_pin();
    let matched = constant_time_eq(pin, &expected);
    let ip = if via_tunnel {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        peer.map(|a| a.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    };
    {
        let mut limiter = state
            .pin_limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Err(code) = limiter.check_and_record(ip, now_ms(), matched) {
            return (
                code,
                axum::Json(json!({ "ok": false, "error": "too many attempts" })),
            )
                .into_response();
        }
    }
    if !matched {
        return (StatusCode::UNAUTHORIZED, axum::Json(json!({ "ok": false }))).into_response();
    }
    let cookie = state.auth.set_cookie_header(keep, via_tunnel);
    let mut resp = axum::Json(json!({ "ok": true })).into_response();
    if let Ok(hv) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(header::SET_COOKIE, hv);
    }
    resp
}

pub async fn get_manifest() -> Response {
    let body = json!({
        "name": "Agents Station",
        "short_name": "Station",
        "start_url": "/phone",
        "display": "standalone",
        "background_color": "#14161e",
        "theme_color": "#d66440"
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response()
}

pub async fn get_sw() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (
                header::HeaderName::from_static("service-worker-allowed"),
                "/",
            ),
        ],
        SW_JS,
    )
        .into_response()
}

pub async fn get_health(
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
) -> (StatusCode, &'static str) {
    let _ = is_loopback_peer(peer, via_tunnel, &headers);
    (StatusCode::OK, "ok")
}

const PIN_HTML: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Agents Station — PIN</title>
<style>body{background:#14161e;color:#f5f0e8;font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0}
form{background:#1e222e;padding:24px;border-radius:12px;border:1px solid #d66440;width:min(320px,90vw)}
input,button{width:100%;padding:10px;margin-top:10px;border-radius:8px;border:1px solid #3a4052;background:#14161e;color:#fff;font-size:16px}
button{background:#d66440;border:none;font-weight:600;cursor:pointer}</style></head>
<body><form id="f"><h2>Agents Station</h2><input id="p" inputmode="numeric" placeholder="Enter PIN" required>
<label style="display:flex;gap:8px;margin-top:10px"><input id="k" type="checkbox" checked style="width:auto">Remember device</label>
<button type="submit">Unlock</button></form>
<script>document.getElementById('f').onsubmit=async e=>{e.preventDefault();const r=await fetch('/pin',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({pin:p.value,keep:k.checked})});if(r.ok)location.href='/phone';else alert('Wrong PIN');};</script>
</body></html>"##;

const SW_JS: &str = r#"self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (e) => e.waitUntil(self.clients.claim()));
self.addEventListener('push', (e) => {
  let d = {};
  try { d = e.data ? e.data.json() : {}; } catch { d = { body: e.data ? e.data.text() : '' }; }
  e.waitUntil(self.registration.showNotification(d.title || 'Agents Station', {
    body: d.body || '',
    tag: d.tag || undefined,
    renotify: !!d.tag,
    data: { url: d.url || '/phone' },
  }));
});
self.addEventListener('notificationclick', (e) => {
  e.notification.close();
  const target = new URL(e.notification.data?.url || '/phone', self.location.origin).href;
  if (new URL(target).origin !== self.location.origin) {
    e.waitUntil(self.clients.openWindow(target));
    return;
  }
  e.waitUntil(self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((list) => {
    const open = list.find((c) => new URL(c.url).origin === self.location.origin);
    if (open) {
      open.postMessage({ k: 'open', url: target });
      return open.focus();
    }
    return self.clients.openWindow(target);
  }));
});
"#;

const PHONE_HTML_TEMPLATE: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<link rel="manifest" href="/manifest.webmanifest"><meta name="theme-color" content="#d66440"><title>Agents Station Remote</title>
<style>
*{box-sizing:border-box}
body{background:#14161e;color:#f5f0e8;font-family:system-ui,-apple-system,sans-serif;margin:0;padding:14px 14px 72px;max-width:680px;margin-inline:auto}
header{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:12px}
h2{margin:0;font-size:18px;letter-spacing:-.01em}
.pill{font-size:12px;padding:3px 8px;border-radius:999px;background:#232838;color:#c7cedf}
.pill.ok{background:#1e3a2f;color:#7ee2b8}
.tabs{display:flex;gap:8px;margin-bottom:14px}
.tab{flex:1;padding:10px;border-radius:10px;border:1px solid #2f3547;background:#1e222e;color:#f5f0e8;font-weight:600;cursor:pointer}
.tab.active{background:#d66440;border-color:#d66440;color:#fff}
.panel{display:none}.panel.active{display:block}
.card{background:#1e222e;border:1px solid #2f3547;border-radius:10px;padding:12px;margin-bottom:10px}
.card.perm{border-color:#d66440}
.card.danger{border-color:#e05252}
.meta{font-size:12px;opacity:.75;margin-top:2px}
.prompt,.detail{opacity:.88;margin-top:6px;font-size:13px;word-break:break-word;white-space:pre-wrap}
.row{display:flex;gap:8px;flex-wrap:wrap;margin-top:10px;align-items:center}
button.btn{padding:8px 12px;border-radius:8px;border:1px solid #3a4052;background:#262b3a;color:#f5f0e8;font-size:13px;font-weight:600;cursor:pointer}
button.btn.primary{background:#d66440;border-color:#d66440;color:#fff}
button.btn.allow{background:#237a57;border-color:#237a57;color:#fff}
button.btn.deny{background:#8c2d2d;border-color:#8c2d2d;color:#fff}
button.btn:disabled{opacity:.45;cursor:not-allowed}
input,textarea,select{width:100%;padding:10px;border-radius:8px;border:1px solid #3a4052;background:#14161e;color:#f5f0e8;font-size:14px;font-family:inherit}
textarea{min-height:84px;resize:vertical}
.warn{font-size:12px;color:#ffb4a2;margin-top:6px}
.note{font-size:12px;opacity:.75;margin-top:6px}
.reacts{display:flex;gap:6px;flex-wrap:wrap;margin-top:8px}
.react{padding:4px 8px;border-radius:999px;border:1px solid #3a4052;background:#14161e;color:#f5f0e8;font-size:12px;cursor:pointer}
</style></head>
<body>
<header>
  <div><h2>Agents Station · Remote</h2><span id="conn" class="pill">Connecting…</span></div>
  <button id="push-btn" class="btn" type="button">Enable Push</button>
</header>
<nav class="tabs" role="tablist">
  <button class="tab active" data-tab="live" type="button">Live</button>
  <button class="tab" data-tab="chat" type="button">Chat</button>
  <button class="tab" data-tab="task" type="button">Task</button>
</nav>
<section id="tab-live" class="panel active">
  <div id="perms"></div>
  <div id="app"><!--APP--></div>
</section>
<section id="tab-chat" class="panel">
  <div id="chat-list"></div>
  <form id="chat-form" class="card">
    <input id="chat-input" type="text" placeholder="Message group chat…" maxlength="1500" autocomplete="off">
    <div class="row"><button class="btn primary" type="submit">Send</button></div>
  </form>
</section>
<section id="tab-task" class="panel">
  <form id="task-form" class="card">
    <label class="meta" for="task-dir">Project / Session</label>
    <select id="task-dir"></select>
    <div class="row">
      <select id="task-agent" style="flex:1"><option value="claude">Claude</option><option value="codex">Codex</option><option value="opencode">OpenCode</option></select>
    </div>
    <div style="margin-top:10px"><textarea id="task-text" placeholder="Send a prompt or task…"></textarea></div>
    <div class="row">
      <button id="task-send" class="btn primary" type="submit">Run Task</button>
      <button id="task-steer" class="btn" type="button">Steer Busy Session</button>
    </div>
    <div id="task-note" class="note"></div>
  </form>
</section>
<script>
(() => {
  const qs = new URLSearchParams(location.search);
  const kParam = qs.get('k');
  const withKey = (p) => kParam ? p + (p.includes('?') ? '&' : '?') + 'k=' + encodeURIComponent(kParam) : p;
  const state = { local: false, phoneApprove: false, phoneTasks: false, sessions: [], perms: [], chat: [], projects: [], taskSessions: [] };

  const el = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined && text !== null) n.textContent = String(text);
    return n;
  };

  async function apiJson(path, body) {
    const opts = body === undefined
      ? { method: 'GET', credentials: 'same-origin' }
      : { method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) };
    const r = await fetch(withKey(path), opts);
    let data = null;
    try { data = await r.json(); } catch {}
    return { ok: r.ok, status: r.status, data };
  }

  // Tabs
  for (const btn of document.querySelectorAll('.tab')) {
    btn.addEventListener('click', () => {
      for (const b of document.querySelectorAll('.tab')) b.classList.toggle('active', b === btn);
      for (const p of document.querySelectorAll('.panel')) p.classList.toggle('active', p.id === 'tab-' + btn.dataset.tab);
      if (btn.dataset.tab === 'chat') loadChat();
      if (btn.dataset.tab === 'task') loadTasks();
    });
  }

  function renderPerms() {
    const root = document.getElementById('perms');
    const nodes = [];
    for (const p of state.perms) {
      const card = el('div', 'card perm' + (p.danger ? ' danger' : ''));
      const title = el('div');
      title.appendChild(el('b', null, p.name || 'Session'));
      title.appendChild(document.createTextNode(' \u00b7 ' + String(p.label || p.kind || 'Approval')));
      card.appendChild(title);
      if (p.detail) card.appendChild(el('div', 'detail', p.detail));
      const canAct = state.local || state.phoneApprove;
      const blockedDanger = !state.local && !!p.danger;
      if (!canAct) {
        card.appendChild(el('div', 'warn', 'Approve on computer (Phone approvals disabled).'));
      } else if (blockedDanger) {
        card.appendChild(el('div', 'warn', 'High-risk command (' + String(p.danger) + ') — approve on computer.'));
      } else if (p.kind === 'ask') {
        const inp = el('input');
        inp.placeholder = 'Type answer…';
        card.appendChild(el('div', 'row')).appendChild(inp);
        const row = el('div', 'row');
        const ansBtn = el('button', 'btn allow', 'Answer');
        ansBtn.type = 'button';
        ansBtn.addEventListener('click', async () => {
          const v = inp.value.trim();
          if (!v) return;
          ansBtn.disabled = true;
          const qKey = String(p.detail || p.label || 'Question');
          await apiJson('/api/perm', { id: p.id, choice: 'answer', answers: { [qKey]: v } });
          refreshLive();
        });
        const denyBtn = el('button', 'btn deny', 'Deny');
        denyBtn.type = 'button';
        denyBtn.addEventListener('click', async () => {
          denyBtn.disabled = true;
          await apiJson('/api/perm', { id: p.id, choice: 'deny' });
          refreshLive();
        });
        row.append(ansBtn, denyBtn);
        card.appendChild(row);
      } else if (p.kind === 'denied') {
        const row = el('div', 'row');
        const retryBtn = el('button', 'btn allow', 'Retry');
        retryBtn.type = 'button';
        retryBtn.addEventListener('click', async () => {
          retryBtn.disabled = true;
          await apiJson('/api/denied', { id: p.id, op: 'retry' });
          refreshLive();
        });
        const keepBtn = el('button', 'btn deny', 'Keep Denied');
        keepBtn.type = 'button';
        keepBtn.addEventListener('click', async () => {
          keepBtn.disabled = true;
          await apiJson('/api/denied', { id: p.id, op: 'keep' });
          refreshLive();
        });
        row.append(retryBtn, keepBtn);
        card.appendChild(row);
      } else {
        const row = el('div', 'row');
        const allowBtn = el('button', 'btn allow', 'Allow');
        allowBtn.type = 'button';
        allowBtn.addEventListener('click', async () => {
          allowBtn.disabled = true;
          await apiJson('/api/perm', { id: p.id, choice: 'allow' });
          refreshLive();
        });
        const denyBtn = el('button', 'btn deny', 'Deny');
        denyBtn.type = 'button';
        denyBtn.addEventListener('click', async () => {
          denyBtn.disabled = true;
          await apiJson('/api/perm', { id: p.id, choice: 'deny' });
          refreshLive();
        });
        const skipBtn = el('button', 'btn', 'Skip');
        skipBtn.type = 'button';
        skipBtn.addEventListener('click', async () => {
          skipBtn.disabled = true;
          await apiJson('/api/perm', { id: p.id, choice: 'skip' });
          refreshLive();
        });
        row.append(allowBtn, denyBtn, skipBtn);
        card.appendChild(row);
      }
      nodes.push(card);
    }
    root.replaceChildren(...nodes);
  }

  function renderSessions() {
    const root = document.getElementById('app');
    const nodes = [];
    for (const s of state.sessions) {
      const card = el('div', 'card');
      const head = el('div');
      head.appendChild(el('b', null, s.name || s.id || 'Agent'));
      head.appendChild(document.createTextNode(' \u00b7 ' + String(s.status || 'idle')));
      card.appendChild(head);
      if (s.agent) card.appendChild(el('div', 'meta', String(s.agent) + (s.cwd ? ' · ' + String(s.cwd) : '')));
      if (s.last_prompt) card.appendChild(el('div', 'prompt', s.last_prompt));
      nodes.push(card);
    }
    if (!nodes.length && !state.perms.length) {
      nodes.push(el('div', 'card', 'No active sessions'));
    }
    root.replaceChildren(...nodes);
  }

  const CHAT_REACTIONS = ["❤️", "👍", "😆", "😮", "😭", "😡"];

  function renderChat() {
    const root = document.getElementById('chat-list');
    const nodes = [];
    for (const m of state.chat.slice(-40)) {
      const card = el('div', 'card');
      const head = el('div');
      head.appendChild(el('b', null, m.name || m.from || 'Station'));
      card.appendChild(head);
      if (m.text) card.appendChild(el('div', 'prompt', m.text));
      const reacts = el('div', 'reacts');
      const counts = m.reacts || {};
      for (const emoji of CHAT_REACTIONS) {
        const who = Array.isArray(counts[emoji]) ? counts[emoji].length : 0;
        const b = el('button', 'react', who ? emoji + ' ' + who : emoji);
        b.type = 'button';
        b.addEventListener('click', async () => {
          await apiJson('/api/chat', { op: 'react', id: m.id, emoji });
          loadChat();
        });
        reacts.appendChild(b);
      }
      card.appendChild(reacts);
      nodes.push(card);
    }
    if (!nodes.length) nodes.push(el('div', 'card', 'No messages yet'));
    root.replaceChildren(...nodes);
  }

  function renderTaskTargets() {
    const sel = document.getElementById('task-dir');
    const prev = sel.value;
    const opts = [];
    const seen = new Set();
    for (const s of state.taskSessions) {
      if (!s.dir) continue;
      const o = el('option', null, (s.name || s.dir) + ' (' + String(s.status || 'live') + ')');
      o.value = s.dir;
      o.dataset.sid = s.sid || s.id || '';
      opts.push(o);
      seen.add(s.dir);
    }
    for (const p of state.projects) {
      if (!p.dir || seen.has(p.dir)) continue;
      const o = el('option', null, p.name || p.dir);
      o.value = p.dir;
      opts.push(o);
    }
    if (!opts.length) {
      const o = el('option', null, 'No known project directories yet');
      o.value = '';
      opts.push(o);
    }
    sel.replaceChildren(...opts);
    if (prev) sel.value = prev;
    const allowed = state.local || state.phoneTasks;
    document.getElementById('task-send').disabled = !allowed;
    document.getElementById('task-steer').disabled = !allowed;
    document.getElementById('task-note').textContent = allowed ? '' : 'Enable Phone Tasks on the desktop app to run remote prompts.';
  }

  async function refreshLive() {
    const [liveRes, prefsRes] = await Promise.all([apiJson('/api/live'), apiJson('/api/prefs')]);
    if (prefsRes.ok && prefsRes.data && prefsRes.data.prefs) {
      state.phoneApprove = !!prefsRes.data.prefs.phoneApprove;
      state.phoneTasks = !!prefsRes.data.prefs.phoneTasks;
    }
    if (liveRes.ok && liveRes.data) {
      state.local = !!liveRes.data.local;
      state.sessions = Array.isArray(liveRes.data.sessions) ? liveRes.data.sessions : [];
      state.perms = Array.isArray(liveRes.data.perms) ? liveRes.data.perms : [];
      renderPerms();
      renderSessions();
    }
  }

  async function loadChat() {
    const res = await apiJson('/api/chat');
    if (res.ok && res.data && Array.isArray(res.data.msgs)) {
      state.chat = res.data.msgs;
      renderChat();
    }
  }

  async function loadTasks() {
    const res = await apiJson('/api/tasks');
    if (res.ok && res.data) {
      state.phoneTasks = !!res.data.allowed;
      state.projects = Array.isArray(res.data.projects) ? res.data.projects : [];
      state.taskSessions = Array.isArray(res.data.sessions) ? res.data.sessions : [];
      renderTaskTargets();
    }
  }

  document.getElementById('chat-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const inp = document.getElementById('chat-input');
    const text = inp.value.trim();
    if (!text) return;
    inp.value = '';
    await apiJson('/api/chat', { op: 'send', text });
    loadChat();
  });

  document.getElementById('task-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const sel = document.getElementById('task-dir');
    const textEl = document.getElementById('task-text');
    const note = document.getElementById('task-note');
    const text = textEl.value.trim();
    if (!text || !sel.value) return;
    const res = await apiJson('/api/tasks', {
      dir: sel.value,
      agent: document.getElementById('task-agent').value,
      text,
    });
    if (res.ok && res.data && res.data.ok) {
      textEl.value = '';
      note.textContent = 'Task started.';
    } else {
      note.textContent = 'Task rejected (' + res.status + ').';
    }
  });

  document.getElementById('task-steer').addEventListener('click', async () => {
    const sel = document.getElementById('task-dir');
    const opt = sel.options[sel.selectedIndex];
    const sid = opt && opt.dataset ? opt.dataset.sid : '';
    const textEl = document.getElementById('task-text');
    const note = document.getElementById('task-note');
    const text = textEl.value.trim();
    if (!text) return;
    const res = await apiJson('/api/tasks/steer', { sid: sid || undefined, dir: sel.value, text });
    if (res.ok && res.data && res.data.ok) {
      textEl.value = '';
      note.textContent = 'Steer sent (' + String(res.data.how || 'ok') + ').';
    } else {
      note.textContent = 'Steer failed (' + res.status + ').';
    }
  });

  // Web Push registration wired to /sw.js and /api/push
  function b64UrlToBytes(str) {
    const pad = '='.repeat((4 - (str.length % 4)) % 4);
    const b64 = (str + pad).replace(/-/g, '+').replace(/_/g, '/');
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  const pushBtn = document.getElementById('push-btn');
  async function syncPushButton() {
    if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
      pushBtn.textContent = 'Push Unsupported';
      pushBtn.disabled = true;
      return;
    }
    try {
      const reg = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
      const sub = await reg.pushManager.getSubscription();
      pushBtn.textContent = sub ? 'Disable Push' : 'Enable Push';
    } catch {
      pushBtn.textContent = 'Push Unavailable';
    }
  }

  pushBtn.addEventListener('click', async () => {
    if (!('serviceWorker' in navigator) || !('PushManager' in window)) return;
    pushBtn.disabled = true;
    try {
      const reg = await navigator.serviceWorker.register('/sw.js', { scope: '/' });
      const existing = await reg.pushManager.getSubscription();
      if (existing) {
        const ep = existing.endpoint;
        await existing.unsubscribe();
        await apiJson('/api/push', { remove: ep });
      } else {
        const keyRes = await apiJson('/api/push');
        if (!keyRes.ok || !keyRes.data || !keyRes.data.publicKey) return;
        const sub = await reg.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: b64UrlToBytes(keyRes.data.publicKey),
        });
        const j = sub.toJSON();
        await apiJson('/api/push', { endpoint: sub.endpoint, keys: j.keys });
      }
    } finally {
      pushBtn.disabled = false;
      syncPushButton();
    }
  });

  // Live SSE stream
  const conn = document.getElementById('conn');
  const es = new EventSource(withKey('/stream'));
  es.onopen = () => { conn.textContent = 'Live'; conn.className = 'pill ok'; };
  es.onerror = () => { conn.textContent = 'Reconnecting…'; conn.className = 'pill'; };
  es.onmessage = (e) => {
    let d;
    try { d = JSON.parse(e.data); } catch { return; }
    if (d.k === 'snap') {
      state.local = !!d.local;
      state.sessions = Array.isArray(d.sessions) ? d.sessions : [];
      state.perms = Array.isArray(d.perms) ? d.perms : [];
      renderPerms();
      renderSessions();
    } else if (d.k === 'phone') {
      state.phoneApprove = !!d.approve;
      state.phoneTasks = !!d.tasks;
      renderPerms();
      renderTaskTargets();
    } else if (d.k === 'chat') {
      loadChat();
    } else {
      refreshLive();
    }
  };

  refreshLive();
  syncPushButton();
})();
</script>
</body></html>"##;

#[cfg(test)]
mod tests {
    use super::escape_html;
    use crate::http::{AppState, build_router};

    #[test]
    fn escape_html_escapes_all_html_metacharacters() {
        assert_eq!(
            escape_html(r#"<img src="x" onerror='alert(1)&'>"#),
            "&lt;img src=&quot;x&quot; onerror=&#39;alert(1)&amp;&#39;&gt;"
        );
    }
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn post_pin_locks_out_on_6th_failure_per_ip_and_21st_global() {
        // 1. Per-IP lockout: 5 bad attempts -> 401, 6th -> 429 (even with valid PIN)
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.auth.set_pin("492817").unwrap();
        let app = build_router(state.clone());
        let ip_a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), 50000);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "000000"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(ip_a));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "attempt {}", i + 1);
        }

        let mut sixth = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "000000"}).to_string()))
            .unwrap();
        sixth.extensions_mut().insert(ConnectInfo(ip_a));
        let resp = app.clone().oneshot(sixth).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // Even the correct PIN is blocked while locked out
        let mut seventh_good = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": state.auth.current_pin()}).to_string()))
            .unwrap();
        seventh_good.extensions_mut().insert(ConnectInfo(ip_a));
        let resp = app.oneshot(seventh_good).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // 2. Global lockout: 20 failures across 5 IPs (4 each) -> 401; 21st from 6th IP -> 429
        let dir2 = tempfile::tempdir().unwrap();
        let state2 = Arc::new(AppState::new(dir2.path().to_path_buf(), None).unwrap());
        state2.auth.set_pin("492817").unwrap();
        let app2 = build_router(state2);

        for ip_idx in 1u8..=5 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, ip_idx)), 50000);
            for attempt in 0..4 {
                let mut req = Request::builder()
                    .method("POST")
                    .uri("/pin")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"pin": "000000"}).to_string()))
                    .unwrap();
                req.extensions_mut().insert(ConnectInfo(addr));
                let resp = app2.clone().oneshot(req).await.unwrap();
                assert_eq!(
                    resp.status(),
                    StatusCode::UNAUTHORIZED,
                    "ip {ip_idx} attempt {}",
                    attempt + 1
                );
            }
        }

        let ip6 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6)), 50000);
        let mut twenty_first = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "000000"}).to_string()))
            .unwrap();
        twenty_first.extensions_mut().insert(ConnectInfo(ip6));
        let resp = app2.oneshot(twenty_first).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn post_api_pin_enforces_rate_limiter_on_6th_bad_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);
        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51000);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "123456"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(loopback));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "attempt {}", i + 1);
        }

        let mut sixth = Request::builder()
            .method("POST")
            .uri("/api/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "492817"}).to_string()))
            .unwrap();
        sixth.extensions_mut().insert(ConnectInfo(loopback));
        let resp = app.oneshot(sixth).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn phone_html_escapes_injected_markup_and_avoids_inner_html() {
        use crate::state::sessions::{PendingPerm, SessionRecord};
        use axum::body::to_bytes;

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        {
            let mut office = state.office.lock().unwrap();
            office.sessions.insert(
                "s1".into(),
                SessionRecord {
                    id: "s1".into(),
                    agent: "claude".into(),
                    name: "<img src=x onerror=alert(1)>".into(),
                    cwd: "D:/repo".into(),
                    status: "work".into(),
                    desk: 0,
                    started_at: 1,
                    updated_at: 1,
                    last_prompt: Some("<script>alert(2)</script>".into()),
                    last: None,
                    model: None,
                    cache: None,
                    tool_calls: 0,
                    edits: 0,
                    fails: 0,
                    ..Default::default()
                },
            );
            office.pending_perms.insert(
                "p1".into(),
                PendingPerm {
                    id: "p1".into(),
                    sess: "s1".into(),
                    name: "proj".into(),
                    kind: "perm".into(),
                    tool_type: "bash".into(),
                    label: "<b>bash</b>".into(),
                    detail: "<img src=x onerror=alert(3)>".into(),
                    danger: None,
                    created_at: 1,
                },
            );
        }

        let app = build_router(state);
        let req = Request::builder()
            .uri("/phone")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();

        assert!(!html.contains("<img src=x onerror=alert(1)>"), "raw img XSS in /phone");
        assert!(!html.contains("<script>alert(2)</script>"), "raw script XSS in /phone");
        assert!(!html.contains("<img src=x onerror=alert(3)>"), "raw perm XSS in /phone");
        assert!(!html.contains("innerHTML"), "unsafe innerHTML sink in /phone JS");
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(html.contains("&lt;script&gt;alert(2)&lt;/script&gt;"));
        assert!(html.contains("textContent"));
    }

    #[test]
    fn phone_chat_reactions_are_all_accepted_by_chat_engine() {
        use crate::shared::chat_engine::ChatEngine;

        let marker = "const CHAT_REACTIONS = ";
        let start = super::PHONE_HTML_TEMPLATE
            .find(marker)
            .expect("PHONE_HTML_TEMPLATE must define CHAT_REACTIONS")
            + marker.len();
        let rest = &super::PHONE_HTML_TEMPLATE[start..];
        let end = rest.find(';').expect("CHAT_REACTIONS declaration must end with ';'");
        let reactions: Vec<String> =
            serde_json::from_str(&rest[..end]).expect("CHAT_REACTIONS must be valid JSON array");
        assert!(!reactions.is_empty(), "CHAT_REACTIONS must not be empty");

        let mut engine = ChatEngine::from_persisted(None, 1000);
        let msg = engine.say("me", "Boss", "hello", 1000, false, None);
        for emoji in &reactions {
            assert!(
                engine.react(&msg.id, "me", emoji),
                "ChatEngine::react rejected /phone reaction {emoji:?}"
            );
        }
    }

    #[tokio::test]
    async fn sw_js_serves_push_service_worker_and_phone_pwa_has_interactive_controls() {
        use axum::body::to_bytes;

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        // 1. /sw.js returns 200 application/javascript with Service-Worker-Allowed: / and push handler
        let sw_req = Request::builder()
            .uri("/sw.js")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let sw_resp = app.clone().oneshot(sw_req).await.unwrap();
        assert_eq!(sw_resp.status(), StatusCode::OK);
        let ct = sw_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("application/javascript"), "unexpected content-type: {ct}");
        assert_eq!(
            sw_resp
                .headers()
                .get("service-worker-allowed")
                .and_then(|v| v.to_str().ok()),
            Some("/")
        );
        let sw_js = String::from_utf8(
            to_bytes(sw_resp.into_body(), 64 * 1024).await.unwrap().to_vec(),
        )
        .unwrap();
        assert!(sw_js.contains("addEventListener('push'"), "sw.js missing push handler");
        assert!(
            sw_js.contains("addEventListener('notificationclick'"),
            "sw.js missing notificationclick handler"
        );
        assert!(sw_js.contains("showNotification"), "sw.js missing showNotification");

        // 2. /phone serves interactive PWA controls while staying 100% free of innerHTML
        let phone_req = Request::builder()
            .uri("/phone")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let phone_resp = app.oneshot(phone_req).await.unwrap();
        assert_eq!(phone_resp.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(phone_resp.into_body(), 256 * 1024).await.unwrap().to_vec(),
        )
        .unwrap();
        assert!(!html.contains("innerHTML"), "unsafe innerHTML sink in /phone");
        assert!(html.contains("/api/perm"), "/phone missing permission approval action");
        assert!(html.contains("/api/chat"), "/phone missing group chat controls");
        assert!(html.contains("/api/tasks"), "/phone missing task/prompt send controls");
        assert!(html.contains("/api/push"), "/phone missing push subscribe wiring");
        assert!(html.contains("/sw.js"), "/phone missing service worker registration");
    }

    #[tokio::test]
    async fn tunnel_pin_failures_do_not_collide_with_local_loopback_bucket() {
        use crate::http::guard::ViaTunnel;

        // 1. 5 bad tunnel PIN attempts lock out the tunnel without locking out local 127.0.0.1,
        //    and a local valid PIN must not clear the tunnel's lockout bucket.
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.auth.set_pin("492817").unwrap();
        let app = build_router(state.clone());
        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52010);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "000000"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(loopback));
            req.extensions_mut().insert(ViaTunnel(true));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "tunnel attempt {}", i + 1);
        }

        let mut tunnel_6th = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "492817"}).to_string()))
            .unwrap();
        tunnel_6th.extensions_mut().insert(ConnectInfo(loopback));
        tunnel_6th.extensions_mut().insert(ViaTunnel(true));
        let resp = app.clone().oneshot(tunnel_6th).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // Local 127.0.0.1 POST /api/pin is NOT locked out by tunnel failures
        let mut local_ok = Request::builder()
            .method("POST")
            .uri("/api/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "839204"}).to_string()))
            .unwrap();
        local_ok.extensions_mut().insert(ConnectInfo(loopback));
        let resp = app.clone().oneshot(local_ok).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Tunnel remains locked out even after local valid PIN succeeded
        let mut tunnel_still_locked = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "839204"}).to_string()))
            .unwrap();
        tunnel_still_locked.extensions_mut().insert(ConnectInfo(loopback));
        tunnel_still_locked.extensions_mut().insert(ViaTunnel(true));
        let resp = app.oneshot(tunnel_still_locked).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // 2. Vice versa: 5 bad local 127.0.0.1 attempts lock out local without locking out tunnel
        let dir2 = tempfile::tempdir().unwrap();
        let state2 = Arc::new(AppState::new(dir2.path().to_path_buf(), None).unwrap());
        state2.auth.set_pin("492817").unwrap();
        let app2 = build_router(state2);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "123456"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(loopback));
            let resp = app2.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "local attempt {}", i + 1);
        }

        let mut local_6th = Request::builder()
            .method("POST")
            .uri("/api/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "839204"}).to_string()))
            .unwrap();
        local_6th.extensions_mut().insert(ConnectInfo(loopback));
        let resp = app2.clone().oneshot(local_6th).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        let mut tunnel_ok = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "492817"}).to_string()))
            .unwrap();
        tunnel_ok.extensions_mut().insert(ConnectInfo(loopback));
        tunnel_ok.extensions_mut().insert(ViaTunnel(true));
        let resp = app2.oneshot(tunnel_ok).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
