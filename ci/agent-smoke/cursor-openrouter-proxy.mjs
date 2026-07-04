#!/usr/bin/env node
// Minimal Cursor -> OpenRouter smoke proxy, adapted from OpenRouterLabs/spawn's
// cursor proxy approach. It is intentionally test-only: fake Cursor auth/model
// RPCs, translate one AgentService stream into OpenRouter chat-completions, and
// log only event names/statuses, never API keys.
import http2 from 'node:http2';
import { appendFileSync } from 'node:fs';

const LOG = process.env.HAKO_CURSOR_PROXY_LOG || '/tmp/hako-cursor-proxy.log';
const CERT = process.env.HAKO_CURSOR_PROXY_CERT;
const KEY = process.env.HAKO_CURSOR_PROXY_KEY;
const MODEL = process.env.HAKO_SMOKE_CURSOR_MODEL || process.env.HAKO_SMOKE_MODEL || 'openrouter/auto';
const OPENROUTER_KEY = process.env.OPENROUTER_API_KEY || '';

function log(msg) {
  appendFileSync(LOG, `${new Date().toISOString()} ${msg}\n`);
}

function ev(v){const b=[];while(v>0x7f){b.push((v&0x7f)|0x80);v>>>=7;}b.push(v&0x7f);return Buffer.from(b);}
function es(f,s){const sb=Buffer.from(String(s));return Buffer.concat([ev((f<<3)|2),ev(sb.length),sb]);}
function em(f,p){return Buffer.concat([ev((f<<3)|2),ev(p.length),p]);}
function cf(p){const f=Buffer.alloc(5+p.length);f[0]=0;f.writeUInt32BE(p.length,1);p.copy(f,5);return f;}
function ct(){const j=Buffer.from('{}');const t=Buffer.alloc(5+j.length);t[0]=2;t.writeUInt32BE(j.length,1);j.copy(t,5);return t;}
function tdf(t){return cf(em(1,em(1,es(1,t))));}
function tef(){return cf(em(1,em(14,Buffer.from([8,10,16,5]))));}
function bmd(id,n){return Buffer.concat([es(1,id),es(3,id),es(4,n),es(5,n)]);}
function bmr(){return em(1,bmd(MODEL,MODEL));}
function bdr(){return em(1,bmd(MODEL,MODEL));}
function xstr(buf,out){let o=0;while(o<buf.length){let t=0,s=0;while(o<buf.length){const b=buf[o++];t|=(b&0x7f)<<s;s+=7;if(!(b&0x80))break;}const wt=t&7;if(wt===0){while(o<buf.length&&buf[o++]&0x80);}else if(wt===2){let l=0,s=0;while(o<buf.length){const b=buf[o++];l|=(b&0x7f)<<s;s+=7;if(!(b&0x80))break;}const d=buf.slice(o,o+l);o+=l;const st=d.toString('utf8');if(/^[\x20-\x7e]+$/.test(st))out.push(st);else try{xstr(d,out);}catch{}}else break;}}

if (!CERT || !KEY) throw new Error('HAKO_CURSOR_PROXY_CERT and HAKO_CURSOR_PROXY_KEY are required');
if (!OPENROUTER_KEY) throw new Error('OPENROUTER_API_KEY is required');

const server = http2.createSecureServer({ cert: await import('node:fs').then(fs => fs.readFileSync(CERT)), key: await import('node:fs').then(fs => fs.readFileSync(KEY)), allowHTTP1: true });

