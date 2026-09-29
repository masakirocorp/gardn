import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

export const LIMIT = 64 * 1024 * 1024;
export function safePath(name) {
  return typeof name === 'string' && name.length > 0 && !name.includes('\\') && !path.posix.isAbsolute(name)
    && name.split('/').every(part => part && part !== '.' && part !== '..')
    && !name.split('/').some(part => /^(\.git|\.sprite|\.sprites|\.ssh|\.aws|\.azure|\.config|\.codex|\.claude|\.vercel|\.npmrc|\.pypirc|\.netrc|credentials(?:\.json)?|auth\.json|id_rsa|id_ed25519)$/i.test(part) || /^\.env(?:\.|$)/i.test(part) || /\.(pem|key|p12|pfx)$/i.test(part));
}
export const INSTALL_WORKSPACE = String.raw`
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(process.argv[1]),input=JSON.parse(fs.readFileSync(0,'utf8'));
const safe=n=>typeof n==='string'&&!!n&&!n.includes('\\')&&!path.posix.isAbsolute(n)&&n.split('/').every(x=>x&&x!=='.'&&x!=='..')&&!n.split('/').some(x=>/^(\.git|\.sprite|\.sprites|\.ssh|\.aws|\.azure|\.config|\.codex|\.claude|\.vercel|\.npmrc|\.pypirc|\.netrc|credentials(?:\.json)?|auth\.json|id_rsa|id_ed25519)$/i.test(x)||/^\.env(?:\.|$)/i.test(x)||/\.(pem|key|p12|pfx)$/i.test(x));
function ensureDirectory(dir){const absolute=path.resolve(dir);let current=path.parse(absolute).root;for(const part of absolute.slice(current.length).split(path.sep).filter(Boolean)){current=path.join(current,part);try{const s=fs.lstatSync(current);if(!s.isDirectory()||s.isSymbolicLink())throw Error('unsafe directory')}catch(e){if(e.code!=='ENOENT')throw e;fs.mkdirSync(current,{mode:0o700});const s=fs.lstatSync(current);if(!s.isDirectory()||s.isSymbolicLink())throw Error('unsafe directory')}}}
if(input.version!==1||!Array.isArray(input.files))throw Error('invalid snapshot');ensureDirectory(root);
for(const f of input.files){if(!safe(f.path)||![420,493].includes(f.mode)||typeof f.data!=='string'||!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(f.data))throw Error('unsafe workspace snapshot');
const target=path.resolve(root,f.path);if(!target.startsWith(root+path.sep))throw Error('workspace path escaped root');ensureDirectory(path.dirname(target));const data=Buffer.from(f.data,'base64'),mode=f.mode;
function same(){try{const s=fs.lstatSync(target);return s.isFile()&&!s.isSymbolicLink()&&(s.mode&0o777)===mode&&crypto.createHash('sha256').update(fs.readFileSync(target)).digest('hex')===crypto.createHash('sha256').update(data).digest('hex')}catch(e){if(e.code==='ENOENT')return false;throw e}}
try{const s=fs.lstatSync(target);if(!s.isFile()||s.isSymbolicLink()||!same())throw Error('remote workspace conflict: '+f.path);continue}catch(e){if(e.code!=='ENOENT')throw e}
const temp=target+'.gardn-'+crypto.randomUUID()+'.tmp';try{fs.writeFileSync(temp,data,{mode,flag:'wx'});fs.chmodSync(temp,mode);try{fs.linkSync(temp,target)}catch(e){if(e.code!=='EEXIST'||!same())throw e}}finally{fs.rmSync(temp,{force:true})}}
const cp=require('node:child_process'),env={...process.env,GIT_CONFIG_NOSYSTEM:'1',GIT_CONFIG_GLOBAL:'/dev/null'};
const git=args=>{const x=cp.spawnSync('git',['-C',root,'-c','core.hooksPath=/dev/null','-c','commit.gpgSign=false',...args],{encoding:'utf8',env,timeout:30000});if(x.error||x.status!==0)throw Error('remote Git workspace initialization failed')};
const dotGit=path.join(root,'.git');
try{const s=fs.lstatSync(dotGit);if(!s.isDirectory()||s.isSymbolicLink())throw Error('unsafe remote Git metadata')}catch(e){if(e.code!=='ENOENT')throw e;git(['init','--quiet'])}
fs.mkdirSync(path.join(dotGit,'info'),{recursive:true,mode:0o700});
fs.writeFileSync(path.join(dotGit,'info','attributes'),'* -text -filter -ident -working-tree-encoding\n',{mode:0o600});
const head=cp.spawnSync('git',['-C',root,'rev-parse','--verify','HEAD'],{encoding:'utf8',env,timeout:30000});
if(head.error)throw Error('remote Git workspace initialization failed');
if(head.status!==0){git(['add','--all','--force']);git(['-c','user.name=Gardn','-c','user.email=gardn@localhost','commit','--allow-empty','-qm','Gardn workspace baseline'])}
process.stdout.write('workspace-ready');
`;
export function git(root, args) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')));
  const result = spawnSync('git', ['-C', root, '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgSign=false', ...args], { encoding: 'utf8', maxBuffer: LIMIT * 2, timeout: 30_000, env: { ...env, LC_ALL: 'C' } });
  if (result.error || result.status !== 0) throw Object.assign(new Error(result.error?.message || String(result.stderr || 'Git command failed').slice(0, 1000)), { code: 'source_error' });
  return result.stdout;
}
export function snapshot(root, limit = LIMIT) {
  const names = [...new Set(git(root, ['ls-files', '--cached', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean))].sort();
  const files = [], excluded = [];
  let bytes = 0;
  for (const name of names) {
    if (!safePath(name)) { excluded.push(name); continue; }
    const file = path.resolve(root, name);
    const rel = path.relative(root, file);
    if (rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) { excluded.push(name); continue; }
    let stat;
    try { stat = fs.lstatSync(file); } catch { excluded.push(name); continue; }
    if (!stat.isFile() || stat.isSymbolicLink()) { excluded.push(name); continue; }
    bytes += stat.size;
    if (bytes > limit) throw Object.assign(new Error(`Eligible workspace files exceed the configured ${Math.floor(limit / 1024 / 1024)} MiB limit.`), { code: 'transfer_limit' });
    files.push({ path: name, mode: stat.mode & 0o111 ? 0o755 : 0o644, data: fs.readFileSync(file).toString('base64') });
  }
  return { version: 1, files, excluded, bytes };
}
function validate(snapshot, limit = LIMIT) {
  if (snapshot?.version !== 1 || !Array.isArray(snapshot.files)) throw Object.assign(new Error('Invalid workspace snapshot.'), { code: 'transfer_protocol' });
  const seen = new Set(); let total = 0;
  for (const file of snapshot.files) {
    if (!safePath(file.path) || seen.has(file.path) || ![0o644, 0o755].includes(file.mode) || typeof file.data !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(file.data)) throw Object.assign(new Error('Workspace snapshot contains an unsafe or malformed file.'), { code: 'transfer_protocol' });
    seen.add(file.path); total += Buffer.byteLength(file.data, 'base64');
    if (total > limit) throw Object.assign(new Error('Workspace snapshot exceeds configured transfer limit.'), { code: 'transfer_limit' });
  }
  return { ...snapshot, bytes: total };
}
export function preview(root, baseline, incoming, limit) {
  const remote = validate(incoming, limit), base = validate(baseline, limit);
  const current = new Map(snapshot(root, limit).files.map(f => [f.path, f]));
  const before = new Map(base.files.map(f => [f.path, f])), after = new Map(remote.files.map(f => [f.path, f]));
  const paths = [...new Set([...before.keys(), ...after.keys()])].sort();
  const changed = paths.filter(p => !equal(before.get(p), after.get(p)));
  const conflicts = changed.filter(p => !equal(current.get(p), before.get(p)) && !equal(current.get(p), after.get(p)));
  return { incoming: remote, files: remote.files.length, bytes: remote.bytes, excluded: remote.excluded ?? [], changed_paths: changed, conflicts };
}
function equal(a, b) { return a?.data === b?.data && a?.mode === b?.mode; }
export function applyPull(root, baseline, previewResult, limit) {
  if (previewResult.conflicts.length) throw Object.assign(new Error(`Pull has local conflicts: ${previewResult.conflicts.slice(0, 20).join(', ')}`), { code: 'transfer_conflict' });
  const base = validate(baseline, limit), incoming = validate(previewResult.incoming, limit);
  const before = new Map(base.files.map(f => [f.path, f])), after = new Map(incoming.files.map(f => [f.path, f]));
  const changed = [...new Set([...before.keys(), ...after.keys()])].filter(p => !equal(before.get(p), after.get(p)));
  for (const name of changed) {
    if (!safePath(name)) throw Object.assign(new Error('Refusing unsafe pull path.'), { code: 'unsafe_path' });
    const target = path.resolve(root, name), relative = path.relative(root, target);
    if (relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw Object.assign(new Error('Refusing pull path outside workspace.'), { code: 'unsafe_path' });
    let cursor = root;
    for (const part of name.split('/').slice(0, -1)) {
      cursor = path.join(cursor, part);
      try { const st = fs.lstatSync(cursor); if (!st.isDirectory() || st.isSymbolicLink()) throw new Error(); }
      catch (error) { if (error.code === 'ENOENT') break; throw Object.assign(new Error(`Refusing pull through unsafe parent path: ${name}`), { code: 'unsafe_path' }); }
    }
  }
  for (const name of changed) {
    const file = after.get(name), target = path.resolve(root, name);
    if (!file) { try { fs.unlinkSync(target); } catch (error) { if (error.code !== 'ENOENT') throw error; } }
    else {
      fs.mkdirSync(path.dirname(target), { recursive: true });
      let cursor = root;
      for (const part of name.split('/').slice(0, -1)) { cursor = path.join(cursor, part); const st = fs.lstatSync(cursor); if (!st.isDirectory() || st.isSymbolicLink()) throw Object.assign(new Error(`Refusing pull through unsafe parent path: ${name}`), { code: 'unsafe_path' }); }
      const temp = `${target}.${process.pid}.sprites.tmp`;
      try { fs.writeFileSync(temp, Buffer.from(file.data, 'base64'), { mode: file.mode, flag: 'wx' }); fs.renameSync(temp, target); }
      finally { fs.rmSync(temp, { force: true }); }
      fs.chmodSync(target, file.mode);
    }
  }
  return changed.length;
}
