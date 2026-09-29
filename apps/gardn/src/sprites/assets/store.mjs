import fs from 'node:fs';
import path from 'node:path';
import { createHash, randomBytes } from 'node:crypto';

export const hashId = id => createHash('sha256').update(String(id)).digest('hex');
export function atomic(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  const tmp = `${file}.${process.pid}.${randomBytes(8).toString('hex')}.tmp`;
  let fd;
  try {
    fd = fs.openSync(tmp, 'wx', 0o600);
    fs.writeFileSync(fd, JSON.stringify(value));
    fs.fsyncSync(fd);
    fs.closeSync(fd); fd = undefined;
    fs.renameSync(tmp, file);
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
    fs.rmSync(tmp, { force: true });
  }
}
export function readJson(file) { return JSON.parse(fs.readFileSync(file, 'utf8')); }
export const operationFile = (root, id) => path.join(root, 'operations', `${hashId(id)}.json`);
export const resourceDir = (root, id) => path.join(root, 'resources', hashId(id));
export const recordFile = (root, id) => path.join(resourceDir(root, id), 'record.json');
export function readRecords(root) {
  const dir = path.join(root, 'resources');
  if (!fs.existsSync(dir)) return [];
  const records = [];
  for (const name of fs.readdirSync(dir)) {
    const file = path.join(dir, name, 'record.json');
    try { records.push(readJson(file)); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  return records;
}
export function saveRecord(root, record) { atomic(recordFile(root, record.id), record); }
export function saveOperation(root, operation) { atomic(operationFile(root, operation.id), operation); }
export function readOperation(root, id) { return readJson(operationFile(root, id)); }
export const connectionReceiptFile = (root, id, operationId) => path.join(resourceDir(root, id), 'connections', `${hashId(operationId)}.json`);
export function saveConnectionReceipt(root, id, operationId, receipt) {
  atomic(connectionReceiptFile(root, id, operationId), receipt);
}
export const connectionLockFile = (root, id) => path.join(resourceDir(root, id), 'connection.lock');
export const hasActiveConnection = (root, id) => {
  const file = connectionLockFile(root, id);
  try { fs.accessSync(file); } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  return !reclaimDeadLock(file);
};
function lockOwner(file) {
  const text = fs.readFileSync(file, 'utf8');
  try {
    const value = JSON.parse(text);
    if (typeof value === 'number') return value;
    return value.worker_pid ?? value.pid;
  } catch { return Number.parseInt(text, 10); }
}
function processAlive(pid) {
  if (!Number.isInteger(pid) || pid < 1) return false;
  try { process.kill(pid, 0); return true; } catch (error) { return error.code !== 'ESRCH'; }
}
function reclaimDeadLock(file) {
  try {
    const before = fs.statSync(file);
    if (processAlive(lockOwner(file))) return false;
    const current = fs.statSync(file);
    if (before.dev !== current.dev || before.ino !== current.ino) return false;
    fs.rmSync(file);
    return true;
  } catch (error) { return error.code === 'ENOENT'; }
}
function acquireLock(file, body, code, message) {
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  for (let attempt = 0; attempt < 2; attempt++) {
    const tmp = `${file}.${process.pid}.${randomBytes(8).toString('hex')}.tmp`;
    let fd;
    try {
      fd = fs.openSync(tmp, 'wx', 0o600);
      fs.writeFileSync(fd, body);
      fs.fsyncSync(fd);
      fs.closeSync(fd); fd = undefined;
      try { fs.linkSync(tmp, file); return; }
      catch (error) {
        if (error.code !== 'EEXIST') throw error;
        if (!reclaimDeadLock(file)) throw Object.assign(new Error(message), { code });
      }
    } finally {
      if (fd !== undefined) fs.closeSync(fd);
      fs.rmSync(tmp, { force: true });
    }
  }
  throw Object.assign(new Error(message), { code });
}
function releaseLock(file, expectedPid = process.pid) {
  try { if (lockOwner(file) === expectedPid) fs.rmSync(file); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
}
export function withLock(root, key, fn) {
  const file = path.join(root, 'locks', `${hashId(key)}.lock`);
  acquireLock(file, `${process.pid}\n`, 'resource_busy', 'A conflicting Sprite operation is already in progress.');
  try { return fn(); } finally { releaseLock(file); }
}
export async function withOperationSlot(root, limit, fn) {
  if (!Number.isInteger(limit) || limit < 1 || limit > 64) throw Object.assign(new Error('Configured operation concurrency must be between 1 and 64.'), { code: 'invalid_config' });
  const dir = path.join(root, 'locks', 'slots'), gate = path.join(dir, 'admission.lock');
  acquireLock(gate, `${process.pid}\n`, 'operation_capacity', 'Sprite operation admission is busy; retry shortly.');
  let slot;
  try {
    const active = [];
    for (let index = 0; index < 64; index++) {
      const candidate = path.join(dir, `${index}.lock`);
      try {
        const value = readJson(candidate);
        if (reclaimDeadLock(candidate)) continue;
        active.push(value);
      } catch (error) { if (error.code !== 'ENOENT') active.push({ max_concurrent_operations: 1 }); }
    }
    const effectiveLimit = Math.min(limit, ...active.map(value => Number.isInteger(value.max_concurrent_operations) ? value.max_concurrent_operations : 1));
    if (active.length >= effectiveLimit) throw Object.assign(new Error('All installation-wide Sprite operation slots are occupied.'), { code: 'operation_capacity', retryable: true });
    for (let index = 0; index < 64; index++) {
      const candidate = path.join(dir, `${index}.lock`);
      const metadata = { worker_pid: process.pid, max_concurrent_operations: limit, started_unix_ms: Date.now() };
      try {
        acquireLock(candidate, JSON.stringify(metadata), 'operation_capacity', 'Sprite operation slot is occupied.');
        slot = candidate;
        break;
      } catch (error) { if (error.code !== 'operation_capacity') throw error; }
    }
    if (!slot) throw Object.assign(new Error('No installation-wide Sprite operation slot is available.'), { code: 'operation_capacity', retryable: true });
  } finally { releaseLock(gate); }
  try { return await fn(); } finally { releaseLock(slot); }
}
export async function withConnectionLock(root, id, fn) {
  const file = connectionLockFile(root, id);
  acquireLock(file, `${process.pid}\\n`, 'connection_exists', 'This Sprite already has a Gardn connection.');
  try { return await fn(); } finally { releaseLock(file); }
}
export function saveApproval(root, approval, expires, scope = {}) {
  const token = randomBytes(32).toString('hex');
  atomic(path.join(root, 'approvals', `${hashId(token)}.json`), { ...approval, tokenHash: hashId(token), scopeHash: hashId(JSON.stringify(scope)), expires });
  return { ...approval, token };
}
export function consumeApproval(root, token, { sprite_id, action, revision, scope = {} }) {
  if (typeof token !== 'string' || !token) return false;
  const file = path.join(root, 'approvals', `${hashId(token)}.json`);
  let value;
  try { value = readJson(file); } catch { return false; }
  if (value.expires < Date.now() || value.sprite_id !== sprite_id || value.action !== action || value.revision !== revision || value.scopeHash !== hashId(JSON.stringify(scope))) return false;
  fs.rmSync(file, { force: true });
  return true;
}
