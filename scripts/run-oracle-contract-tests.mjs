#!/usr/bin/env node
// G1 Contract Oracle Runner & 70-Second Zero-PowerShell <= 35 MB Soak Gate
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
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

async function waitForHealth(port, timeoutMs = 10000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(500) });
      if (r.ok && (await r.text()) === 'ok') return;
    } catch {}
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error(`agents-station serve failed to become healthy on port ${port}`);
}

function startServer(port, dataDir) {
  const child = spawn(BIN, ['--port', String(port), 'serve'], {
    cwd: ROOT,
    env: { ...process.env, OFFICE_DATA_DIR: dataDir },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  return child;
}

async function stopServer(child) {
  if (!child || child.exitCode !== null) return;
  try {
    child.kill('SIGINT');
  } catch {}
  await new Promise((resolve) => {
    const t = setTimeout(() => {
      try { child.kill('SIGKILL'); } catch {}
      resolve();
    }, 3000);
    child.once('exit', () => {
      clearTimeout(t);
      resolve();
    });
  });
}

function sampleProcessMetrics(pid) {
  // Query Windows Task Manager Private Working Set + child powershell.exe count externally from test harness
  const ps = `$p = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=${pid}" -ErrorAction SilentlyContinue; ` +
    `$kids = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=${pid} AND Name='powershell.exe'" -ErrorAction SilentlyContinue).Count; ` +
    `if ($p) { Write-Output ("{0},{1}" -f [math]::Round($p.WorkingSetPrivate/1MB,2), $kids) } else { Write-Output "-1,0" }`;
  const out = execFileSync('powershell.exe', ['-NoProfile', '-Command', ps], {
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
  const [mb, kids] = out.split(',').map(Number);
  return { privateMb: mb, powershellChildren: kids };
}

async function main() {
  assert.ok(fs.existsSync(BIN), `Binary missing: ${BIN}`);
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'agents-station-g1-'));
  let port = await getFreePort();
  let proc = startServer(port, dataDir);

  try {
    await waitForHealth(port);
    const base = `http://127.0.0.1:${port}`;
    console.log(`[G1] Server healthy on ${base} (pid=${proc.pid}, dataDir=${dataDir})`);

    // 1. Security guards: spoofed Host & Origin must return 403
    const badOrigin = await fetch(`${base}/api/harnesses/launch`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Origin: 'https://evil.com' },
      body: JSON.stringify({ id: 'claude' }),
    });
    assert.equal(badOrigin.status, 403, 'spoofed Origin on /api/harnesses/launch must be 403');

    const badEventOrigin = await fetch(`${base}/event`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Origin: 'https://evil.com' },
      body: JSON.stringify({ session_id: 'bad', hook_event_name: 'SessionStart' }),
    });
    assert.equal(badEventOrigin.status, 403, 'spoofed Origin on /event must be 403');

    // 2. All 5 agents SessionStart + UserPromptSubmit + PostToolUse
    for (const agent of ['claude', 'codex', 'opencode', 'zcode', 'antigravity']) {
      const r1 = await fetch(`${base}/event`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          agent,
          session_id: `s-${agent}`,
          hook_event_name: 'SessionStart',
          cwd: `D:/repos/${agent}-demo`,
        }),
      });
      assert.equal(r1.status, 200);
      const r2 = await fetch(`${base}/event`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          agent,
          session_id: `s-${agent}`,
          hook_event_name: 'UserPromptSubmit',
          prompt: `check ${agent} with token sk-secret1234567890abcdef`,
        }),
      });
      assert.equal(r2.status, 200);
    }

    // 3. Hook endpoints
    for (const [ep, payload] of [
      ['/hook/loop', { session_id: 's-claude', tool_name: 'Bash', tool_input: { command: 'ls' } }],
      ['/hook/status', { session_id: 's-claude', cost: { total_cost_usd: 0.12 } }],
      ['/hook/start', { session_id: 's-claude' }],
      ['/hook/permission', { session_id: 's-claude', tool_name: 'Bash' }],
      ['/hook/question', { session_id: 's-claude', tool_name: 'AskUserQuestion' }],
      ['/hook/denied', { session_id: 's-claude', tool_name: 'Bash' }],
    ]) {
      const r = await fetch(`${base}${ep}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      assert.equal(r.status, 200, `${ep} must return 200`);
    }

    // 4. OTLP Oracle fixtures
    for (const file of ['metrics-delta-1.json', 'metrics-delta-2.json']) {
      const raw = fs.readFileSync(path.join(ORACLE_FIXTURES, file), 'utf8');
      const r = await fetch(`${base}/v1/metrics`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: raw,
      });
      assert.equal(r.status, 200, `OTLP ${file} must return 200`);
      const body = await r.json();
      assert.ok(body.accepted > 0, `OTLP ${file} must accept data points`);
    }
    const logsRaw = fs.readFileSync(path.join(ORACLE_FIXTURES, 'logs.json'), 'utf8');
    const logsRes = await fetch(`${base}/v1/logs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: logsRaw,
    });
    assert.equal(logsRes.status, 200);

    // 5. SSE /stream receives initial snapshot
    const sseCtrl = new AbortController();
    const sseRes = await fetch(`${base}/stream`, { signal: sseCtrl.signal });
    assert.equal(sseRes.status, 200);
    const reader = sseRes.body.getReader();
    const { value } = await reader.read();
    const chunk = new TextDecoder().decode(value);
    assert.ok(chunk.includes('"k":"snap"'), `SSE must emit initial snapshot, got: ${chunk.slice(0, 120)}`);
    sseCtrl.abort();

    // 6. Core APIs + Persistence mutations
    const live = await (await fetch(`${base}/api/live`)).json();
    assert.equal(live.sessions.length, 5, 'all 5 agents must appear in /api/live');

    const vault = await (await fetch(`${base}/api/vault?q=check`)).json();
    assert.ok(vault.items.length >= 1);
    assert.ok(!JSON.stringify(vault).includes('sk-secret1234567890abcdef'), 'vault must redact secrets');

    await fetch(`${base}/api/prefs`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ lang: 'vi' }),
    });
    await fetch(`${base}/api/pins`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ sid: 's-claude', text: 'remember rust parity' }),
    });
    await fetch(`${base}/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: 'hello station' }),
    });

    for (const ep of [
      '/api/doctor', '/api/sys', '/api/burn', '/api/wrapped', '/api/replay/days',
      '/api/undo?sid=s-claude', '/api/chat', '/api/prefs', '/api/pins', '/api/harnesses',
    ]) {
      const r = await fetch(`${base}${ep}`);
      assert.equal(r.status, 200, `GET ${ep} must return 200`);
    }

    const harnesses = await (await fetch(`${base}/api/harnesses`)).json();
    assert.equal(harnesses.harnesses.length, 5, '5 built-in harnesses listed');

    // 7. Native `agents-station.exe hook` subprocess tests
    const hookOut = execFileSync(BIN, ['hook', `${base}/event`, '--agent', 'claude', '--event', 'PostToolUse'], {
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

    // 8. Graceful restart persistence check
    await stopServer(proc);
    port = await getFreePort();
    proc = startServer(port, dataDir);
    await waitForHealth(port);
    const base2 = `http://127.0.0.1:${port}`;
    const prefs2 = await (await fetch(`${base2}/api/prefs`)).json();
    assert.equal(prefs2.lang, 'vi', 'prefs.json must persist across restart');
    const pins2 = await (await fetch(`${base2}/api/pins`)).json();
    assert.deepEqual(pins2.pins['s-claude'], ['remember rust parity'], 'pins.json must persist across restart');

    // 9. 70-second idle soak: RAM <= 35 MB & 0 powershell.exe children
    console.log(`[G1] Starting ${SOAK_SECONDS}s idle soak (target <= 35 MB Private Working Set, 0 PowerShell)...`);
    let maxMb = 0;
    let totalPsKids = 0;
    const steps = Math.max(1, Math.ceil(SOAK_SECONDS / 10));
    for (let i = 0; i <= steps; i++) {
      // Trigger /api/sys periodically during soak to prove Win32 system monitor spawns 0 powershell.exe
      const sysRes = await fetch(`${base2}/api/sys`);
      assert.equal(sysRes.status, 200);
      const m = sampleProcessMetrics(proc.pid);
      assert.ok(m.privateMb > 0, `server pid ${proc.pid} disappeared during soak`);
      maxMb = Math.max(maxMb, m.privateMb);
      totalPsKids += m.powershellChildren;
      console.log(`  [soak ${i * 10}s/${SOAK_SECONDS}s] PrivateWS=${m.privateMb.toFixed(2)} MB, powershell_children=${m.powershellChildren}`);
      if (i < steps) await new Promise((r) => setTimeout(r, 10_000));
    }

    assert.equal(totalPsKids, 0, `expected 0 powershell.exe children, saw ${totalPsKids}`);
    assert.ok(maxMb <= 35.0, `idle serve Private Working Set ${maxMb.toFixed(2)} MB exceeded 35 MB budget`);
    console.log(`[G1] PASS! Peak PrivateWS=${maxMb.toFixed(2)} MB (<= 35 MB), PowerShell children=0.`);
  } finally {
    await stopServer(proc);
    fs.rmSync(dataDir, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error('[G1] FAIL:', err);
  process.exit(1);
});
