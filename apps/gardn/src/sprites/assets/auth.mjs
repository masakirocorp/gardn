// Adapted from Fly.io Sprites native plugin source (MIT; see LICENSE).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
const MAX = 64 * 1024;
const hash = value => createHash('sha256').update(value).digest('hex');
function json(text) {
  try { if (Buffer.byteLength(text) > MAX) throw new Error(); const value = JSON.parse(text); if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(); return value; }
  catch { throw new Error('Agent login cache is invalid. Sign in locally again.'); }
}
function readCache(file) {
  try { if (fs.statSync(file).size > MAX) throw new Error('size'); return json(fs.readFileSync(file, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw new Error('Cannot read agent login cache. Check local login and file permissions.'); }
}
function keychain(service, account) {
  const result = spawnSync('/usr/bin/security', ['find-generic-password', '-s', service, '-a', account, '-w'], { encoding: 'utf8', maxBuffer: MAX, timeout: 30_000, stdio: ['ignore', 'pipe', 'pipe'] });
  if (result.status === 44) return null;
  if (result.error || result.status !== 0) throw new Error('Cannot read agent login from macOS Keychain. Allow access or unlock the Keychain, then reconnect.');
  return json(result.stdout);
}
const validToken = value => typeof value === 'string' && value.length > 0 && value.length < MAX && !/[\x00-\x20\x7f]/.test(value);
// Only call after an explicit per-create share_credentials=true choice. Never inspect project env files or credential helpers.
export function localCredentials(agent, { env = process.env, home = os.homedir(), platform = process.platform } = {}) {
  if (!['claude', 'codex'].includes(agent)) return null;
  const vars = agent === 'claude' ? ['CLAUDE_CODE_OAUTH_TOKEN', 'ANTHROPIC_API_KEY', 'ANTHROPIC_AUTH_TOKEN'] : ['OPENAI_API_KEY'];
  for (const name of vars) if (env[name]) {
    if (!validToken(env[name])) throw new Error(`Invalid ${name}; expected one token without whitespace.`);
    return { kind: 'environment', env: { [name]: env[name] } };
  }
  if (agent === 'claude') {
    const dir = path.resolve(env.CLAUDE_CONFIG_DIR || path.join(home, '.claude'));
    const suffix = env.CLAUDE_CONFIG_DIR ? `-${hash(dir).slice(0, 8)}` : '';
    const value = (platform === 'darwin' ? keychain(`Claude Code-credentials${suffix}`, env.USER || os.userInfo().username) : null) ?? readCache(path.join(dir, '.credentials.json'));
    if (!value) return null;
    const oauth = value.claudeAiOauth;
    if (!oauth || !validToken(oauth.accessToken)) throw new Error('No supported Claude login in the local cache. Run claude /login locally.');
    const selected = Object.fromEntries(['accessToken', 'refreshToken', 'expiresAt', 'scopes', 'subscriptionType', 'rateLimitTier'].filter(k => Object.hasOwn(oauth, k)).map(k => [k, oauth[k]]));
    return { kind: 'login', file: '.claude/.credentials.json', data: { claudeAiOauth: selected } };
  }
  const dir = path.resolve(env.CODEX_HOME || path.join(home, '.codex'));
  let storage = 'file';
  try { const root = fs.readFileSync(path.join(dir, 'config.toml'), 'utf8').split(/^\s*\[/m)[0]; storage = root.match(/^\s*cli_auth_credentials_store\s*=\s*["'](file|keyring|auto)["']/m)?.[1] ?? 'file'; }
  catch (error) { if (error.code !== 'ENOENT') throw new Error('Cannot read Codex credential storage configuration.'); }
  let canonical = dir; try { canonical = fs.realpathSync(dir); } catch {}
  if (storage === 'keyring' && platform !== 'darwin') throw new Error('Codex keyring handoff currently supports macOS. Use file credential storage or sign in manually.');
  const stored = storage !== 'file' && platform === 'darwin' ? keychain('Codex Auth', `cli|${hash(canonical).slice(0, 16)}`) : null;
  const value = stored ?? (storage === 'keyring' ? null : readCache(path.join(dir, 'auth.json')));
  if (!value) return null;
  if (!validToken(value.OPENAI_API_KEY) && !validToken(value.tokens?.access_token)) throw new Error('No supported Codex login in the local cache. Run codex login locally.');
  return { kind: 'login', file: '.codex/auth.json', data: Object.fromEntries(['auth_mode', 'OPENAI_API_KEY', 'tokens', 'last_refresh'].filter(k => Object.hasOwn(value, k)).map(k => [k, value[k]])) };
}
export const INSTALL_AUTH = String.raw`const fs=require('fs'),path=require('path');try{const p=JSON.parse(fs.readFileSync(0,'utf8')),base=process.argv[1];function write(file,v){const dir=path.dirname(file);let cur='/';for(const part of dir.split('/').filter(Boolean)){cur=path.join(cur,part);try{fs.mkdirSync(cur,{mode:0o700})}catch(e){if(e.code!=='EEXIST')throw e}const s=fs.lstatSync(cur);if(!s.isDirectory()||s.isSymbolicLink())throw Error()}const tmp=file+'.gardn-'+require('crypto').randomUUID();try{fs.writeFileSync(tmp,JSON.stringify(v),{mode:0o600,flag:'wx'});fs.renameSync(tmp,file)}finally{fs.rmSync(tmp,{force:true})}}if(p.file){if(!['.claude/.credentials.json','.codex/auth.json'].includes(p.file))throw Error();write('/home/sprite/'+p.file,p.data)}if(p.env)write(base+'/agent-env.json',p.env);if(p.agent==='claude'){const f='/home/sprite/.claude.json';let v={};if(fs.existsSync(f)){if(fs.lstatSync(f).isSymbolicLink())throw Error();v=JSON.parse(fs.readFileSync(f,'utf8'))}write(f,{...v,hasCompletedOnboarding:true})}}catch{process.stderr.write('Could not install agent credentials privately.\n');process.exit(1)}`;
