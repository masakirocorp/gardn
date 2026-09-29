import fs from 'node:fs';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';

const OUTPUT_LIMIT = 8 * 1024 * 1024;
const nodeScript = bin => ['.mjs', '.cjs', '.js'].includes(path.extname(bin).toLowerCase());
export function spawnProvider(bin, args, options) {
  return nodeScript(bin) ? spawn(process.execPath, [bin, ...args], options) : spawn(bin, args, options);
}
function run(bin, args, { input, timeout = 30_000, maxBuffer = OUTPUT_LIMIT } = {}) {
  const node = nodeScript(bin);
  const result = spawnSync(node ? process.execPath : bin, node ? [bin, ...args] : args, { encoding: 'utf8', input, timeout, maxBuffer, windowsHide: true, env: { ...process.env, GIT_CONFIG_NOSYSTEM: '1' } });
  if (result.error) throw Object.assign(new Error(result.error.code === 'ENOENT' ? 'Sprite provider executable is unavailable.' : 'Sprite provider command failed before returning.'), { code: result.error.code === 'ENOENT' ? 'provider_unavailable' : 'provider_error' });
  if (result.status !== 0) {
    const raw = String(result.stderr || '');
    const code = result.status === 127 ? 'provider_unavailable' : /unauthori[sz]ed|not authenticated|login required|invalid credentials/i.test(raw) ? 'authentication_required' : /not found|no such sprite/i.test(raw) ? 'resource_missing' : 'provider_error';
    const message = code === 'authentication_required' ? 'Sprite provider authentication is required.'
      : code === 'resource_missing' ? 'The exact Sprite resource was not found.'
      : code === 'provider_unavailable' ? 'Sprite provider executable is unavailable.'
      : `Sprite provider command failed (status ${result.status}).`;
    throw Object.assign(new Error(message), { code });
  }
  return String(result.stdout ?? '');
}
function assertOwner(ownerPid) {
  if (!Number.isInteger(ownerPid) || ownerPid < 1) throw Object.assign(new Error('Sprites operation has no valid owning coordinator PID.'), { code: 'owner_unavailable' });
  try { process.kill(ownerPid, 0); }
  catch (error) { if (error.code === 'ESRCH') throw Object.assign(new Error('Sprites owning Gardn coordinator exited; provider action was fenced.'), { code: 'owner_unavailable' }); }
}
function parseJson(text, what) {
  try { return JSON.parse(text); } catch { throw Object.assign(new Error(`Sprite provider returned invalid structured ${what}.`), { code: 'provider_protocol' }); }
}
const safe = value => typeof value === 'string' && value.length > 0 && !/[\0\r\n]/.test(value);
export class SpriteProvider {
  constructor(config, { ownerPid = process.ppid, fencePath = null } = {}) {
    this.bin = config.sprite_bin; this.org = config.org; this.ownerPid = ownerPid; this.fencePath = fencePath;
  }
  _run(args, options) {
    assertOwner(this.ownerPid);
    if (this.fencePath) {
      let fence;
      try { fence = JSON.parse(fs.readFileSync(this.fencePath, 'utf8')); }
      catch { throw Object.assign(new Error('Sprites configuration fence is unavailable; provider action refused.'), { code: 'disabled' }); }
      if (fence.enabled !== true) throw Object.assign(new Error('Sprites integration is disabled; provider action refused.'), { code: 'disabled' });
    }
    return run(this.bin, args, options);
  }
  args(name, argv) {
    if (!safe(this.org) || !safe(name)) throw Object.assign(new Error('Sprite organization and exact resource name are required.'), { code: 'invalid_target' });
    return ['-o', this.org, '-s', name, ...argv];
  }
  inventory() {
    const data = parseJson(this._run(['-o', this.org, 'api', '/v1/sprites', '--', '-fsS', '--max-time', '30']), 'inventory');
    const sprites = Array.isArray(data) ? data : data.sprites;
    if (!Array.isArray(sprites)) throw Object.assign(new Error('Sprite inventory response has no sprites array.'), { code: 'provider_protocol' });
    return sprites.map(item => {
      const name = item?.name ?? item?.id;
      if (!safe(name)) throw Object.assign(new Error('Sprite inventory contains a resource without a valid name.'), { code: 'provider_protocol' });
      return { name, state: typeof item.state === 'string' ? item.state : typeof item.status === 'string' ? item.status : null };
    });
  }
  sessions(name) {
    const data = parseJson(this._run(this.args(name, ['api', `/v1/sprites/${encodeURIComponent(name)}/exec`, '--', '-fsS', '--max-time', '30'])), 'sessions');
    if (!Array.isArray(data.sessions)) throw Object.assign(new Error('Sprite sessions response has no sessions array.'), { code: 'provider_protocol' });
    return data.sessions.map(s => {
      if (s?.id === undefined || s?.id === null || !String(s.id)) throw Object.assign(new Error('Sprite session inventory contains an invalid session ID.'), { code: 'provider_protocol' });
      const command = Array.isArray(s.command) ? s.command.map(String) : typeof s.command === 'string' ? [s.command] : [];
      return { id: String(s.id), created: typeof s.created === 'string' ? s.created : null, command, tty: s.tty === true, owned: false };
    });
  }
  sessionsForMarker(name, marker, sessions = this.sessions(name)) {
    const script = `const fs=require('node:fs'),marker=process.argv[1],out=[];for(const pid of fs.readdirSync('/proc')){if(!/^\\d+$/.test(pid))continue;try{const env=fs.readFileSync('/proc/'+pid+'/environ','utf8').split('\\0');if(!env.includes(marker))continue;const stat=fs.readFileSync('/proc/'+pid+'/stat','utf8'),fields=stat.slice(stat.lastIndexOf(')')+2).trim().split(/\\s+/),sid=Number(fields[3]);if(Number.isSafeInteger(sid)&&sid>0)out.push({marker,sid})}catch{}}process.stdout.write(JSON.stringify(out))`;
    const data = parseJson(this.exec(name, ['node', '-e', script, marker], { timeout: 10_000 }), 'operation session identity');
    if (!Array.isArray(data)) throw Object.assign(new Error('Sprite process identity response has no array.'), { code: 'provider_protocol' });
    const ids = new Set(data.filter(item => item?.marker === marker && Number.isSafeInteger(item.sid) && item.sid > 0).map(item => String(item.sid)));
    if (ids.size > 1) throw Object.assign(new Error('Multiple remote process sessions match this connection operation.'), { code: 'session_ambiguous' });
    return sessions.filter(session => session.tty && ids.has(session.id));
  }
  create(name) { this._run(['-o', this.org, 'create', name, '--skip-console']); }
  destroy(name) { this._run(['-o', this.org, 'destroy', name, '--force']); }
  kill(name, sessionId) { this._run(this.args(name, ['sessions', 'kill', sessionId])); }
  checkpoint(name, comment) {
    const text = this._run(this.args(name, ['checkpoint', 'create', '--comment', comment]));
    const id = text.replace(/\x1b\[[0-9;]*m/g, '').match(/Checkpoint\s+(v[\w.-]+)\s+created/i)?.[1] ?? text.match(/sprite restore\s+(v[\w.-]+)/i)?.[1];
    if (!id) throw Object.assign(new Error('Checkpoint command returned no version identifier.'), { code: 'provider_protocol' });
    return id;
  }
  checkpoints(name) {
    const data = parseJson(this._run(this.args(name, ['api', `/v1/sprites/${encodeURIComponent(name)}/checkpoints`, '--', '-fsS', '--max-time', '30'])), 'checkpoint inventory');
    const rows = Array.isArray(data) ? data : data.checkpoints;
    if (!Array.isArray(rows)) throw Object.assign(new Error('Checkpoint response has no checkpoints array.'), { code: 'provider_protocol' });
    return rows.map(row => String(typeof row === 'string' ? row : row?.id ?? row?.version ?? '')).filter(id => id && id !== 'Current');
  }
  restore(name, checkpoint) { this._run(this.args(name, ['restore', checkpoint])); }
  exec(name, args, options = {}) { return this._run(this.args(name, ['exec', '--no-port-forward', '--', ...args]), options); }
}
export { run };
