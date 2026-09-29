import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

import { applyPull, preview as previewTransfer, snapshot } from '../transfer.mjs';
import { connectionLockFile, consumeApproval, hasActiveConnection, saveApproval, withConnectionLock, withLock } from '../store.mjs';
const worker = fileURLToPath(new URL('../worker.mjs', import.meta.url));
const hash = value => createHash('sha256').update(value).digest('hex');
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gardn-sprites-'));
  const db = path.join(root, 'provider.json');
  fs.writeFileSync(db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [] }));
  const bin = path.join(root, 'sprite-fake.mjs');
  fs.writeFileSync(bin, `import fs from 'node:fs';
const d=process.env.SPRITES_FAKE_DB,a=process.argv.slice(2),s=JSON.parse(fs.readFileSync(d,'utf8'));
const save=()=>fs.writeFileSync(d,JSON.stringify(s)),out=x=>process.stdout.write(JSON.stringify(x));
const api=a.indexOf('api'),route=api>=0?a[api+1]:'';
if(route==='/v1/sprites'){if(s.authError){process.stderr.write('not authenticated\\\\n');process.exit(2)}out({sprites:s.sprites});process.exit(0)}
if(route.endsWith('/checkpoints')){out({checkpoints:s.checkpoints??[]});process.exit(0)}
if(route.endsWith('/exec')){out({sessions:s.sessions??[]});process.exit(0)}
if(a.includes('checkpoint')&&a.includes('create')){const id='v'+(s.checkpoints.filter(row=>row.id!=='Current').length+1);s.checkpoints.push({id});save();process.stdout.write('Checkpoint '+id+' created\\n');process.exit(0)}
if(a.includes('create')){if(s.createFailure){process.stderr.write(s.createFailure);process.exit(2)}if(s.delayMs)Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,s.delayMs);s.creates++;s.sprites.push({name:a[a.indexOf('create')+1],state:'running'});save();process.exit(0)}
if(a.includes('destroy')){if(s.destroyGate){fs.writeFileSync(s.destroyGate+'.ready','ready');while(!fs.existsSync(s.destroyGate+'.release'))Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,10)}const name=a[a.indexOf('destroy')+1];s.sprites=s.sprites.filter(sprite=>sprite.name!==name);save();process.exit(0)}
if(a.includes('restore')){s.restored=a[a.indexOf('restore')+1];save();process.exit(0)}
if(a.includes('sessions')&&a.includes('kill')){const id=a[a.indexOf('kill')+1];s.sessions=(s.sessions??[]).filter(x=>x.id!==id);save();process.exit(0)}
if(a.includes('sessions')&&a.includes('attach')){if(s.attachFailure){process.stderr.write('remote attach rejected\\n');process.exit(2)}process.stdin.resume();process.stdin.on('end',()=>process.exit(0));setInterval(()=>{},1000)}
  if(a.some(x=>typeof x==='string'&&x.includes('invalid snapshot'))&&s.uploadGate){fs.writeFileSync(s.uploadGate+'.ready','ready');while(!fs.existsSync(s.uploadGate+'.release'))Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,10)}
if(a.includes('exec')){
  if(s.failExec){process.stderr.write('simulated remote exec failure\\n');process.exit(2)}
  if(a.includes('--tty')){const text=a.join(' '),marker=text.match(/GARDN_SPRITE_OPERATION=[a-f0-9]+/),sid=715,id=String(sid),created='2026-09-29T02:57:53.569345439Z';s.sessions??=[];s.markerProcesses??=[];s.sessions.push({id,created,command:'bash --noprofile --norc',tty:true});s.markerProcesses.push({marker:marker?.[0],sid});save();setInterval(()=>{},1000)}
  else if(a.join(' ').includes('GARDN_SPRITE_OPERATION')){out(s.markerProcesses??[]);process.exit(0)}
  else {if(a.join(' ').includes('command -v'))process.stdout.write(s.agentAvailable===false?'missing':'available');else if(s.execOutput)process.stdout.write(s.execOutput);else process.stdout.write('workspace-ready');process.exit(0)}
}
`);
  const gitRoot = path.join(root, 'workspace'); fs.mkdirSync(gitRoot);
  const git = spawnSync('git', ['init', gitRoot], { encoding: 'utf8' });
  assert.equal(git.status, 0);
  fs.writeFileSync(path.join(gitRoot, 'README.txt'), 'starter source\n');
  const state = path.join(root, 'state'); fs.mkdirSync(state);
  fs.writeFileSync(path.join(state, 'backend-config.json'), JSON.stringify({ enabled: true, org: 'acme', sprite_bin: bin, owner_pid: process.pid }));
  return { root, db, bin, gitRoot, state };
}
function invoke(f, action, params, operationId = 'create-request-1', configOverrides = {}) {
  const now = Date.now();
  const operation = { id: operationId, request: { request_id: operationId, action, params, focus: false }, status: 'queued', stage: 'queued', created_unix_ms: now, updated_unix_ms: now, result: null, error: null };
  const config = { enabled: true, org: 'acme', sprite_bin: f.bin, node_bin: process.execPath, name_prefix: 'gardn-', max_sprites: 4, max_concurrent_operations: 2, max_transfer_mib: 64, ...configOverrides };
  const result = spawnSync(process.execPath, [worker], { input: JSON.stringify({ config, state_dir: f.state, owner_pid: process.pid, operation }), encoding: 'utf8', env: { ...process.env, SPRITES_FAKE_DB: f.db }, maxBuffer: 4 * 1024 * 1024 });
  const frames = result.stdout.trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
  return { ...result, frames };
}

