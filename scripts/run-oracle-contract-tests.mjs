#!/usr/bin/env node
// Plan R2 Task 7 — Dual-Server Side-by-Side Golden Oracle Contract Certification
// + Crash-Survival Proof + 70s Zero-PowerShell <= 35 MB Private WS Soak Gate.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ORACLE_SERVER = 'D:/mystation/my-station/server/index.mjs';
const ORACLE_FIXTURES = 'D:/mystation/my-station/tests/fixtures/otel';
const BIN = process.env.STATION_BIN || 'D:/mystation/cargo-target/debug/agents-station.exe';
const SOAK_SECONDS = Number(process.env.SOAK_SECONDS || 70);

function getFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      srv.close(() => resolve(addr.port));
    });
    srv.on('error', reject);
  });
}

async function waitForHealth(port, label, timeoutMs = 15_000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(500) });
      if (r.ok && (await r.text()) === 'ok') return;
    } catch {}
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error(`${label} failed to become healthy on port ${port}`);
}

function startOracleServer(port, dataDir, claudeDir) {
  return spawn(process.execPath, [ORACLE_SERVER], {
    cwd: path.dirname(path.dirname(ORACLE_SERVER)),
    env: {
      ...process.env,
      OFFICE_NO_NEXT: '1',
      OFFICE_PORT: String(port),
      PORT: String(port),
      OFFICE_DATA_DIR: dataDir,
      CLAUDE_CONFIG_DIR: claudeDir,
      OFFICE_TRANSLATE: '0',
      OFFICE_AI_ACCOUNTS: '0',
      OFFICE_AG_RUNS: '0',
      OFFICE_OPEN_URL: '0',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
}

function startRustServer(port, dataDir, claudeDir) {
  return spawn(BIN, ['--port', String(port), 'serve'], {
    cwd: ROOT,
    env: {
      ...process.env,
      OFFICE_DATA_DIR: dataDir,
      CLAUDE_CONFIG_DIR: claudeDir,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
}

async function stopServer(child, { force = false } = {}) {
  if (!child || child.exitCode !== null) return;
  try {
    child.kill(force ? 'SIGKILL' : 'SIGINT');
  } catch {}
  await new Promise((resolve) => {
    const waitMs = force ? 1000 : 3000;
    const t = setTimeout(() => {
      try {
        child.kill('SIGKILL');
      } catch {}
      if (process.platform === 'win32' && child.pid) {
        try {
          execFileSync('taskkill', ['/F', '/T', '/PID', String(child.pid)], {
            stdio: 'ignore',
            windowsHide: true,
          });
        } catch {}
      }
      resolve();
    }, waitMs);
    child.once('exit', () => {
      clearTimeout(t);
      resolve();
    });
  });
}

function rawHttpRequest({ port, method = 'GET', reqPath = '/', headers = {}, body = null }) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      { hostname: '127.0.0.1', port, method, path: reqPath, headers },
      (res) => {
        const chunks = [];
        res.on('data', (c) => chunks.push(c));
        res.on('end', () => resolve({ status: res.statusCode, text: Buffer.concat(chunks).toString('utf8') }));
      },
    );
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

async function jsonReq(base, reqPath, opts = {}) {
  const r = await fetch(`${base}${reqPath}`, opts);
  const text = await r.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {}
  return { status: r.status, text, json };
}

function assertSubsetKeys(endpoint, oracleObj, rustObj, ignoreOracleKeys = []) {
  assert.ok(oracleObj && typeof oracleObj === 'object', `${endpoint}: Oracle response must be JSON object`);
  assert.ok(rustObj && typeof rustObj === 'object', `${endpoint}: Rust response must be JSON object`);
  const ignored = new Set(ignoreOracleKeys);
  const missing = Object.keys(oracleObj)
    .filter((k) => !ignored.has(k) && !(k in rustObj))
    .sort();
  assert.deepEqual(
    missing,
    [],
    `${endpoint}: Rust response missing Golden Oracle keys: ${missing.join(', ')}\n  Oracle keys: ${Object.keys(oracleObj).sort().join(',')}\n  Rust keys:   ${Object.keys(rustObj).sort().join(',')}`,
  );
}

async function sampleProcessMetrics(pid) {
  const ps =
    `$p = @(Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=${pid}" -ErrorAction SilentlyContinue) | Select-Object -First 1; ` +
    `$kids = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=${pid} AND Name='powershell.exe'" -ErrorAction SilentlyContinue).Count; ` +
    `if ($p -and $p.WorkingSetPrivate -gt 0) { Write-Output ("{0},{1}" -f [math]::Round($p.WorkingSetPrivate/1MB,2), $kids) } else { Write-Output "-1,$kids" }`;
  let lastOut = '';
  for (let attempt = 0; attempt < 4; attempt++) {
    const out = execFileSync('powershell.exe', ['-NoProfile', '-Command', ps], {
      encoding: 'utf8',
      windowsHide: true,
    }).trim();
    lastOut = out;
    const [mb, kids] = out.split(',').map(Number);
    if (Number.isFinite(mb) && mb > 0) {
      return { privateMb: mb, powershellChildren: Number.isFinite(kids) ? kids : 0 };
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  const [mb, kids] = lastOut.split(',').map(Number);
  return {
    privateMb: Number.isFinite(mb) ? mb : -1,
    powershellChildren: Number.isFinite(kids) ? kids : 0,
    raw: lastOut,
  };
}

async function readFirstSseChunk(base) {
  const ctrl = new AbortController();
  const res = await fetch(`${base}/stream`, { signal: ctrl.signal });
  assert.equal(res.status, 200, `${base}/stream must return 200`);
  const reader = res.body.getReader();
  const { value } = await reader.read();
  ctrl.abort();
  return new TextDecoder().decode(value);
}

async function verifyBlockingPermissionRoundTrip(label, base, isOracle) {
  const pres = await jsonReq(base, '/api/presence', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ page: 'contract-test', focused: true }),
  });
  assert.equal(pres.status, 200, `${label}: POST /api/presence must return 200`);

  const permPromise = jsonReq(base, '/hook/permission', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      session_id: 's-claude',
      tool_name: 'Bash',
      tool_input: { command: 'echo contract-ok' },
    }),
  });

  let pendingPerm = null;
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline && !pendingPerm) {
    await new Promise((r) => setTimeout(r, 120));
    const live = await jsonReq(base, '/api/live');
    assert.equal(live.status, 200, `${label}: GET /api/live during hold must return 200`);
    const snap = isOracle ? live.json?.msgs?.[0] : live.json;
    const list = snap?.perms || [];
    if (list.length > 0) pendingPerm = list[0];
  }
  assert.ok(pendingPerm?.id, `${label}: /hook/permission did not register pending hold while focused`);

  const dec = await jsonReq(base, '/api/perm', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: pendingPerm.id, choice: 'allow' }),
  });
  assert.equal(dec.status, 200, `${label}: POST /api/perm must return 200`);
  assert.equal(dec.json?.ok, true, `${label}: POST /api/perm must return ok:true`);

  const permRes = await Promise.race([
    permPromise,
    new Promise((_, rej) => setTimeout(() => rej(new Error(`${label}: /hook/permission timed out`)), 5000)),
  ]);
  assert.equal(permRes.status, 200, `${label}: /hook/permission must return 200 after decision`);
  assert.equal(
    permRes.json?.hookSpecificOutput?.decision?.behavior,
    'allow',
    `${label}: /hook/permission must resolve with allow decision, got ${permRes.text}`,
  );
  // Release focus after test
  await jsonReq(base, '/api/presence', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ page: 'contract-test', focused: false }),
  });
}