server.on('request', (req, res) => {
  const chunks = [];
  req.on('data', c => chunks.push(c));
  req.on('end', () => {
    const url = req.url || '';
    if (url.includes('/agent.v1.AgentService/')) return;
    log(`unary ${req.method} ${url}`);
    if (url === '/auth/exchange_user_api_key') {
      res.writeHead(200, {'content-type':'application/json'});
      res.end(JSON.stringify({accessToken:'hako-smoke-token', refreshToken:'hako-smoke-refresh', authId:'hako-smoke'}));
      return;
    }
    if (url.includes('GetUsableModels')) {
      res.writeHead(200, {'content-type':'application/proto'}); res.end(bmr()); return;
    }
    if (url.includes('GetDefaultModelForCli')) {
      res.writeHead(200, {'content-type':'application/proto'}); res.end(bdr()); return;
    }
    if (url.includes('Analytics') || url.includes('TrackEvents') || url.includes('SubmitLogs') || url.includes('/v1/traces')) {
      res.writeHead(200, {'content-type':'application/json'}); res.end('{}'); return;
    }
    res.writeHead(200, {'content-type':'application/proto'}); res.end();
  });
});

server.on('stream', (stream, headers) => {
  const path = String(headers[':path'] || '');
  if (!path.includes('/agent.v1.AgentService/')) return;
  log(`agent-stream ${path}`);
  let handled = false;
  const chunks = [];
  const handle = async (force = false) => {
    if (handled) return;
    const strs = [];
    try {
      for (const chunk of chunks) xstr(chunk.length > 5 ? chunk.subarray(5) : chunk, strs);
    } catch {}
    const prompt = selectPrompt(strs);
    if (!force && chunks.length < 8) return;
    handled = true;
    log(`cursor-prompt strings=${strs.length} bytes=${Buffer.byteLength(prompt, 'utf8')}`);
    stream.respond({':status':200, 'content-type':'application/connect+proto'});
    if (process.env.HAKO_CURSOR_PROXY_STATIC_REPLY) {
      stream.write(tdf(process.env.HAKO_CURSOR_PROXY_STATIC_REPLY));
      stream.write(tef()); stream.end(ct());
      log('static-complete');
      return;
    }
    await callOpenRouter(prompt, stream);
  };
  stream.on('data', chunk => {
    if (handled) return;
    chunks.push(chunk);
    void handle();
  });
  stream.on('end', () => { void handle(true); });
  setTimeout(() => { void handle(true); }, 1000).unref();
});

function selectPrompt(strs) {
  const useful = strs.filter(s => s.length > 0 && s.length < 10000 && !/^[a-f0-9]{8}-/.test(s));
  return useful.sort((a, b) => b.length - a.length)[0]
    || 'Reply exactly HAKO_CURSOR_PROXY_OK';
}

async function callOpenRouter(prompt, stream) {
  log(`openrouter-request model=${MODEL}`);
  const r = await fetch('https://openrouter.ai/api/v1/chat/completions', {
    method: 'POST',
    headers: { Authorization: `Bearer ${OPENROUTER_KEY}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ model: MODEL, messages: [{role:'user', content: prompt}], temperature: 0, max_tokens: 32, stream: true }),
  });
  if (!r.ok) {
    const err = await r.text().catch(() => '');
    stream.write(tdf(`OpenRouter error ${r.status}: ${err.slice(0, 200)}`));
    stream.write(tef()); stream.end(ct());
    log(`openrouter-error status=${r.status}`);
    return;
  }
  const reader = r.body.getReader();
  const dec = new TextDecoder();
  let buf = '';
  let emitted = false;
  while (true) {
    const {done, value} = await reader.read();
    if (done) break;
    buf += dec.decode(value, {stream:true});
    const lines = buf.split('\n'); buf = lines.pop() || '';
    for (const line of lines) {
      if (!line.startsWith('data: ')) continue;
      const data = line.slice(6).trim();
      if (data === '[DONE]') continue;
      try {
        const json = JSON.parse(data);
        const content = json.choices?.[0]?.delta?.content;
        if (content) { emitted = true; stream.write(tdf(content)); }
      } catch {}
    }
  }
  if (!emitted) stream.write(tdf('HAKO_CURSOR_PROXY_OK'));
  stream.write(tef()); stream.end(ct());
  log('openrouter-complete');
}

server.listen(443, '0.0.0.0', () => log('cursor-proxy-listening'));
