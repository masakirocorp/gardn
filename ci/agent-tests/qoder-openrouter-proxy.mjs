#!/usr/bin/env node
// Minimal Qoder -> OpenRouter test proxy. Test-only: keep Qoder auth/catalog
// traffic real, intercept the entitlement-gated inference stream, and emit the
// same Qoder SSE envelope shape the CLI expects.
import https from 'node:https';
import dns from 'node:dns/promises';
import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const LOG = process.env.OMH_QODER_PROXY_LOG || '/tmp/omh-qoder-proxy.log';
const CERT = process.env.OMH_QODER_PROXY_CERT;
const KEY = process.env.OMH_QODER_PROXY_KEY;
const MODEL = process.env.OMH_TEST_QODER_PROXY_MODEL || process.env.OMH_TEST_MODEL || 'poolside/laguna-m.1:free';
const OPENROUTER_KEY = process.env.OPENROUTER_API_KEY || '';

const isDirectRun = Boolean(process.argv[1]) && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isDirectRun) {
  if (!CERT || !KEY) throw new Error('OMH_QODER_PROXY_CERT and OMH_QODER_PROXY_KEY are required');
  if (!OPENROUTER_KEY) throw new Error('OPENROUTER_API_KEY is required');
}

function log(msg) {
  appendFileSync(LOG, `${new Date().toISOString()} ${msg}\n`);
}

export function isInferenceUrl(url) {
  const lower = url.toLowerCase();
  return lower.includes('/agent_chat_generation')
    || (lower.includes('agent') && lower.includes('chat'))
    || (lower.includes('chat') && lower.includes('generation'))
    || (lower.includes('chat') && lower.includes('completion'));
}

function createProxyServer() {
  return https.createServer({ cert: readFileSync(CERT), key: readFileSync(KEY) }, (req, res) => {
    const chunks = [];
    req.on('data', c => chunks.push(c));
    req.on('end', async () => {
      const body = Buffer.concat(chunks);
      const url = req.url || '';
      log(`request ${req.method} ${url} bytes=${body.length}`);
      try {
        if (isInferenceUrl(url)) {
          await handleInference(req, res);
          return;
        }
        if (url.includes('/model/list')) {
          await forwardModelList(req, res, body);
          return;
        }
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ code: 0, success: true, data: {} }));
      } catch (err) {
        log(`proxy-error ${err?.stack || err}`);
        res.writeHead(500, { 'content-type': 'text/plain' });
        res.end(String(err?.message || err));
      }
    });
  });
}

async function forwardModelList(req, res, body) {
  const upstream = await requestRealQoderApi2(req, body);
  log(`model-list status=${upstream.status}`);
  res.writeHead(upstream.status, { 'content-type': upstream.contentType || 'application/json' });
  res.end(upstream.body);
}

async function requestRealQoderApi2(req, body) {
  const addresses = await dns.resolve4('api2.qoder.sh');
  const host = addresses[0];
  const headers = {};
  for (const [key, value] of Object.entries(req.headers)) {
    if (key === 'host' || key === 'connection' || key === 'content-length') continue;
    headers[key] = value;
  }
  headers.host = 'api2.qoder.sh';
  if (body.length) headers['content-length'] = body.length;
  return await new Promise((resolve, reject) => {
    const upstream = https.request({
      host,
      servername: 'api2.qoder.sh',
      path: req.url,
      method: req.method,
      headers,
    }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({
        status: response.statusCode || 502,
        contentType: response.headers['content-type'],
        body: Buffer.concat(chunks),
      }));
    });
    upstream.on('error', reject);
    if (body.length) upstream.write(body);
    upstream.end();
  });
}

async function handleInference(req, res) {
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  if (process.env.OMH_QODER_PROXY_STATIC_REPLY) {
    writeQoderChunk(res, process.env.OMH_QODER_PROXY_STATIC_REPLY);
    writeQoderDone(res);
    res.end();
    log('static-complete');
    return;
  }
  log(`openrouter-request model=${MODEL}`);
  const upstream = await fetch('https://openrouter.ai/api/v1/chat/completions', {
    method: 'POST',
    headers: { Authorization: `Bearer ${OPENROUTER_KEY}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: MODEL,
      messages: [{ role: 'user', content: 'Reply exactly OMH_QODER_PROXY_OK' }],
      temperature: 0,
      max_tokens: 32,
      stream: true,
    }),
  });
  if (!upstream.ok || !upstream.body) {
    const text = await upstream.text().catch(() => '');
    writeQoderChunk(res, `OpenRouter error ${upstream.status}: ${text.slice(0, 200)}`);
    writeQoderDone(res);
    res.end();
    log(`openrouter-error status=${upstream.status}`);
    return;
  }

  const reader = upstream.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let emitted = false;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';
    for (const line of lines) {
      if (!line.startsWith('data: ')) continue;
      const data = line.slice(6).trim();
      if (!data || data === '[DONE]') continue;
      try {
        const json = JSON.parse(data);
        const content = json.choices?.[0]?.delta?.content;
        if (content) {
          emitted = true;
          writeQoderChunk(res, content);
        }
      } catch {}
    }
  }
  if (!emitted) writeQoderChunk(res, 'OMH_QODER_PROXY_OK');
  writeQoderDone(res);
  res.end();
  log('openrouter-complete');
}

function writeQoderChunk(res, content) {
  const inner = {
    id: 'chatcmpl-omh-qoder-test',
    object: 'chat.completion.chunk',
    created: Math.floor(Date.now() / 1000),
    model: 'qmodel_latest',
    choices: [{ index: 0, delta: { role: 'assistant', content }, finish_reason: null }],
  };
  res.write(`data: ${JSON.stringify({ statusCodeValue: 200, body: JSON.stringify(inner) })}\n\n`);
}

function writeQoderDone(res) {
  const inner = {
    id: 'chatcmpl-omh-qoder-test',
    object: 'chat.completion.chunk',
    created: Math.floor(Date.now() / 1000),
    model: 'qmodel_latest',
    choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
  };
  res.write(`data: ${JSON.stringify({ statusCodeValue: 200, body: JSON.stringify(inner) })}\n\n`);
  res.write(`data: ${JSON.stringify({ statusCodeValue: 200, body: '[DONE]' })}\n\n`);
}

if (isDirectRun) {
  createProxyServer().listen(443, '0.0.0.0', () => log('qoder-proxy-listening'));
}