async function main() {
  assert.ok(fs.existsSync(BIN), `Rust binary missing: ${BIN}`);
  assert.ok(fs.existsSync(ORACLE_SERVER), `Golden Oracle server missing: ${ORACLE_SERVER}`);
  assert.ok(fs.existsSync(ORACLE_FIXTURES), `Golden Oracle OTLP fixtures missing: ${ORACLE_FIXTURES}`);

  const oracleDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oracle-contract-node-'));
  const rustDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oracle-contract-rust-'));
  const sharedClaudeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oracle-contract-claude-'));

  let oraclePort = await getFreePort();
  let rustPort = await getFreePort();
  let oracleProc = null;
  let rustProc = null;
  let oracleStderr = '';
  let rustStderr = '';

  try {
    oracleProc = startOracleServer(oraclePort, oracleDataDir, sharedClaudeDir);
    oracleProc.stderr?.on('data', (b) => { oracleStderr += b.toString(); });
    rustProc = startRustServer(rustPort, rustDataDir, sharedClaudeDir);
    rustProc.stderr?.on('data', (b) => { rustStderr += b.toString(); });

    await Promise.all([
      waitForHealth(oraclePort, 'Golden Oracle'),
      waitForHealth(rustPort, 'Rust agents-station'),
    ]);
    const ora = `http://127.0.0.1:${oraclePort}`;
    const rst = `http://127.0.0.1:${rustPort}`;
    console.log(`[Oracle] Golden Oracle healthy on ${ora} (pid=${oracleProc.pid})`);
    console.log(`[Rust]   agents-station healthy on ${rst} (pid=${rustProc.pid})`);

    // 1. /health side-by-side
    const [hO, hR] = await Promise.all([fetch(`${ora}/health`), fetch(`${rst}/health`)]);
    assert.equal(hO.status, 200);
    assert.equal(hR.status, 200);
    assert.equal(await hO.text(), 'ok');
    assert.equal(await hR.text(), 'ok');
    console.log('  [PASS] GET /health matches 1:1 (200 "ok")');

    // 2. Identical /event ingestion for all 5 agents
    for (const agent of ['claude', 'codex', 'opencode', 'zcode', 'antigravity']) {
      for (const payload of [
        {
          agent,
          session_id: `s-${agent}`,
          hook_event_name: 'SessionStart',
          cwd: `D:/repos/${agent}-demo`,
        },
        {
          agent,
          session_id: `s-${agent}`,
          hook_event_name: 'UserPromptSubmit',
          prompt: `check ${agent} with token sk-secret1234567890abcdef`,
        },
        {
          agent,
          session_id: `s-${agent}`,
          hook_event_name: 'PostToolUse',
          tool_name: 'Read',
          tool_input: { file_path: `D:/repos/${agent}-demo/README.md` },
        },
      ]) {
        const [rO, rR] = await Promise.all([
          jsonReq(ora, '/event', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
          }),
          jsonReq(rst, '/event', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
          }),
        ]);
        assert.equal(rO.status, 200, `Oracle /event ${agent} ${payload.hook_event_name}`);
        assert.equal(rR.status, 200, `Rust /event ${agent} ${payload.hook_event_name}`);
      }
    }
    console.log('  [PASS] POST /event (5 agents x SessionStart/UserPromptSubmit/PostToolUse) 200 on both');

    // 3. Blocking /hook/permission round-trip with POST /api/presence + POST /api/perm on BOTH servers
    await verifyBlockingPermissionRoundTrip('Oracle', ora, true);
    await verifyBlockingPermissionRoundTrip('Rust', rst, false);
    console.log('  [PASS] Blocking POST /hook/permission + POST /api/perm allow round-trip on both');

    // 4. OTLP /v1/metrics & /v1/logs with Golden Oracle fixtures on BOTH servers
    for (const file of ['metrics-delta-1.json', 'metrics-delta-2.json']) {
      const raw = fs.readFileSync(path.join(ORACLE_FIXTURES, file), 'utf8');
      const [rO, rR] = await Promise.all([
        jsonReq(ora, '/v1/metrics', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: raw }),
        jsonReq(rst, '/v1/metrics', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: raw }),
      ]);
      assert.equal(rO.status, 200, `Oracle /v1/metrics ${file}`);
      assert.equal(rR.status, 200, `Rust /v1/metrics ${file}`);
      assert.ok((rR.json?.accepted ?? 0) > 0, `Rust /v1/metrics ${file} accepted > 0`);
    }
    const logsRaw = fs.readFileSync(path.join(ORACLE_FIXTURES, 'logs.json'), 'utf8');
    const [lO, lR] = await Promise.all([
      jsonReq(ora, '/v1/logs', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: logsRaw }),
      jsonReq(rst, '/v1/logs', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: logsRaw }),
    ]);
    assert.equal(lO.status, 200, 'Oracle /v1/logs');
    assert.equal(lR.status, 200, 'Rust /v1/logs');
    console.log('  [PASS] OTLP POST /v1/metrics + POST /v1/logs 200 on both');

    // 5. SSE /stream initial snapshot on BOTH servers
    const [sseO, sseR] = await Promise.all([readFirstSseChunk(ora), readFirstSseChunk(rst)]);
    assert.ok(sseO.includes('"k":"snap"'), `Oracle SSE missing snap: ${sseO.slice(0, 120)}`);
    assert.ok(sseR.includes('"k":"snap"'), `Rust SSE missing snap: ${sseR.slice(0, 120)}`);
    console.log('  [PASS] GET /stream initial snapshot ("k":"snap") on both');

    // 6. Side-by-side GET contract assertions across all core API endpoints
    const [liveO, liveR] = await Promise.all([jsonReq(ora, '/api/live'), jsonReq(rst, '/api/live')]);
    assert.equal(liveO.status, 200);
    assert.equal(liveR.status, 200);
    const snapO = liveO.json?.msgs?.[0];
    const snapR = liveR.json;
    assert.equal(snapO?.k, 'snap', 'Oracle /api/live msgs[0] must be snap');
    assert.equal(snapR?.k, 'snap', 'Rust /api/live must be snap');
    for (const key of ['k', 'lang', 'sessions', 'perms', 'feed', 'stats', 'local', 'todos', 'trophies', 'pin', 'lift']) {
      assert.ok(key in snapO, `Oracle snap missing ${key}`);
      assert.ok(key in snapR, `Rust snap missing ${key}`);
    }
    assert.equal(snapR.sessions.length, 5, 'Rust /api/live must show all 5 agent sessions');
    const oraClaudeId = snapO.sessions.find((s) => s.sid === 's-claude')?.id;
    const rstClaudeId = snapR.sessions.find((s) => s.id === 's-claude' || s.sid === 's-claude')?.id;
    assert.ok(oraClaudeId, 'Oracle session id for s-claude missing');
    assert.ok(rstClaudeId, 'Rust session id for s-claude missing');
    console.log('  [PASS] GET /api/live normalized snapshot keys match');

    const sharedEndpoints = [
      '/api/burn',
      '/api/wrapped',
      '/api/vault',
      '/api/timeline',
      '/api/sys',
      '/api/stranded',
      '/api/models',
      '/api/push',
      '/api/tunnel',
    ];
    for (const ep of sharedEndpoints) {
      const [rO, rR] = await Promise.all([jsonReq(ora, ep), jsonReq(rst, ep)]);
      assert.equal(rO.status, 200, `Oracle GET ${ep} must be 200`);
      assert.equal(rR.status, 200, `Rust GET ${ep} must be 200`);
      assertSubsetKeys(`GET ${ep}`, rO.json, rR.json);
    }
    console.log(`  [PASS] Side-by-side 1:1 key-shape parity across ${sharedEndpoints.length} shared GET endpoints`);

    // Vault redaction verification on Rust
    const vaultSearch = await jsonReq(rst, '/api/vault?q=check');
    assert.equal(vaultSearch.status, 200);
    assert.ok(vaultSearch.json.items.length >= 1, 'Rust vault must index submitted prompts');
    assert.ok(!vaultSearch.text.includes('sk-secret1234567890abcdef'), 'Rust vault must redact API secrets');

    // /api/prefs (POST on both, GET on Rust) & /api/harnesses (Rust native launcher)
    const [prefsPostO, prefsPostR] = await Promise.all([
      jsonReq(ora, '/api/prefs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ lang: 'vi' }),
      }),
      jsonReq(rst, '/api/prefs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ lang: 'vi' }),
      }),
    ]);
    assert.equal(prefsPostO.status, 200, 'Oracle POST /api/prefs');
    assert.equal(prefsPostR.status, 200, 'Rust POST /api/prefs');
    const prefsGetR = await jsonReq(rst, '/api/prefs');
    assert.equal(prefsGetR.status, 200);
    assert.equal(prefsGetR.json.lang, 'vi');

    const harnessesR = await jsonReq(rst, '/api/harnesses');
    assert.equal(harnessesR.status, 200);
    assert.equal(harnessesR.json.harnesses.length, 5, 'Rust /api/harnesses must list 5 built-in harnesses');
    console.log('  [PASS] /api/prefs (both) + /api/harnesses (Rust 5-harness catalog)');

    // 7. Mutations side-by-side: POST /api/pins, POST /api/chat, GET /api/undo?id=..., POST /api/pin
    const [pinAddO, pinAddR] = await Promise.all([
      jsonReq(ora, '/api/pins', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: oraClaudeId, op: 'add', text: 'remember rust parity' }),
      }),
      jsonReq(rst, '/api/pins', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: rstClaudeId, op: 'add', text: 'remember rust parity' }),
      }),
    ]);
    assert.equal(pinAddO.status, 200);
    assert.equal(pinAddR.status, 200);
    assert.equal(pinAddO.json?.ok, true, `Oracle POST /api/pins failed: ${pinAddO.text}`);
    assert.equal(pinAddR.json?.ok, true, `Rust POST /api/pins failed: ${pinAddR.text}`);

    const [chatO, chatR] = await Promise.all([
      jsonReq(ora, '/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ op: 'say', text: 'hello station' }),
      }),
      jsonReq(rst, '/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ op: 'say', text: 'hello station' }),
      }),
    ]);
    assert.equal(chatO.status, 200);
    assert.equal(chatR.status, 200);
    assert.equal(chatO.json?.ok, true);
    assert.equal(chatR.json?.ok, true);

    const [undoO, undoR] = await Promise.all([
      jsonReq(ora, `/api/undo?id=${encodeURIComponent(oraClaudeId)}`),
      jsonReq(rst, `/api/undo?id=${encodeURIComponent(rstClaudeId)}`),
    ]);
    assert.equal(undoO.status, 200);
    assert.equal(undoR.status, 200);
    assert.equal(typeof undoO.json?.ok, 'boolean');
    assert.equal(typeof undoR.json?.ok, 'boolean');

    const [setPinO, setPinR] = await Promise.all([
      jsonReq(ora, '/api/pin', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pin: '739204' }),
      }),
      jsonReq(rst, '/api/pin', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ pin: '739204' }),
      }),
    ]);
    assert.equal(setPinO.status, 200);
    assert.equal(setPinR.status, 200);
    assert.equal(setPinO.json?.ok, true);
    assert.equal(setPinR.json?.ok, true);
    assert.equal(setPinO.json?.pin, '739204');
    assert.equal(setPinR.json?.pin, '739204');
    console.log('  [PASS] Mutations (POST /api/pins, POST /api/chat, GET /api/undo, POST /api/pin) match on both');

    // 8. Security guards
    const [badOriginO, badOriginR] = await Promise.all([
      fetch(`${ora}/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Origin: 'http://localhost:3000' },
        body: JSON.stringify({ op: 'say', text: 'csrf' }),
      }),
      fetch(`${rst}/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Origin: 'http://localhost:3000' },
        body: JSON.stringify({ op: 'say', text: 'csrf' }),
      }),
    ]);
    assert.equal(badOriginO.status, 403, 'Oracle must reject cross-port Origin http://localhost:3000 with 403');
    assert.equal(badOriginR.status, 403, 'Rust must reject cross-port Origin http://localhost:3000 with 403');

    const evilHostR = await rawHttpRequest({
      port: rustPort,
      method: 'GET',
      reqPath: '/api/live',
      headers: { Host: `evil.com:${rustPort}` },
    });
    assert.equal(evilHostR.status, 421, `Rust must reject spoofed Host evil.com:${rustPort} with 421`);

    const badReplayR = await fetch(`${rst}/api/replay?day=../../x`);
    assert.equal(badReplayR.status, 400, 'Rust must reject traversal /api/replay?day=../../x with 400');
    console.log('  [PASS] Security (Origin:localhost:3000 -> 403 both, Host:evil.com -> 421 Rust, replay traversal -> 400 Rust)');

    // 9. Native `agents-station.exe hook` subprocess contract
    const hookOut = execFileSync(BIN, ['hook', `${rst}/event`, '--agent', 'claude', '--event', 'PostToolUse'], {
      input: JSON.stringify({ session_id: 's-claude', tool_name: 'Read' }),
      encoding: 'utf8',
      windowsHide: true,
    });
    assert.ok(hookOut.includes('"ok":true'), `hook live stdout: ${hookOut}`);
    const closedPort = await getFreePort();
    const stopFallback = execFileSync(
      BIN,
      ['hook', `http://127.0.0.1:${closedPort}/event`, '--fast', '--event', 'Stop', '--response-json'],
      { input: '{}', encoding: 'utf8', windowsHide: true },
    );
    assert.equal(stopFallback, '{"decision":""}');
    console.log('  [PASS] Native `agents-station.exe hook` live & closed-port fallback');

    // Done with Golden Oracle — stop it cleanly before crash-survival & 70s soak
    await stopServer(oracleProc);
    oracleProc = null;
    console.log('[Oracle] Golden Oracle cleanly stopped');

    // 10. Crash-survival on Rust: wait for 15s periodic OTel flush, hard-kill (SIGKILL), restart with same dataDir
    const otelStateFile = path.join(rustDataDir, 'otel', 'state.json');
    const flushDeadline = Date.now() + 20_000;
    while (Date.now() < flushDeadline && !fs.existsSync(otelStateFile)) {
      await new Promise((r) => setTimeout(r, 250));
    }
    assert.ok(fs.existsSync(otelStateFile), 'periodic 15s OTel maintenance flush must write otel/state.json before crash');

    console.log(`[Rust]   Hard-killing agents-station (pid=${rustProc.pid}) to test crash survival...`);
    await stopServer(rustProc, { force: true });
    rustPort = await getFreePort();
    rustProc = startRustServer(rustPort, rustDataDir, sharedClaudeDir);
    await waitForHealth(rustPort, 'Rust agents-station (post-crash)');
    const rst2 = `http://127.0.0.1:${rustPort}`;

    const vaultAfter = await jsonReq(rst2, '/api/vault?q=check');
    assert.equal(vaultAfter.status, 200);
    assert.ok(vaultAfter.json.items.length >= 1, 'vault must survive hard crash');

    const burnAfter = await jsonReq(rst2, '/api/burn?range=30d');
    assert.equal(burnAfter.status, 200);
    assert.ok((burnAfter.json.total?.usd ?? 0) > 0, `burn OTel totals must survive hard crash, got ${burnAfter.text}`);

    const chatAfter = await jsonReq(rst2, '/api/chat');
    assert.equal(chatAfter.status, 200);
    assert.ok(
      (chatAfter.json.msgs || []).some((m) => m.text === 'hello station'),
      'chat messages must survive hard crash',
    );

    const replayDaysAfter = await jsonReq(rst2, '/api/replay/days');
    assert.equal(replayDaysAfter.status, 200);
    assert.ok((replayDaysAfter.json.days || []).length >= 1, 'replay day files must survive hard crash');
    const todayKey = replayDaysAfter.json.days[replayDaysAfter.json.days.length - 1];
    const replayAfter = await jsonReq(rst2, `/api/replay?day=${encodeURIComponent(todayKey)}`);
    assert.equal(replayAfter.status, 200);
    assert.ok((replayAfter.json.items || []).length >= 1, 'replay JSONL items must survive hard crash');

    const prefsAfter = await jsonReq(rst2, '/api/prefs');
    assert.equal(prefsAfter.json.lang, 'vi', 'prefs must survive hard crash');
    const pinsAfter = await jsonReq(rst2, '/api/pins');
    assert.deepEqual(pinsAfter.json.pins['s-claude'], ['remember rust parity'], 'pins must survive hard crash');
    console.log('  [PASS] Crash-survival verified (vault, burn, chat, replay, prefs, pins survived SIGKILL)');

    // 11. 70-second idle soak: Private Working Set <= 35 MB AND 0 powershell.exe children
    console.log(`[Soak]   Starting ${SOAK_SECONDS}s soak on pid=${rustProc.pid} (target <= 35 MB Private WS, 0 powershell.exe)...`);
    let maxMb = 0;
    let totalPsKids = 0;
    const steps = Math.max(1, Math.ceil(SOAK_SECONDS / 10));
    for (let i = 0; i <= steps; i++) {
      const sysRes = await fetch(`${rst2}/api/sys`);
      assert.equal(sysRes.status, 200, 'GET /api/sys during soak must return 200');
      const m = await sampleProcessMetrics(rustProc.pid);
      assert.ok(
        m.privateMb > 0,
        `agents-station pid ${rustProc.pid} metrics unavailable (exitCode=${rustProc.exitCode}, raw=${m.raw ?? ''})`,
      );
      maxMb = Math.max(maxMb, m.privateMb);
      totalPsKids += m.powershellChildren;
      console.log(
        `  [soak ${Math.min(i * 10, SOAK_SECONDS)}s/${SOAK_SECONDS}s] PrivateWS=${m.privateMb.toFixed(2)} MB, powershell_children=${m.powershellChildren}`,
      );
      if (i < steps) await new Promise((r) => setTimeout(r, 10_000));
    }

    assert.equal(totalPsKids, 0, `expected 0 powershell.exe children, saw ${totalPsKids}`);
    assert.ok(maxMb <= 35.0, `agents-station Private Working Set ${maxMb.toFixed(2)} MB exceeded 35 MB budget`);
    console.log(`[PASS]   ALL DUAL-SERVER ORACLE CONTRACT + CRASH-SURVIVAL + SOAK GATES PASSED! Peak PrivateWS=${maxMb.toFixed(2)} MB (<= 35 MB), PowerShell children=0.`);
  } catch (err) {
    if (oracleStderr) console.error('--- Oracle stderr ---\n', oracleStderr.slice(-1500));
    if (rustStderr) console.error('--- Rust stderr ---\n', rustStderr.slice(-1500));
    throw err;
  } finally {
    const stops = [];
    if (oracleProc) stops.push(stopServer(oracleProc));
    if (rustProc) stops.push(stopServer(rustProc));
    await Promise.all(stops);
    for (const d of [oracleDataDir, rustDataDir, sharedClaudeDir]) {
      try {
        fs.rmSync(d, { recursive: true, force: true });
      } catch {}
    }
  }
}

main().catch((err) => {
  console.error('[FAIL]', err);
  process.exit(1);
});
