#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { SpriteProvider } from './provider.mjs';
import { atomic, readJson, readRecords, saveApproval, saveOperation, saveRecord, readOperation, withLock, withOperationSlot, consumeApproval, resourceDir, hashId, hasActiveConnection } from './store.mjs';
import { localCredentials, INSTALL_AUTH } from './auth.mjs';
import { snapshot, preview as previewTransfer, applyPull, INSTALL_WORKSPACE } from './transfer.mjs';

const now = () => Date.now();
const fail = (code, message, retryable = false) => Object.assign(new Error(message), { code, retryable });
const emit = frame => process.stdout.write(`${JSON.stringify(frame)}\n`);
const opStage = (operation, stage) => { operation.stage = stage; operation.updated_unix_ms = now(); saveOperation(operation._root, cleanOperation(operation)); emit({ type: 'operation', operation: cleanOperation(operation) }); };
const cleanOperation = operation => { const { _root, ...value } = operation; return value; };
const makeError = error => ({ code: error.code || 'operation_failed', message: String(error.message || 'Sprite operation failed').replace(/(?:bearer\s+)[^\s]+/ig, 'Bearer [redacted]').slice(0, 1600), retryable: error.retryable === true });
const shellQuote = value => `'${String(value).replaceAll("'", "'\\''")}'`;
const resourceId = (org, name) => `${org}/${name}`;
function validateInput(input) {
  if (!input || !input.config || !input.operation || typeof input.state_dir !== 'string' || !path.isAbsolute(input.state_dir)) throw fail('invalid_request', 'Backend input requires config, an absolute state_dir, and operation.');
  const c = input.config;
  if (c.enabled !== true) throw fail('disabled', 'Sprites integration is disabled.');
  if (!Number.isSafeInteger(c.max_sprites) || c.max_sprites < 1 || !Number.isSafeInteger(c.max_concurrent_operations) || c.max_concurrent_operations < 1 || c.max_concurrent_operations > 64 || !Number.isSafeInteger(c.max_transfer_mib) || c.max_transfer_mib < 1 || c.max_transfer_mib > 512) throw fail('invalid_config', 'Sprite limits are outside their supported ranges.');
  if (typeof c.sprite_bin !== 'string' || !c.sprite_bin || typeof c.node_bin !== 'string' || !c.node_bin) throw fail('invalid_config', 'Sprite and Node executable paths are required.');
  const ownerPid = input.owner_pid ?? process.ppid;
  if (!Number.isInteger(ownerPid) || ownerPid < 1) throw fail('owner_unavailable', 'A valid owning Gardn coordinator PID is required.');
  try { process.kill(ownerPid, 0); } catch (error) { if (error.code === 'ESRCH') throw fail('owner_unavailable', 'The owning Gardn coordinator is no longer alive.'); }
  const fence = readJson(path.join(input.state_dir, 'backend-config.json'));
  if (fence.enabled !== true) throw fail('disabled', 'Sprites configuration fence is disabled.');
  input.owner_pid = ownerPid;
  return input;
}
function requestTarget(records, target) {
  if (!target || typeof target.sprite_id !== 'string' || !target.sprite_id) throw fail('invalid_target', 'An exact Sprite resource ID is required.');
  const record = records.find(item => item.id === target.sprite_id);
  if (!record) throw fail('unknown_resource', 'Sprite is not in the local catalog. Refresh inventory or explicitly adopt it first.');
  return record;
}
const result = (kind, data) => ({ kind, data });
function recordBase({ org, name, managed }) { return { id: resourceId(org, name), org, name, managed, workspace_id: null, source: null, agent: null, phase: 'unknown', provider_state: null, sessions: [], checkpoint_id: null, revision: 1, updated_unix_ms: now(), observed_unix_ms: null, last_error: null, unpulled_changes: null }; }
function saveObserved(root, record, sessions, providerState = record.provider_state) {
  record.sessions = sessions; record.provider_state = providerState; record.observed_unix_ms = now(); record.updated_unix_ms = now(); record.revision++;
  if (record.phase !== 'creating' && record.phase !== 'preparing' && record.phase !== 'partial') record.phase = sessions.some(s => s.tty) ? 'running' : providerState || 'ready';
  saveRecord(root, record); return record;
}
function sessionOwned(record, session) {
  return record.managed && typeof session.created === 'string' && record.sessions.some(saved => saved.id === session.id && saved.created === session.created && saved.owned);
}
function sourceRoot(source) {
  if (!source || source.execution_host_id !== 'local') throw fail('unsupported_host', `Workspace source host ${source?.execution_host_id ?? '(missing)'} is not available to the local Sprite backend; no local fallback was attempted.`);
  if (!path.isAbsolute(source.path)) throw fail('invalid_source', 'Workspace source path must be absolute.');
  // Native realpath expands Windows short names before comparison with Git's root.
  return fs.realpathSync.native(source.path);
}
async function perform(input) {
  const { config, state_dir: root } = input;
  const operation = input.operation; operation._root = root;
  const provider = new SpriteProvider(config, { ownerPid: input.owner_pid, fencePath: path.join(root, 'backend-config.json') });
  const command = operation.request;
  const action = command?.action;
  const params = command?.params ?? {};
  const records = readRecords(root);
  const limit = config.max_transfer_mib * 1024 * 1024;
  if (action === 'operation') return result('completed', { message: 'Operation lookup is served by Gardn local operation storage.' });
  if (action === 'cancel') throw fail('cancel_unsupported', 'An already running provider operation cannot be canceled safely; its durable state remains available for recovery.');
  if (action === 'list') {
    if (params.refresh !== true) return result('inventory', readRecords(root));
    return withLock(root, 'catalog', () => {
      const inventory = provider.inventory();
      const existing = readRecords(root);
      for (const item of inventory) {
        const id = resourceId(config.org, item.name);
        try {
          withLock(root, id, () => {
            const record = readRecords(root).find(r => r.id === id)
              ?? { ...recordBase({ org: config.org, name: item.name, managed: false }), provider_state: item.state, phase: item.state || 'available' };
            if (record.phase === 'destroyed') {
              record.managed = false; record.workspace_id = null; record.source = null; record.agent = null;
              record.phase = item.state || 'available';
            } else if (!record.managed) record.phase = item.state || 'available';
            record.provider_state = item.state; record.observed_unix_ms = now(); record.updated_unix_ms = now();
            saveRecord(root, record);
          });
        } catch (error) { if (error.code !== 'resource_busy') throw error; }
      }
      for (const saved of existing) if (saved.org === config.org && !inventory.some(i => i.name === saved.name)) {
        try {
          withLock(root, saved.id, () => {
            const latest = readRecords(root).find(r => r.id === saved.id);
            if (!latest || latest.phase === 'destroyed') return;
            latest.provider_state = 'missing'; latest.phase = 'missing'; latest.observed_unix_ms = now(); latest.updated_unix_ms = now();
            saveRecord(root, latest);
          });
        } catch (error) { if (error.code !== 'resource_busy') throw error; }
      }
      return result('inventory', readRecords(root));
    });
  }
  if (action === 'preflight' || action === 'create') {
    const paramsCreate = params;
    const localRoot = sourceRoot(paramsCreate.source);
    const git = spawnSync('git', ['-C', localRoot, '-c', 'core.hooksPath=/dev/null', 'rev-parse', '--show-toplevel'], { encoding: 'utf8', timeout: 10_000, env: { ...process.env, LC_ALL: 'C' } });
    if (git.error || git.status !== 0) throw fail('source_not_git', `${JSON.stringify(localRoot)} is not a Git repository. No files were transferred.`);
    const gitRoot = fs.realpathSync.native(git.stdout.trim());
    const rel = path.relative(gitRoot, localRoot);
    if (rel === '..' || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) throw fail('invalid_source', 'Selected source is outside its Git worktree.');
    const baseline = snapshot(gitRoot, limit);
    const managedSource = { ...paramsCreate.source, path: gitRoot };
    if (action === 'preflight') {
      provider.inventory();
      return result('transfer', { files: baseline.files.length, bytes: baseline.bytes, excluded: baseline.excluded, changed_paths: [], conflicts: [] });
    }
    const prefix = config.name_prefix || 'gardn-';
    const name = paramsCreate.name || `${prefix}${createHash('sha256').update(operation.id).digest('hex').slice(0, 18)}`;
    if (!/^[a-z][a-z0-9-]{0,62}$/.test(name)) throw fail('invalid_name', 'Sprite name must be a lowercase provider-safe name.');
    if (!paramsCreate.workspace_id || !paramsCreate.agent || !Array.isArray(paramsCreate.agent.command) || !paramsCreate.agent.command.length || paramsCreate.agent.command.some(v => typeof v !== 'string' || !v || v.includes('\0'))) throw fail('invalid_create', 'Create requires a workspace ID and non-empty agent command argv.');
    return withLock(root, 'catalog', () => {
      const inventory = provider.inventory();
      const id = resourceId(config.org, name);
      return withLock(root, id, () => {
        const allRecords = readRecords(root);
        const managed = allRecords.filter(r => r.managed && r.phase !== 'destroyed');
        const dir = resourceDir(root, id);
      const intentFile = path.join(dir, 'intent.json');
      const requestedIntent = { operation_id: operation.id, workspace_id: paramsCreate.workspace_id, source: managedSource, agent: paramsCreate.agent };
      let record = allRecords.find(r => r.id === id);
      if (record && !record.managed) throw fail('name_conflict', 'The requested Sprite name is already catalogued as a foreign resource.');
      if (!record && inventory.some(r => r.name === name)) throw fail('name_conflict', 'A Sprite with the requested name exists but is not managed by this installation.');
      if (!record && managed.length >= config.max_sprites) throw fail('resource_limit', `Installation-wide maximum of ${config.max_sprites} managed Sprites has been reached.`);
      fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
      if (fs.existsSync(intentFile)) {
        const savedIntent = readJson(intentFile);
        if (savedIntent.operation_id !== operation.id || savedIntent.workspace_id !== requestedIntent.workspace_id || JSON.stringify(savedIntent.source) !== JSON.stringify(requestedIntent.source) || JSON.stringify(savedIntent.agent) !== JSON.stringify(requestedIntent.agent)) throw fail('name_conflict', 'This Sprite name is bound to a different creation request. Choose a new name.');
      } else {
        if (record) throw fail('recovery_unavailable', 'Managed Sprite is missing its durable creation intent; refusing to replay or overwrite it.');
        atomic(intentFile, requestedIntent);
      }
      if (!record) {
        record = { ...recordBase({ org: config.org, name, managed: true }), workspace_id: paramsCreate.workspace_id, source: managedSource, agent: paramsCreate.agent, phase: 'creating' };
        saveRecord(root, record); // intended resource and request are durable before provider create
      }
      if (record.phase === 'destroyed') throw fail('resource_destroyed', 'This resource was explicitly destroyed; choose a new Sprite name rather than recreating it implicitly.');
      if (record.phase === 'missing') throw fail('resource_missing', 'This Sprite was authoritatively absent. It will not be recreated by retry; choose a new name for a new resource.');
      if (record.phase === 'unconfirmed') throw fail('resource_unconfirmed', 'Sprite presence is unknown; refresh inventory before retrying.', true);
      if (['ready', 'running', 'stopped'].includes(record.phase)) {
        if (inventory.some(item => item.name === name)) return result('resource', record);
        record.phase = 'missing'; record.provider_state = 'missing'; record.observed_unix_ms = now(); record.updated_unix_ms = now(); record.revision++; saveRecord(root, record);
        throw fail('resource_missing', 'This Sprite was authoritatively absent. It will not be recreated by retry; choose a new name for a new resource.');
      }
      if (!['creating', 'preparing', 'partial'].includes(record.phase)) throw fail('recovery_unsafe', `Cannot safely resume creation from phase ${record.phase}.`);
      const baselineFile = path.join(dir, 'baseline.json');
      const originalBaseline = fs.existsSync(baselineFile) ? readJson(baselineFile) : baseline;
      if (!fs.existsSync(baselineFile)) atomic(baselineFile, originalBaseline);
      const remotelyPresent = inventory.some(item => item.name === name);
      if (remotelyPresent && !record.remote_confirmed) {
        record.remote_confirmed = true; record.provider_state = 'available'; record.observed_unix_ms = now(); record.updated_unix_ms = now(); saveRecord(root, record);
      } else if (!remotelyPresent && record.remote_confirmed) {
        record.phase = 'missing'; record.provider_state = 'missing'; record.observed_unix_ms = now(); record.updated_unix_ms = now(); record.revision++; saveRecord(root, record);
        throw fail('resource_missing', 'Previously confirmed Sprite is now absent. It will not be recreated by retry; choose a new name for a new resource.');
      }
      if (!remotelyPresent) {
        opStage(operation, 'creating');
        try {
          provider.create(name);
          record.remote_confirmed = true; record.provider_state = 'available'; record.observed_unix_ms = now(); record.updated_unix_ms = now(); saveRecord(root, record);
        } catch (error) {
          let actual;
          try { actual = provider.inventory(); } catch { throw fail('create_unconfirmed', 'Sprite creation outcome is unknown because authenticated inventory could not be refreshed. The intended record was preserved.', true); }
          if (!actual.some(item => item.name === name)) { record.phase = 'partial'; record.last_error = makeError(error).message; record.updated_unix_ms = now(); saveRecord(root, record); throw error; }
          record.remote_confirmed = true; record.provider_state = 'available'; record.observed_unix_ms = now(); record.updated_unix_ms = now(); saveRecord(root, record);
        }
      }
      if (paramsCreate.agent.share_credentials) {
        if (!['claude', 'codex'].includes(paramsCreate.agent.kind)) throw fail('credential_handoff_unsupported', `Credential handoff is not supported for agent kind ${paramsCreate.agent.kind}; sign in manually.`);
        const credentials = localCredentials(paramsCreate.agent.kind);
        if (credentials) {
          try {
            provider.exec(name, ['node', '-e', INSTALL_AUTH, `/home/sprite/gardn/${hashId(record.id)}`], { input: JSON.stringify({ ...credentials, agent: paramsCreate.agent.kind }), maxBuffer: 64 * 1024, timeout: 30_000 });
          } catch { throw fail('credential_handoff_failed', 'Opt-in credential handoff failed; credential data was not included in operation state or logs. Sign in manually in the Sprite.'); }
        }
      }
      record.phase = 'preparing'; record.last_error = null; record.updated_unix_ms = now(); saveRecord(root, record);
      opStage(operation, 'uploading');
      const remoteRoot = `/home/sprite/gardn/${hashId(record.id)}/workspace`;
      try {
        provider.exec(name, ['node', '-e', INSTALL_WORKSPACE, remoteRoot], { input: JSON.stringify(originalBaseline), maxBuffer: 1024 * 1024, timeout: 120_000 });
      } catch (error) {
        record.phase = 'partial'; record.last_error = makeError(error).message; record.updated_unix_ms = now(); saveRecord(root, record); throw error;
      }
      const toolPath = `/home/sprite/gardn/${hashId(record.id)}/tools/node_modules/.bin`;
      const availabilityScript = `export PATH=${shellQuote(toolPath)}:$PATH; if command -v ${shellQuote(record.agent.command[0])} >/dev/null 2>&1; then printf available; else printf missing; fi`;
      let available;
      try { available = provider.exec(name, ['sh', '-lc', availabilityScript], { maxBuffer: 64 * 1024, timeout: 30_000 }); }
      catch (error) {
        record.phase = 'partial'; record.last_error = makeError(error).message; record.updated_unix_ms = now(); saveRecord(root, record);
        throw fail('agent_preflight_failed', 'Could not verify the selected agent executable in the prepared Sprite; retry after checking provider and image availability.', true);
      }
      if (available.trim() !== 'available') {
        record.phase = 'partial'; record.last_error = `Agent command ${JSON.stringify(record.agent.command[0])} is unavailable in the Sprite.`;
        record.updated_unix_ms = now(); saveRecord(root, record);
        throw fail('agent_unavailable', `${record.last_error} Install it inside the Sprite, then retry Create; the workspace and remote Git baseline are preserved.`, true);
      }
      record.phase = 'ready'; record.last_error = null; record.updated_unix_ms = now(); record.revision++; saveRecord(root, record);
      return result('resource', record);
    });
    });
  }
  const target = params.target ?? params;
  const record = requestTarget(records, target);
  return withLock(root, record.id, () => {
    const currentRecord = requestTarget(readRecords(root), target);
    provider.org = currentRecord.org; // Existing records retain their original organization across config changes.
    const record = currentRecord;
    if (action === 'inspect') {
      const sessions = provider.sessions(record.name).map(s => ({ ...s, owned: sessionOwned(record, s) }));
      return result('resource', saveObserved(root, record, sessions));
    }
    if (action === 'connect' || action === 'start' || action === 'shell' || action === 'resume') {
      if (!record.managed && action !== 'connect' && action !== 'shell') throw fail('foreign_resource', 'Starting or resuming a foreign Sprite is not permitted.');
      const mode = action;
      let conversationRef = null;
      if (mode === 'resume') {
        conversationRef = params.conversation_ref;
        if (typeof conversationRef !== 'string' || !conversationRef || /[\0\r\n]/.test(conversationRef)) throw fail('invalid_conversation_ref', 'Resume requires an exact non-empty conversation reference.');
        if (!['codex', 'claude'].includes(record.agent?.kind)) throw fail('resume_unsupported', `Resume is not supported for agent kind ${record.agent?.kind ?? '(unknown)'}.`);
      }
      const sessions = provider.sessions(record.name);
      let session;
      if (mode === 'connect') {
        if (!target.session_id) throw fail('session_required', 'Connect requires the exact session_id; use Start to create a session.');
        session = sessions.find(s => s.id === target.session_id);
        if (!session) throw fail('session_missing', 'The exact requested session is not currently present.');
      }
      if ((mode === 'start' || mode === 'resume') && (!Array.isArray(record.agent?.command) || !record.agent.command.length)) throw fail('agent_unavailable', 'No recorded agent command is available for this Sprite.');
      const activeConnection = hasActiveConnection(root, record.id);
      if (activeConnection && mode !== 'connect') throw fail('connection_exists', 'This Sprite already has an active Gardn terminal connection.');
      const attemptId = randomUUID();
      const args = [path.join(path.dirname(fileURLToPath(import.meta.url)), 'connect.mjs'), root, record.id, mode, session?.id ?? '', conversationRef ?? '', operation.id, String(input.owner_pid), attemptId];
      return result('connection', { sprite_id: record.id, session_id: session?.id ?? null, program: config.node_bin, args, agent_kind: mode === 'shell' ? null : record.agent?.kind ?? null, starts_session: mode !== 'connect', remote_cwd: record.managed ? `/home/sprite/gardn/${hashId(record.id)}/workspace` : '/home/sprite', attempt_id: attemptId });
    }
    if (action === 'stop') {
      if (!target.session_id) throw fail('session_required', 'Stop requires the exact session_id; stopping all sessions is not supported.');
      if (!record.managed) throw fail('foreign_resource', 'Stopping a foreign Sprite is not permitted.');
      const session = provider.sessions(record.name).map(s => ({ ...s, owned: sessionOwned(record, s) })).find(s => s.id === target.session_id);
      if (!session) throw fail('session_missing', 'The exact session is no longer present.');
      if (!session.owned) throw fail('session_not_owned', 'This session is not known to be Gardn-owned; refusing to stop it.');
      provider.kill(record.name, session.id);
      record.sessions = provider.sessions(record.name).map(s => ({ ...s, owned: sessionOwned(record, s) }));
      record.phase = record.sessions.some(s => s.tty) ? 'running' : 'stopped'; record.revision++; record.observed_unix_ms = now(); record.updated_unix_ms = now(); saveRecord(root, record);
      return result('resource', record);
    }
    if (action === 'pull_preview' || action === 'pull') {
      if (!record.managed || !record.workspace_id || !record.source) throw fail('foreign_resource', 'Pull is only available for an associated Gardn-managed Sprite.');
      const sessions = provider.sessions(record.name);
      if (sessions.some(s => s.tty)) throw fail('sessions_running', 'Stop the Sprite session before pulling workspace changes.');
      const rootPath = sourceRoot(record.source);
      const baselineFile = path.join(resourceDir(root, record.id), 'baseline.json');
      const baseline = readJson(baselineFile);
      const remoteRoot = `/home/sprite/gardn/${hashId(record.id)}/workspace`;
      const script = `const fs=require('fs'),p=require('path'),r=${JSON.stringify(remoteRoot)},out=[],excluded=[];const safe=n=>!!n&&!n.startsWith('/')&&!n.includes('\\\\')&&n.split('/').every(x=>x&&x!=='.'&&x!=='..')&&!n.split('/').some(x=>/^(\\.git|\\.sprite|\\.sprites|\\.ssh|\\.aws|\\.azure|\\.config|\\.codex|\\.claude|\\.vercel|\\.npmrc|\\.pypirc|\\.netrc|credentials(?:\\.json)?|auth\\.json|id_rsa|id_ed25519)$/i.test(x)||/^\\.env(?:\\.|$)/i.test(x)||/\\.(pem|key|p12|pfx)$/i.test(x));function walk(d){for(const e of fs.readdirSync(d,{withFileTypes:true})){const f=p.join(d,e.name),n=p.relative(r,f).split(p.sep).join('/');if(!safe(n)||e.isSymbolicLink()){excluded.push(n);continue}if(e.isDirectory())walk(f);else if(e.isFile()){const b=fs.readFileSync(f);out.push({path:n,mode:(fs.statSync(f).mode&0o111)?493:420,data:b.toString('base64')})}}}walk(r);process.stdout.write(JSON.stringify({version:1,files:out,excluded}));`;
      const incoming = JSON.parse(provider.exec(record.name, ['node', '-e', script], { maxBuffer: limit * 3, timeout: 120_000 }));
      const preview = previewTransfer(rootPath, baseline, incoming, limit);
      if (action === 'pull_preview') {
        record.unpulled_changes = preview.changed_paths.length > 0;
        record.updated_unix_ms = now(); saveRecord(root, record);
        return result('transfer', { files: preview.files, bytes: preview.bytes, excluded: preview.excluded, changed_paths: preview.changed_paths, conflicts: preview.conflicts });
      }
      if (preview.conflicts.length) throw fail('transfer_conflict', `Pull has local conflicts: ${preview.conflicts.slice(0, 20).join(', ')}`);
      const count = applyPull(rootPath, baseline, preview, limit);
      atomic(baselineFile, preview.incoming);
      record.unpulled_changes = false; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record);
      return result('completed', { message: `Pulled ${count} changed workspace paths.` });
    }
    if (action === 'checkpoint') {
      if (!record.managed) throw fail('foreign_resource', 'Checkpoint is unavailable for foreign Sprites.');
      const id = provider.checkpoint(record.name, 'Gardn checkpoint'); record.checkpoint_id = id; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record); return result('resource', record);
    }
    if (action === 'checkpoints') return result('checkpoints', provider.checkpoints(record.name));
    if (action === 'restore' || action === 'destroy') {
      if (hasActiveConnection(root, record.id)) throw fail('connection_active', `${action} is unavailable while Gardn has an active connection to this Sprite.`);
      if (!record.managed) throw fail('foreign_resource', `${action} is unavailable for foreign Sprites.`);
      if (action === 'restore' && !target.checkpoint_id) throw fail('checkpoint_required', 'Restore requires an exact checkpoint_id.');
      const sessions = provider.sessions(record.name);
      if (sessions.some(s => s.tty)) throw fail('sessions_running', `${action} requires the Sprite to have no active sessions; stop each known owned session first.`);
      const approval = target.approval;
      const approvalScope = action === 'restore' ? { checkpoint_id: target.checkpoint_id } : {};
      if (!consumeApproval(root, approval, { sprite_id: record.id, action, revision: record.revision, scope: approvalScope })) {
        const token = saveApproval(root, { sprite_id: record.id, action, revision: record.revision, summary: action === 'destroy' ? 'Permanently destroy this exact Sprite; remote data will be lost.' : 'Restore the selected checkpoint; current remote state may be replaced.' }, now() + 5 * 60_000, approvalScope);
        return result('approval_required', token);
      }
      if (action === 'restore') {
        const recoveryCheckpoint = provider.checkpoint(record.name, 'Gardn recovery before restore');
        record.checkpoint_id = recoveryCheckpoint; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record);
        provider.restore(record.name, target.checkpoint_id);
        record.phase = 'ready'; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record);
        return result('resource', record);
      }
      try { provider.destroy(record.name); } catch (error) {
        let remaining;
        try { remaining = provider.inventory(); } catch { throw fail('destroy_unconfirmed', 'Destroy outcome is unknown because authenticated absence could not be verified. The local record was preserved.', true); }
        if (remaining.some(item => item.name === record.name)) throw error;
      }
      let remaining;
      try { remaining = provider.inventory(); } catch { throw fail('destroy_unconfirmed', 'Destroy may have succeeded, but authenticated absence could not be verified. The local record was preserved.', true); }
      if (remaining.some(item => item.name === record.name)) throw fail('destroy_unconfirmed', 'Provider still reports this Sprite after destroy; local record was preserved.', true);
      record.phase = 'destroyed'; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record); return result('completed', { message: `Destroyed ${record.id}.` });
    }
    if (action === 'forget') {
      if (hasActiveConnection(root, record.id)) throw fail('connection_active', 'Forget is unavailable while Gardn has an active connection to this Sprite.');
      if (!consumeApproval(root, target.approval, { sprite_id: record.id, action, revision: record.revision })) {
        const token = saveApproval(root, { sprite_id: record.id, action, revision: record.revision, summary: 'Remove only this local Gardn record. The remote Sprite will remain untouched.' }, now() + 5 * 60_000);
        return result('approval_required', token);
      }
      fs.rmSync(resourceDir(root, record.id), { recursive: true, force: true });
      return result('completed', { message: `Forgot local record for ${record.id}; remote resource untouched.` });
    }
    if (action === 'reassociate') {
      if (!record.managed) throw fail('foreign_resource', 'Foreign Sprites must be deliberately adopted before reassociation.');
      const selectedRoot = sourceRoot(params.source);
      const git = spawnSync('git', ['-C', selectedRoot, '-c', 'core.hooksPath=/dev/null', 'rev-parse', '--show-toplevel'], { encoding: 'utf8', timeout: 10_000, env: { ...process.env, LC_ALL: 'C' } });
      if (git.error || git.status !== 0) throw fail('source_not_git', 'Reassociation requires an accessible Git worktree.');
      if (!params.workspace_id) throw fail('invalid_source', 'A stable workspace ID is required.');
      record.workspace_id = params.workspace_id; record.source = { ...params.source, path: fs.realpathSync.native(git.stdout.trim()) }; record.revision++; record.updated_unix_ms = now(); saveRecord(root, record); return result('resource', record);
    }
    throw fail('invalid_action', `Unsupported Sprite action: ${String(action)}`);
  });
}
async function main() {
  let operation;
  try {
    const input = validateInput(JSON.parse(fs.readFileSync(0, 'utf8')));
    operation = input.operation;
    operation._root = input.state_dir;
    const prior = (() => { try { return readOperation(input.state_dir, operation.id); } catch { return null; } })();
    const live = prior ? { ...prior, ...operation, _root: input.state_dir } : { ...operation, _root: input.state_dir };
    live.status = 'running'; live.stage = 'starting'; live.updated_unix_ms = now(); saveOperation(input.state_dir, cleanOperation(live)); emit({ type: 'operation', operation: cleanOperation(live) });
    try {
      await withOperationSlot(input.state_dir, input.config.max_concurrent_operations, async () => {
        try {
          const value = await perform({ ...input, operation: live });
          live.result = value; live.status = 'succeeded'; live.stage = 'completed'; live.error = null;
        } catch (error) { live.status = 'failed'; live.stage = 'failed'; live.error = makeError(error); live.result = null; }
        live.updated_unix_ms = now(); saveOperation(input.state_dir, cleanOperation(live)); emit({ type: 'operation', operation: cleanOperation(live) });
      });
    } catch (error) {
      live.status = 'failed'; live.stage = 'failed'; live.error = makeError(error); live.result = null;
      live.updated_unix_ms = now(); saveOperation(input.state_dir, cleanOperation(live)); emit({ type: 'operation', operation: cleanOperation(live) });
    }
    const observesInventory = live.request.action === 'list';
    const snapshot = { enabled: true, resources: readRecords(input.state_dir), operations: [cleanOperation(live)], observation_error: observesInventory ? live.error : null, observed_unix_ms: observesInventory && !live.error ? now() : null };
    emit({ type: 'snapshot', snapshot });
    if (live.status === 'failed') process.exitCode = 1;
  } catch (error) {
    if (operation) {
      operation.status = 'failed'; operation.stage = 'failed'; operation.error = makeError(error); operation.result = null; operation.updated_unix_ms = now();
      try { saveOperation(operation._root, cleanOperation(operation)); emit({ type: 'operation', operation: cleanOperation(operation) }); } catch {}
    }
    emit({ type: 'snapshot', snapshot: { enabled: false, resources: [], operations: operation ? [cleanOperation(operation)] : [], observation_error: makeError(error), observed_unix_ms: now() } });
    process.exitCode = 1;
  }
}
await main();
