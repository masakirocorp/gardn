#!/usr/bin/env node
import path from 'node:path';
import { createHash } from 'node:crypto';
import { connectionReceiptFile, readJson, readRecords, saveConnectionReceipt, saveRecord, withConnectionLock, withLock } from './store.mjs';
import { SpriteProvider, spawnProvider } from './provider.mjs';

const [stateDir, spriteId, mode, sessionId = '', conversationRef = '', operationId = '', ownerPidArg = '', attemptId = ''] = process.argv.slice(2);
const ownerPid = Number(ownerPidArg);
const fail = message => { process.stderr.write(`${message}\n`); process.exitCode = 1; };
const killTransport = child => {
  try {
    if (process.platform === 'win32') child.kill('SIGKILL');
    else process.kill(-child.pid, 'SIGKILL');
  } catch {}
};
const hash = value => createHash('sha256').update(value).digest('hex');
const now = () => Date.now();
function receipt(status, fields = {}) {
  if (!stateDir || !spriteId || !operationId || !attemptId) return;
  const error = fields.error ? { code: fields.error.code || 'connection_failed', message: fields.error.message, retryable: fields.error.retryable === true } : null;
  saveConnectionReceipt(stateDir, spriteId, operationId, { sprite_id: spriteId, operation_id: operationId, attempt_id: attemptId, mode, status, session_id: fields.session_id ?? null, error, updated_unix_ms: now() });
}
function saveStartedSession(record, sessions, started) {
  return withLock(stateDir, record.id, () => {
    const current = readRecords(stateDir).find(item => item.id === record.id);
    if (!current) throw new Error('Sprite record disappeared while observing startup.');
    current.sessions = sessions.map(session => ({ ...session, owned: (session.id === started.id && typeof session.created === 'string' && session.created === started.created) || current.sessions.some(old => old.id === session.id && old.created === session.created && typeof session.created === 'string' && old.owned) }));
    current.phase = current.sessions.some(session => session.tty) ? 'running' : 'ready';
    current.observed_unix_ms = now(); current.updated_unix_ms = now(); current.revision++;
    saveRecord(stateDir, current);
  });
}
async function main() {
  if (!stateDir || !path.isAbsolute(stateDir) || !spriteId || !operationId || !attemptId || !Number.isInteger(ownerPid) || ownerPid < 1 || !['connect', 'start', 'resume', 'shell'].includes(mode)) throw Object.assign(new Error('Invalid Sprite connection arguments.'), { code: 'invalid_request' });
  const config = readJson(path.join(stateDir, 'backend-config.json'));
  if (config.enabled !== true) throw Object.assign(new Error('Sprites integration is disabled; refusing to connect.'), { code: 'disabled' });
  try { process.kill(ownerPid, 0); } catch (error) { if (error.code === 'ESRCH') throw Object.assign(new Error('Owning Gardn coordinator is no longer alive.'), { code: 'owner_unavailable' }); }
  const record = readRecords(stateDir).find(item => item.id === spriteId);
  if (!record || record.phase === 'destroyed') throw Object.assign(new Error('Sprite resource is unavailable in the local catalog.'), { code: 'unknown_resource' });
  if (!record.managed && mode !== 'connect' && mode !== 'shell') throw Object.assign(new Error('Starting or opening a shell in a foreign Sprite is not permitted.'), { code: 'foreign_resource' });
  if (mode === 'connect' && !sessionId) throw Object.assign(new Error('Connect requires an exact existing session ID.'), { code: 'session_required' });
  if (mode === 'resume' && (!conversationRef || /[\0\r\n]/.test(conversationRef))) throw Object.assign(new Error('Resume requires an exact conversation reference.'), { code: 'invalid_conversation_ref' });
  const provider = new SpriteProvider({ ...config, org: record.org }, { ownerPid, fencePath: path.join(stateDir, 'backend-config.json') });
  const workspace = `/home/sprite/gardn/${hash(record.id)}/workspace`;
  const command = [...(record.agent?.command ?? [])];
  if (mode === 'resume') {
    if (record.agent?.kind === 'codex') command.push('resume', conversationRef);
    else if (record.agent?.kind === 'claude') command.push('--resume', conversationRef);
    else throw Object.assign(new Error(`Resume is not supported for agent kind ${record.agent?.kind ?? '(unknown)'}.`), { code: 'resume_unsupported' });
  }
  const envFile = `/home/sprite/gardn/${hash(record.id)}/agent-env.json`;
  const envRunner = `const fs=require('fs'),cp=require('child_process');let env=process.env;try{env={...env,...JSON.parse(fs.readFileSync(${JSON.stringify(envFile)},'utf8'))}}catch(e){if(e.code!=='ENOENT')throw e}const r=cp.spawnSync(process.argv[1],process.argv.slice(2),{stdio:'inherit',env});if(r.error)throw r.error;process.exit(r.status??1)`;
  const marker = `GARDN_SPRITE_OPERATION=${hash(operationId)}`;
  const invocation = `export ${marker} && cd ${JSON.stringify(workspace)} && exec node -e ${JSON.stringify(envRunner)} "$@"`;
  let args = ['-o', record.org, '-s', record.name, ...(mode === 'connect'
    ? ['sessions', 'attach', sessionId, '--no-port-forward']
    : ['exec', '--tty', '--no-port-forward', '--', ...(mode === 'shell'
      ? ['/bin/sh', '-lc', `export ${marker} && cd ${JSON.stringify(record.managed ? workspace : '/home/sprite')} && exec /bin/sh`]
      : ['/bin/sh', '-lc', invocation, 'gardn', ...command])])];

  await withConnectionLock(stateDir, spriteId, () => {
    const currentConfig = readJson(path.join(stateDir, 'backend-config.json'));
    if (currentConfig.enabled !== true) throw Object.assign(new Error('Sprites integration was disabled before connection launch.'), { code: 'disabled' });
    try { process.kill(ownerPid, 0); } catch (error) { if (error.code === 'ESRCH') throw Object.assign(new Error('Owning Gardn coordinator exited before connection launch.'), { code: 'owner_unavailable' }); }
    const before = mode === 'connect' ? [] : provider.sessions(record.name);
    if (mode === 'connect' && !provider.sessions(record.name).some(session => session.id === sessionId)) throw Object.assign(new Error('The exact requested session is no longer present.'), { code: 'session_missing' });
    const markerMatches = mode === 'connect' ? [] : provider.sessionsForMarker(record.name, marker, before);
    let existingReceipt = null;
    if (mode !== 'connect') {
      try { existingReceipt = readJson(connectionReceiptFile(stateDir, spriteId, operationId)); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      if (markerMatches.length > 1) throw Object.assign(new Error('Multiple remote sessions match this connection operation; refusing to attach ambiguously.'), { code: 'session_ambiguous' });
      if (markerMatches.length === 1) args = ['-o', record.org, '-s', record.name, 'sessions', 'attach', markerMatches[0].id, '--no-port-forward'];
      else if (existingReceipt) throw Object.assign(new Error('Prior connection outcome is uncertain; inspect Sprite sessions before retrying this operation.'), { code: 'connection_unconfirmed', retryable: true });
    }
    const recoveredSession = markerMatches[0] ?? null;
    receipt('starting');
    return new Promise((resolve, reject) => {
      // Keep the caller's real terminal FDs while isolating the provider into
      // its own process group, so dropping this bridge never signals the remote TTY.
      const child = spawnProvider(currentConfig.sprite_bin, args, { stdio: 'inherit', windowsHide: true, detached: true, env: process.env });
      let settled = false, acknowledged = false, startupFailed = false, detached = false, transportReceipt;
      const startupFailure = (code, message, retryable = false) => {
        if (acknowledged || settled || startupFailed) return;
        startupFailed = true;
        receipt('failed', { error: { code, message, retryable } });
        killTransport(child);
      };
      const detach = () => {
        detached = true;
        if (!acknowledged && !startupFailed) {
          startupFailed = true;
          receipt('failed', { error: { code: 'connection_detached', message: 'Connection ended before remote session startup could be confirmed.', retryable: true } });
        }
        killTransport(child);
      };
      const resize = () => { if (process.platform !== 'win32') { try { process.kill(-child.pid, 'SIGWINCH'); } catch {} } };
      const poll = setInterval(() => {
        if (settled || acknowledged || (mode !== 'start' && mode !== 'resume' && mode !== 'shell')) return;
        try {
          const liveConfig = readJson(path.join(stateDir, 'backend-config.json'));
          if (liveConfig.enabled !== true) return startupFailure('disabled', 'Sprites integration was disabled before remote session startup was confirmed.');
          const sessions = provider.sessions(record.name);
          const candidates = provider.sessionsForMarker(record.name, marker, sessions)
            .filter(session => !recoveredSession || session.id === recoveredSession.id);
          if (candidates.length === 1) {
            const started = candidates[0];
            try { saveStartedSession(record, sessions, started); }
            catch (error) { if (error.code === 'resource_busy') return; throw error; }
            acknowledged = true; receipt('started', { session_id: started.id });
          } else if (candidates.length > 1) startupFailure('session_ambiguous', 'More than one new remote session matched this operation; startup remains unconfirmed.');
        } catch (error) {
          if (['authentication_required', 'provider_unavailable', 'provider_protocol', 'session_ambiguous'].includes(error.code)) startupFailure(error.code, error.message, error.code !== 'session_ambiguous');
        }
      }, 1000);
      poll.unref();
      const startupDeadline = setTimeout(() => startupFailure('connection_timeout', 'Provider did not expose the exact started session before timeout; remote outcome is unknown. Inspect before retrying.', true), 60_000);
      startupDeadline.unref();
      const finish = (error, code, signal) => {
        if (settled) return;
        settled = true; clearInterval(poll); clearTimeout(startupDeadline); clearTimeout(transportReceipt);
        for (const s of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.off(s, detach);
        process.off('SIGWINCH', resize);
        if (error || (mode === 'connect' && code !== 0 && !detached)) receipt('failed', { error: { code: error?.code || 'connection_failed', message: error ? 'Sprite provider connection failed before remote attachment was confirmed.' : `Sprite provider transport exited before remote attachment was confirmed (status ${code ?? signal ?? 'unknown'}).`, retryable: true } });
        else if (!acknowledged && !startupFailed) receipt('failed', { error: { code: 'connection_exited', message: `Provider connection exited before startup was confirmed (status ${code ?? signal ?? 'unknown'}).`, retryable: true } });
        if (error) reject(error);
        else if (signal) resolve();
        else { process.exitCode = code ?? 1; resolve(); }
      };
      for (const s of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(s, detach);
      process.on('SIGWINCH', resize);
      child.once('error', error => {
        startupFailed = true;
        receipt('failed', { error: { code: 'provider_unavailable', message: `Sprite connection failed: ${error.message}`, retryable: true } });
        finish(error);
      });
      child.once('spawn', () => {
        if (mode === 'connect') {
          transportReceipt = setTimeout(() => {
            if (!settled) { acknowledged = true; receipt('transport_open', { session_id: sessionId }); }
          }, 250);
          transportReceipt.unref();
        }
      });
      child.once('exit', (code, signal) => finish(null, code, signal));
    });
  });
}

main().catch(error => {
  try { receipt('failed', { error: { code: error.code || 'connection_failed', message: String(error.message || 'Sprite connection failed').slice(0, 800), retryable: error.retryable === true } }); } catch {}
  fail(String(error.message || 'Sprite connection failed'));
});