function launch(f, action, params, operationId, configOverrides = {}) {
  const now = Date.now();
  const operation = { id: operationId, request: { request_id: operationId, action, params, focus: false }, status: 'queued', stage: 'queued', created_unix_ms: now, updated_unix_ms: now, result: null, error: null };
  const config = { enabled: true, org: 'acme', sprite_bin: f.bin, node_bin: process.execPath, name_prefix: 'gardn-', max_sprites: 4, max_concurrent_operations: 2, max_transfer_mib: 64, ...configOverrides };
  const child = spawn(process.execPath, [worker], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, SPRITES_FAKE_DB: f.db } });
  let stdout = '', stderr = '';
  child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
  child.stdout.on('data', value => { stdout += value; }); child.stderr.on('data', value => { stderr += value; });
  const done = new Promise(resolve => child.once('close', status => resolve({ status, stdout, stderr, frames: stdout.trim().split('\n').filter(Boolean).map(line => JSON.parse(line)) })));
  child.stdin.end(JSON.stringify({ config, state_dir: f.state, owner_pid: process.pid, operation }));
  return { child, done };
}

async function until(predicate, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  throw new Error('Timed out waiting for backend state.');
}

test('creation persists an intended managed resource and repeated request does not create twice', () => {
  const f = fixture();
  try {
    const params = { workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const first = invoke(f, 'create', params);
    assert.equal(first.status, 0, first.stdout + first.stderr);
    const firstOperation = first.frames.findLast(frame => frame.type === 'operation').operation;
    assert.equal(firstOperation.status, 'succeeded');
    assert.equal(firstOperation.result.kind, 'resource');
    assert.equal(firstOperation.result.data.phase, 'ready');
    const saved = JSON.parse(fs.readFileSync(path.join(f.state, 'resources', hash('acme/gardn-' + hash('create-request-1').slice(0, 18)), 'record.json'), 'utf8'));
    assert.equal(saved.managed, true);
    assert.equal(saved.workspace_id, 'workspace-1');
    assert.equal(saved.phase, 'ready');
    const retry = invoke(f, 'create', params);
    assert.equal(retry.status, 0, retry.stdout + retry.stderr);
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('pull previews and applies remote edits without changing the local Git index', () => {
  const f = fixture();
  try {
    const params = { workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const created = invoke(f, 'create', params).frames.findLast(frame => frame.type === 'operation').operation;
    assert.equal(created.status, 'succeeded', JSON.stringify(created.error));
    const target = { sprite_id: created.result.data.id };
    const staged = spawnSync('git', ['-C', f.gitRoot, 'add', 'README.txt']);
    assert.equal(staged.status, 0);
    const index = fs.readFileSync(path.join(f.gitRoot, '.git', 'index'));
    const state = JSON.parse(fs.readFileSync(f.db, 'utf8'));
    state.execOutput = JSON.stringify({ version: 1, files: [{ path: 'README.txt', mode: 420, data: Buffer.from('remote edit\n').toString('base64') }], excluded: [] });
    fs.writeFileSync(f.db, JSON.stringify(state));
    const preview = invoke(f, 'pull_preview', target, 'preview').frames.findLast(frame => frame.type === 'operation').operation;
    assert.equal(preview.status, 'succeeded', JSON.stringify(preview.error));
    assert.deepEqual(preview.result.data.changed_paths, ['README.txt']);
    assert.equal(fs.readFileSync(path.join(f.gitRoot, 'README.txt'), 'utf8'), 'starter source\n');
    const pulled = invoke(f, 'pull', target, 'pull').frames.findLast(frame => frame.type === 'operation').operation;
    assert.equal(pulled.status, 'succeeded', JSON.stringify(pulled.error));
    assert.equal(fs.readFileSync(path.join(f.gitRoot, 'README.txt'), 'utf8'), 'remote edit\n');
    assert.deepEqual(fs.readFileSync(path.join(f.gitRoot, '.git', 'index')), index);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('inventory authentication failure preserves previously managed resources', () => {
  const f = fixture();
  try {
    const id = 'acme/prior';
    const dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'prior', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { kind: 'claude', command: ['claude'] }, phase: 'ready', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 3, updated_unix_ms: 10, observed_unix_ms: 9, last_error: null, unpulled_changes: null }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, authError: true }));
    const run = invoke(f, 'list', { refresh: true }, 'list-1');
    assert.equal(run.status, 1);
    assert.equal(run.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'authentication_required');
    assert.equal(run.frames.find(frame => frame.type === 'snapshot').snapshot.observed_unix_ms, null);
    const retained = run.frames.find(frame => frame.type === 'snapshot').snapshot.resources;
    assert.equal(retained.length, 1);
    assert.equal(retained[0].phase, 'ready');
    assert.equal(retained[0].observed_unix_ms, 9);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('refresh returns the full catalog but does not refresh another organization', () => {
  const f = fixture();
  try {
    const id = 'other/prior', observed = 123;
    const dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'other', name: 'prior', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: null, phase: 'ready', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 2, updated_unix_ms: observed, observed_unix_ms: observed, last_error: null, unpulled_changes: null }));
    const run = invoke(f, 'list', { refresh: true }, 'list-other');
    assert.equal(run.status, 0, run.stderr);
    const resources = run.frames.find(frame => frame.type === 'snapshot').snapshot.resources;
    assert.equal(resources.length, 1);
    assert.equal(resources[0].id, id);
    assert.equal(resources[0].observed_unix_ms, observed);
    assert.equal(resources[0].phase, 'ready');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('installation-wide managed Sprite limit blocks a second create', () => {
  const f = fixture();
  try {
    const params = { workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const first = invoke(f, 'create', params, 'create-1', { max_sprites: 1 });
    assert.equal(first.status, 0, first.stderr);
    const second = invoke(f, 'create', { ...params, name: 'explicit-second' }, 'create-2', { max_sprites: 1 });
    assert.equal(second.status, 1);
    assert.equal(second.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'resource_limit');
    const observation = second.frames.find(frame => frame.type === 'snapshot').snapshot;
    assert.equal(observation.observation_error, null);
    assert.equal(observation.observed_unix_ms, null);
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('Connect returns an exact existing session plan and never starts a session', () => {
  const f = fixture();
  try {
    const id = 'acme/foreign'; const dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'foreign', managed: false, workspace_id: null, source: null, agent: null, phase: 'available', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'foreign', state: 'running' }], sessions: [{ id: 'session-exact', command: ['shell'], tty: true }] }));
    const run = invoke(f, 'connect', { sprite_id: id, session_id: 'session-exact' }, 'connect-1');
    assert.equal(run.status, 0, run.stderr);
    const connection = run.frames.findLast(frame => frame.type === 'operation').operation.result;
    assert.equal(connection.kind, 'connection');
    assert.equal(connection.data.session_id, 'session-exact');
    assert.equal(connection.data.starts_session, false);
    assert.equal(connection.data.args[2], id);
    assert.equal(connection.data.args[3], 'connect');
    assert.match(connection.data.attempt_id, /^[0-9a-f-]{36}$/);
    assert.equal(connection.data.args.at(-1), connection.data.attempt_id);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('Connect reports a failed receipt when the provider rejects attachment immediately', async () => {
  const f = fixture();
  try {
    const id = 'acme/foreign';
    const dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'foreign', managed: false, workspace_id: null, source: null, agent: null, phase: 'available', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'foreign', state: 'running' }], sessions: [{ id: 'session-exact', command: ['shell'], tty: true }], attachFailure: true }));
    const planned = invoke(f, 'connect', { sprite_id: id, session_id: 'session-exact' }, 'connect-fails');
    const connection = planned.frames.findLast(frame => frame.type === 'operation').operation.result.data;
    const bridge = spawn(process.execPath, connection.args, { stdio: 'ignore', env: { ...process.env, SPRITES_FAKE_DB: f.db } });
    await new Promise(resolve => bridge.once('close', resolve));
    const receiptFile = path.join(dir, 'connections', `${hash('connect-fails')}.json`);
    const receipt = JSON.parse(fs.readFileSync(receiptFile, 'utf8'));
    assert.equal(receipt.attempt_id, connection.attempt_id);
    assert.equal(receipt.status, 'failed');
    assert.equal(receipt.error.code, 'connection_failed');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('provider session strings are retained for display without claiming same-workspace sessions', () => {
  const f = fixture();
  try {
    const id = 'acme/managed', dir = path.join(f.state, 'resources', hash(id));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'managed', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { kind: 'claude', command: ['claude'] }, phase: 'ready', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    const command = 'bash --noprofile --norc';
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'managed', state: 'running' }], sessions: [{ id: '715', command, workdir: `/home/sprite/gardn/${hash(id)}/workspace`, tty: true }] }));

    const inspected = invoke(f, 'inspect', { sprite_id: id }, 'inspect-string-command');
    assert.equal(inspected.status, 0, inspected.stderr);
    const session = inspected.frames.findLast(frame => frame.type === 'operation').operation.result.data.sessions[0];
    assert.deepEqual(session.command, [command]);
    assert.equal(session.owned, false);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});
test('a reused provider session ID with a different creation time cannot authorize Stop', () => {
  const f = fixture();
  try {
    const id = 'acme/managed', dir = path.join(f.state, 'resources', hash(id));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'managed', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { kind: 'claude', command: ['claude'] }, phase: 'running', provider_state: 'running', sessions: [{ id: '715', created: '2026-09-28T10:00:00Z', command: ['old process'], tty: true, owned: true }], checkpoint_id: null, revision: 2, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    const current = { id: '715', created: '2026-09-29T10:00:00Z', command: 'bash --noprofile --norc', workdir: `/home/sprite/gardn/${hash(id)}/workspace`, tty: true };
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'managed', state: 'running' }], sessions: [current] }));

    const stopped = invoke(f, 'stop', { sprite_id: id, session_id: '715' }, 'stop-reused-session-id');
    assert.equal(stopped.status, 1);
    assert.equal(stopped.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'session_not_owned');
    assert.deepEqual(JSON.parse(fs.readFileSync(f.db, 'utf8')).sessions, [current]);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('stopping one owned session preserves control of the other owned session', () => {
  const f = fixture();
  try {
    const id = 'acme/managed', dir = path.join(f.state, 'resources', hash(id));
    fs.mkdirSync(dir, { recursive: true });
    const sessions = [
      { id: '715', created: '2026-09-29T10:00:00Z', command: ['bash'], tty: true, owned: true },
      { id: '800', created: '2026-09-29T11:00:00Z', command: ['bash'], tty: true, owned: true },
    ];
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'managed', managed: true, sessions, phase: 'running', revision: 1 }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'managed' }], sessions }));
    const first = invoke(f, 'stop', { sprite_id: id, session_id: '715' }, 'stop-first');
    assert.equal(first.status, 0, first.stdout);
    const second = invoke(f, 'stop', { sprite_id: id, session_id: '800' }, 'stop-second');
    assert.equal(second.status, 0, second.stdout);
    assert.deepEqual(JSON.parse(fs.readFileSync(f.db, 'utf8')).sessions, []);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});


test('workspace transfer excludes secrets and symlinks, prevents unsafe pulls, and preserves Git index', () => {
  const f = fixture();
  try {
    fs.writeFileSync(path.join(f.gitRoot, '.env'), 'TOKEN=not-for-transfer');
    fs.mkdirSync(path.join(f.gitRoot, '.ssh')); fs.writeFileSync(path.join(f.gitRoot, '.ssh', 'key'), 'private');
    const outside = path.join(f.root, 'outside'); fs.mkdirSync(outside); fs.writeFileSync(path.join(outside, 'secret'), 'outside');
    const linked = path.join(f.gitRoot, 'linked');
    fs.mkdirSync(linked); fs.writeFileSync(path.join(linked, 'secret'), 'tracked before replacement');
    assert.equal(spawnSync('git', ['-C', f.gitRoot, 'add', 'linked/secret'], { encoding: 'utf8' }).status, 0);
    fs.rmSync(linked, { recursive: true });
    fs.symlinkSync(outside, path.join(f.gitRoot, 'linked'), 'junction');
    assert.equal(spawnSync('git', ['-C', f.gitRoot, 'add', 'README.txt'], { encoding: 'utf8' }).status, 0);
    const index = path.join(f.gitRoot, '.git', 'index');
    const indexHash = () => createHash('sha256').update(fs.readFileSync(index)).digest('hex');
    const beforeIndex = indexHash();
    const base = snapshot(f.gitRoot);
    assert.equal(base.files.some(file => file.path === '.env' || file.path === '.ssh/key' || file.path === 'linked' || file.path.startsWith('linked/')), false);
    assert.ok(base.excluded.includes('.env'));
    assert.ok(base.excluded.includes('.ssh/key'));
    const incoming = { version: 1, files: [{ path: 'README.txt', mode: 0o644, data: Buffer.from('remote\n').toString('base64') }] };
    const preview = previewTransfer(f.gitRoot, base, incoming);
    assert.deepEqual(preview.conflicts, []);
    assert.equal(applyPull(f.gitRoot, base, preview), 1);
    assert.equal(fs.readFileSync(path.join(f.gitRoot, 'README.txt'), 'utf8'), 'remote\n');
    fs.writeFileSync(path.join(f.gitRoot, 'README.txt'), 'local edit\n');
    const conflict = previewTransfer(f.gitRoot, preview.incoming, { version: 1, files: [{ path: 'README.txt', mode: 0o644, data: Buffer.from('other remote\n').toString('base64') }] });
    assert.deepEqual(conflict.conflicts, ['README.txt']);
    const symlinkPull = { version: 1, files: [...base.files, { path: 'linked/secret', mode: 0o644, data: Buffer.from('overwrite').toString('base64') }] };
    const unsafe = previewTransfer(f.gitRoot, base, symlinkPull);
    assert.throws(() => applyPull(f.gitRoot, base, unsafe), { code: 'unsafe_path' });
    assert.throws(() => previewTransfer(f.gitRoot, base, { version: 1, files: [{ path: '../escape', mode: 0o644, data: '' }] }), { code: 'transfer_protocol' });
    assert.equal(indexHash(), beforeIndex);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('pull preview rejects an ignored local destination and preserves its contents', () => {
  const f = fixture();
  try {
    fs.writeFileSync(path.join(f.gitRoot, '.gitignore'), 'local-data.txt\n');
    fs.writeFileSync(path.join(f.gitRoot, 'local-data.txt'), 'keep this ignored data\n');
    const base = snapshot(f.gitRoot);
    assert.equal(base.files.some(file => file.path === 'local-data.txt'), false);
    const incoming = { version: 1, files: [...base.files, { path: 'local-data.txt', mode: 0o644, data: Buffer.from('remote overwrite\n').toString('base64') }] };
    const preview = previewTransfer(f.gitRoot, base, incoming);
    assert.deepEqual(preview.conflicts, ['local-data.txt']);
    assert.throws(() => applyPull(f.gitRoot, base, preview), { code: 'transfer_conflict' });
    assert.equal(fs.readFileSync(path.join(f.gitRoot, 'local-data.txt'), 'utf8'), 'keep this ignored data\n');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('reapplying an unchanged added file remains idempotent', () => {
  const f = fixture();
  try {
    const base = snapshot(f.gitRoot);
    const incoming = { version: 1, files: [...base.files, { path: 'remote-added.txt', mode: 0o644, data: Buffer.from('remote content\n').toString('base64') }] };
    const firstPreview = previewTransfer(f.gitRoot, base, incoming);
    assert.deepEqual(firstPreview.conflicts, []);
    assert.equal(applyPull(f.gitRoot, base, firstPreview), 1);
    const retryPreview = previewTransfer(f.gitRoot, base, incoming);
    assert.deepEqual(retryPreview.conflicts, []);
    assert.equal(applyPull(f.gitRoot, base, retryPreview), 1);
    assert.equal(fs.readFileSync(path.join(f.gitRoot, 'remote-added.txt'), 'utf8'), 'remote content\n');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('pull preview protects an ignored local file from a remote deletion', () => {
  const f = fixture();
  try {
    const localFile = path.join(f.gitRoot, 'local-data.txt');
    fs.writeFileSync(localFile, 'baseline workspace data\n');
    const base = snapshot(f.gitRoot);
    assert.ok(base.files.some(file => file.path === 'local-data.txt'));
    fs.appendFileSync(path.join(f.gitRoot, '.git', 'info', 'exclude'), '\nlocal-data.txt\n');
    fs.writeFileSync(localFile, 'preserve changed ignored data\n');
    const incoming = { version: 1, files: base.files.filter(file => file.path !== 'local-data.txt') };
    const preview = previewTransfer(f.gitRoot, base, incoming);
    assert.deepEqual(preview.conflicts, ['local-data.txt']);
    assert.throws(() => applyPull(f.gitRoot, base, preview), { code: 'transfer_conflict' });
    assert.equal(fs.readFileSync(localFile, 'utf8'), 'preserve changed ignored data\n');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('destructive approvals bind exact action, resource, revision, expiration, and are single-use', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gardn-approval-'));
  try {
    const approval = saveApproval(root, { sprite_id: 'acme/a', action: 'restore', revision: 7 }, Date.now() + 60_000, { checkpoint_id: 'v1' });
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/b', action: 'restore', revision: 7, scope: { checkpoint_id: 'v1' } }), false);
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/a', action: 'destroy', revision: 7, scope: { checkpoint_id: 'v1' } }), false);
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/a', action: 'restore', revision: 8, scope: { checkpoint_id: 'v1' } }), false);
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/a', action: 'restore', revision: 7, scope: { checkpoint_id: 'v2' } }), false);
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/a', action: 'restore', revision: 7, scope: { checkpoint_id: 'v1' } }), true);
    assert.equal(consumeApproval(root, approval.token, { sprite_id: 'acme/a', action: 'restore', revision: 7, scope: { checkpoint_id: 'v1' } }), false);
    const expired = saveApproval(root, { sprite_id: 'acme/a', action: 'forget', revision: 7 }, Date.now() - 1);
    assert.equal(consumeApproval(root, expired.token, { sprite_id: 'acme/a', action: 'forget', revision: 7 }), false);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('checkpoint, restore, and destroy act on exact managed target and preserve unrelated Sprites', () => {
  const f = fixture();
  try {
    const id = 'acme/managed', dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'managed', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { kind: 'claude', command: ['claude'] }, phase: 'ready', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'managed', state: 'running' }, { name: 'unrelated', state: 'running' }], creates: 0, sessions: [], checkpoints: [] }));
    const target = { sprite_id: id };
    const checkpoint = invoke(f, 'checkpoint', target, 'checkpoint-1');
    assert.equal(checkpoint.status, 0, checkpoint.stderr);
    assert.equal(checkpoint.frames.findLast(frame => frame.type === 'operation').operation.result.data.checkpoint_id, 'v1');
    const providerState = JSON.parse(fs.readFileSync(f.db, 'utf8'));
    providerState.checkpoints.unshift({ id: 'Current' });
    fs.writeFileSync(f.db, JSON.stringify(providerState));
    const listing = invoke(f, 'checkpoints', target, 'checkpoints-1');
    assert.deepEqual(listing.frames.findLast(frame => frame.type === 'operation').operation.result.data, ['v1']);
    const restoreTarget = { ...target, checkpoint_id: 'v1' };
    const approvalReply = invoke(f, 'restore', restoreTarget, 'restore-approval');
    const restoreToken = approvalReply.frames.findLast(frame => frame.type === 'operation').operation.result.data.token;
    const wrongCheckpoint = invoke(f, 'restore', { ...restoreTarget, checkpoint_id: 'v2', approval: restoreToken }, 'restore-wrong-checkpoint');
    assert.equal(wrongCheckpoint.frames.findLast(frame => frame.type === 'operation').operation.result.kind, 'approval_required');
    const restored = invoke(f, 'restore', { ...restoreTarget, approval: restoreToken }, 'restore-confirm');
    assert.equal(restored.status, 0, restored.stderr);
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).restored, 'v1');
    assert.equal(JSON.parse(fs.readFileSync(path.join(dir, 'record.json'), 'utf8')).checkpoint_id, 'v2');
    const destroyApproval = invoke(f, 'destroy', target, 'destroy-approval');
    const destroyToken = destroyApproval.frames.findLast(frame => frame.type === 'operation').operation.result.data.token;
    const destroyed = invoke(f, 'destroy', { ...target, approval: destroyToken }, 'destroy-confirm');
    assert.equal(destroyed.status, 0, destroyed.stderr);
    const db = JSON.parse(fs.readFileSync(f.db, 'utf8'));

    assert.deepEqual(db.sprites.map(sprite => sprite.name), ['unrelated']);
    const record = JSON.parse(fs.readFileSync(path.join(dir, 'record.json'), 'utf8'));
    assert.equal(record.phase, 'destroyed');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('confirmed partial Create is not silently recreated after later absence', () => {
  const f = fixture();
  try {
    const params = { name: 'partial', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [], failExec: true }));
    const first = invoke(f, 'create', params, 'create-partial');
    assert.equal(first.status, 1);
    let db = JSON.parse(fs.readFileSync(f.db, 'utf8'));
    assert.equal(db.creates, 1);
    assert.equal(first.frames.find(frame => frame.type === 'snapshot').snapshot.resources[0].remote_confirmed, true);
    db.sprites = []; db.failExec = false; fs.writeFileSync(f.db, JSON.stringify(db));
    const retry = invoke(f, 'create', params, 'create-partial');
    assert.equal(retry.status, 1);
    assert.equal(retry.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'resource_missing');
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('partial Create retry keeps its operation-bound organization and name after config changes', () => {
  const f = fixture();
  try {
    const params = { workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [], failExec: true }));
    const first = invoke(f, 'create', params, 'create-identity');
    assert.equal(first.status, 1);
    fs.writeFileSync(f.db, JSON.stringify({ ...JSON.parse(fs.readFileSync(f.db, 'utf8')), failExec: false }));
    const retry = invoke(f, 'create', params, 'create-identity', { org: 'changed-org', name_prefix: 'changed-' });
    assert.equal(retry.status, 0, retry.stderr);
    assert.equal(retry.frames.findLast(frame => frame.type === 'operation').operation.result.data.id, `acme/gardn-${hash('create-identity').slice(0, 18)}`);
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});

test('approved Destroy wins admission over a previously planned Start', async () => {
  const f = fixture();
  let bridge;
  const gate = path.join(f.root, 'destroy-gate');
  try {
    const params = { name: 'guarded', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const created = invoke(f, 'create', params, 'create-guarded').frames.findLast(frame => frame.type === 'operation').operation;
    assert.equal(created.status, 'succeeded');
    const target = { sprite_id: created.result.data.id };
    const plan = invoke(f, 'start', target, 'start-before-destroy').frames.findLast(frame => frame.type === 'operation').operation.result.data;
    const approval = invoke(f, 'destroy', target, 'destroy-approval-guard').frames.findLast(frame => frame.type === 'operation').operation.result.data.token;
    fs.writeFileSync(f.db, JSON.stringify({ ...JSON.parse(fs.readFileSync(f.db, 'utf8')), destroyGate: gate }));
    const destroying = launch(f, 'destroy', { ...target, approval }, 'destroy-guarded');
    await until(() => fs.existsSync(`${gate}.ready`));
    bridge = spawn(process.execPath, plan.args, { stdio: 'ignore', env: { ...process.env, SPRITES_FAKE_DB: f.db } });
    const bridgeStatus = await new Promise(resolve => bridge.once('close', resolve));
    assert.notEqual(bridgeStatus, 0);
    fs.writeFileSync(`${gate}.release`, 'release');
    const destroyed = await destroying.done;
    assert.equal(destroyed.status, 0, destroyed.stderr);
    const record = JSON.parse(fs.readFileSync(path.join(f.state, 'resources', hash(target.sprite_id), 'record.json'), 'utf8'));
    assert.equal(record.phase, 'destroyed');
    assert.deepEqual(JSON.parse(fs.readFileSync(f.db, 'utf8')).sessions, []);
  } finally {
    fs.writeFileSync(`${gate}.release`, 'release');
    if (bridge && bridge.exitCode === null) bridge.kill('SIGKILL');
    fs.rmSync(f.root, { recursive: true, force: true });
  }
});

test('Create reserves resource capacity without holding catalog admission through upload', async () => {
  const f = fixture();
  const gate = path.join(f.root, 'upload-gate');
  try {
    const params = { name: 'reserved', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [], uploadGate: gate }));
    const creating = launch(f, 'create', params, 'create-reserved', { max_sprites: 1 });
    await until(() => fs.existsSync(`${gate}.ready`));
    const refreshed = invoke(f, 'list', { refresh: true }, 'refresh-during-upload');
    assert.equal(refreshed.status, 0, refreshed.stderr);
    assert.equal(refreshed.frames.findLast(frame => frame.type === 'operation').operation.status, 'succeeded');
    const second = invoke(f, 'create', { ...params, name: 'over-limit' }, 'create-over-limit', { max_sprites: 1 });
    assert.equal(second.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'resource_limit');
    fs.writeFileSync(`${gate}.release`, 'release');
    const complete = await creating.done;
    assert.equal(complete.status, 0, complete.stderr);
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally {
    fs.writeFileSync(`${gate}.release`, 'release');
    fs.rmSync(f.root, { recursive: true, force: true });
  }
});

test('installation-wide operation admission refuses excess concurrent work', async () => {
  const f = fixture();
  try {
    const params = { name: 'one', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [], delayMs: 500 }));
    const first = launch(f, 'create', params, 'parallel-one', { max_concurrent_operations: 1 });
    await until(() => fs.existsSync(path.join(f.state, 'locks', 'slots', '0.lock')));
    const second = launch(f, 'create', { ...params, name: 'two' }, 'parallel-two', { max_concurrent_operations: 1 });
    const [a, b] = await Promise.all([first.done, second.done]);
    assert.equal(a.status, 0, a.stderr);
    assert.equal(b.status, 1);
    assert.equal(b.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'operation_capacity');
    assert.equal(JSON.parse(fs.readFileSync(f.db, 'utf8')).creates, 1);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});
test('detaching the bridge kills only local transport and leaves the remote session', async () => {
  const f = fixture();
  let bridge, retryBridge;
  try {
    const id = 'acme/attached', dir = path.join(f.state, 'resources', hash(id)); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'record.json'), JSON.stringify({ id, org: 'acme', name: 'attached', managed: true, workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { kind: 'claude', command: ['claude'] }, phase: 'ready', provider_state: 'running', sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: 1, observed_unix_ms: 1, last_error: null, unpulled_changes: null }));
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [{ name: 'attached', state: 'running' }], creates: 0, sessions: [], checkpoints: [] }));
    const planned = invoke(f, 'start', { sprite_id: id }, 'start-detach');
    const connection = planned.frames.findLast(frame => frame.type === 'operation').operation.result.data;
    bridge = spawn(process.execPath, connection.args, { stdio: 'ignore', env: { ...process.env, SPRITES_FAKE_DB: f.db } });
    const receiptFile = path.join(f.state, 'resources', hash(id), 'connections', `${hash('start-detach')}.json`);
    await until(() => fs.existsSync(receiptFile) && JSON.parse(fs.readFileSync(receiptFile, 'utf8')).status === 'started');
    assert.equal(JSON.parse(fs.readFileSync(receiptFile, 'utf8')).attempt_id, connection.attempt_id);
    const exited = new Promise(resolve => bridge.once('close', resolve));
    assert.equal(JSON.parse(fs.readFileSync(receiptFile, 'utf8')).session_id, '715');
    bridge.kill('SIGTERM');
    await exited;
    const db = JSON.parse(fs.readFileSync(f.db, 'utf8'));
    retryBridge = spawn(process.execPath, connection.args, { stdio: 'ignore', env: { ...process.env, SPRITES_FAKE_DB: f.db } });
    await new Promise(resolve => retryBridge.once('close', resolve));
    const retried = JSON.parse(fs.readFileSync(f.db, 'utf8'));
    assert.equal(retried.sessions.length, 1);
    assert.equal(db.sessions.length, 1);
    assert.equal(db.sessions[0].tty, true);
  } finally {
    if (bridge && bridge.exitCode === null) bridge.kill('SIGKILL');
    if (retryBridge && retryBridge.exitCode === null) retryBridge.kill('SIGKILL');
    fs.rmSync(f.root, { recursive: true, force: true });
  }
});

test('dead lock owners are reclaimed while live resource and connection locks remain exclusive', async () => {
  const f = fixture();
  try {
    const file = path.join(f.state, 'locks', `${hash('resource')}.lock`);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, '2147483647\n');
    assert.equal(withLock(f.state, 'resource', () => 'recovered'), 'recovered');
    fs.writeFileSync(file, `${process.pid}\n`);
    assert.throws(() => withLock(f.state, 'resource', () => 'unreachable'), { code: 'resource_busy' });
    const id = 'acme/locked', connection = connectionLockFile(f.state, id);
    fs.mkdirSync(path.dirname(connection), { recursive: true });
    fs.writeFileSync(connection, '2147483647\n');
    assert.equal(hasActiveConnection(f.state, id), false);
    await withConnectionLock(f.state, id, async () => {
      assert.equal(hasActiveConnection(f.state, id), true);
    });
    assert.equal(hasActiveConnection(f.state, id), false);
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});
test('provider diagnostics cannot leak arbitrary secrets into operation state or stdout', () => {
  const f = fixture();
  try {
    const secret = 'credential-value-that-must-not-persist';
    fs.writeFileSync(f.db, JSON.stringify({ sprites: [], creates: 0, sessions: [], checkpoints: [], createFailure: secret }));
    const params = { name: 'secret-test', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const run = invoke(f, 'create', params, 'secret-error');
    assert.equal(run.status, 1);
    assert.equal(run.stdout.includes(secret), false);
    const operation = fs.readFileSync(path.join(f.state, 'operations', `${hash('secret-error')}.json`), 'utf8');
    assert.equal(operation.includes(secret), false);
    assert.equal(run.frames.findLast(frame => frame.type === 'operation').operation.error.code, 'provider_error');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});
test('Destroy and Forget retire Create reservations so a new managed resource can use released capacity', () => {
  const f = fixture();
  try {
    const params = { name: 'capacity-one', workspace_id: 'workspace-1', source: { execution_host_id: 'local', path: f.gitRoot }, agent: { profile_id: 'claude', kind: 'claude', command: ['claude'], share_credentials: false } };
    const created = invoke(f, 'create', params, 'capacity-create', { max_sprites: 1 });
    assert.equal(created.status, 0, created.stderr);
    const target = { sprite_id: created.frames.findLast(frame => frame.type === 'operation').operation.result.data.id };
    const destroyApproval = invoke(f, 'destroy', target, 'capacity-destroy-approval').frames.findLast(frame => frame.type === 'operation').operation.result.data.token;
    assert.equal(invoke(f, 'destroy', { ...target, approval: destroyApproval }, 'capacity-destroy', { max_sprites: 1 }).status, 0);
    const forgetApproval = invoke(f, 'forget', target, 'capacity-forget-approval').frames.findLast(frame => frame.type === 'operation').operation.result.data.token;
    assert.equal(invoke(f, 'forget', { ...target, approval: forgetApproval }, 'capacity-forget', { max_sprites: 1 }).status, 0);
    const next = invoke(f, 'create', { ...params, name: 'capacity-two' }, 'capacity-create-two', { max_sprites: 1 });
    assert.equal(next.status, 0, next.stderr);
    assert.equal(next.frames.findLast(frame => frame.type === 'operation').operation.result.data.id, 'acme/capacity-two');
  } finally { fs.rmSync(f.root, { recursive: true, force: true }); }
});
