#!/usr/bin/env bun
// Test-only Amp actor bridge, not Amp's hosted agent implementation.
// The mode selects live OpenRouter inference or explicitly deterministic replies.
import { mkdtemp, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { completeOpenRouterTurn } from './amp-openrouter.mjs';

const [mode, harness = fileURLToPath(new URL('./amp-status-test.py', import.meta.url))] = process.argv.slice(2);
if (mode !== 'openrouter' && mode !== 'deterministic') {
  throw new Error('Usage: amp-proxy-test.mjs <openrouter|deterministic> [harness-path]');
}
const apiKey = mode === 'openrouter' ? process.env.OPENROUTER_API_KEY : undefined;
if (mode === 'openrouter' && !apiKey) throw new Error('OPENROUTER_API_KEY is required');
const model = process.env.GARDN_TEST_MODEL || 'openrouter/free';
const shutdown = new AbortController();
let pendingTurn = false;
let child;

const threadId = `T-${crypto.randomUUID()}`;
const thread = {
  id: threadId, v: 0, created: Date.now(), agentMode: 'medium', messages: [],
  meta: { usesThreadActors: true, usesDtw: true }, env: { trees: [] },
};
let created = false;
let protocolFailed = false;
const token = 'gardn-local-bridge';
const prompts = [
  'Join "GARDN_AMP" and "CI_OK" with an underscore. Reply with only the result. Do not use tools.',
  'Reply with your previous assistant response followed by _RESUMED. Do not use tools.',
];

function fail(message) {
  const firstFailure = !protocolFailed;
  protocolFailed = true;
  console.error(`Amp actor bridge: ${message}`);
  // Interrupt the harness so its cleanup runs; never synthesize a successful turn.
  if (firstFailure && child?.exitCode === null) child.kill('SIGINT');
}

async function completeTurn(notify) {
  pendingTurn = true;
  try {
    let text;
    if (mode === 'deterministic') {
      // Only the offline target synthesizes a provider response and its latency.
      await Bun.sleep(300);
      const previous = thread.messages.findLast(message => message.role === 'assistant');
      text = previous ? `${previous.content[0].text}_RESUMED` : 'GARDN_AMP_CI_OK';
    } else {
      text = await completeOpenRouterTurn({
        messages: thread.messages.map(message => ({
          role: message.role,
          content: message.content.map(block => block.text).join(''),
        })),
        model,
        apiKey,
        baseUrl: process.env.OPENROUTER_BASE_URL || undefined,
        signal: shutdown.signal,
      });
      console.log(`Amp bridge: OpenRouter-compatible response completed (${model})`);
    }
    const assistant = {
      threadId, messageId: `M-${String(thread.v).padStart(22, '0')}`, role: 'assistant',
      content: [{ type: 'text', text }], state: { type: 'complete' },
    };
    thread.messages.push(assistant);
    notify('message_added', { message: assistant, seq: ++thread.v });
    notify('agent_state', { state: 'idle', agentMode: 'medium' });
  } catch (error) {
    fail(error.message);
  } finally {
    pendingTurn = false;
  }
}

const server = Bun.serve({
  hostname: '127.0.0.1',
  port: 0,
  async fetch(request, server) {
    const url = new URL(request.url);
    if (request.headers.get('upgrade')?.toLowerCase() === 'websocket') {
      const isThread = url.pathname === '/actors/gateway/threadActor/websocket/';
      const isUser = url.pathname === '/actors/gateway/userActor/connect';
      if ((!isThread && !isUser) || (isThread && url.searchParams.get('rvt-key') !== threadId)) {
        fail(`Unexpected actor URL: ${url.pathname}`);
        return new Response('Unknown actor', { status: 404 });
      }
      if (server.upgrade(request, { data: { isThread } })) return;
      return new Response('WebSocket upgrade failed', { status: 400 });
    }
    const body = await request.json();
    if (url.pathname === '/api/thread-actors') {
      if (body.threadId ? body.threadId !== threadId || !created : created) {
        fail('Expected creation followed by resume of the same native thread');
        return new Response('Unknown or duplicate thread', { status: 409 });
      }
      created = true;
      return Response.json({
        threadId, wsToken: token, ownerUserId: token, threadVersion: thread.v,
        usesDtw: true, usesThreadActors: true, executorType: 'local-client', poolName: 'gardn',
      });
    }
    if (url.pathname === '/api/user-actor-credentials') {
      return Response.json({ userId: token, wsToken: token, poolName: 'gardn' });
    }
    if (url.pathname !== '/api/internal') {
      fail(`Unexpected HTTP route: ${url.pathname}`);
      return new Response('Unknown route', { status: 404 });
    }
    let result;
    switch (body.method) {
      case 'getUserInfo':
        result = { id: token, displayName: 'Gardn CI', email: 'gardn@example.invalid', features: [], team: null };
        break;
      case 'getThreadTail':
      case 'getThreadInitialTranscriptPage':
        if (body.params.thread !== threadId || !created) {
          fail('Transcript requested for an unknown thread');
          return Response.json({ ok: false, error: 'Thread not found' });
        }
        // Return the whole small transcript so no pagination is necessary.
        result = { thread: { data: thread }, messages: thread.messages, hasMoreBefore: false, firstMessageIndex: 0, firstMessageID: thread.messages[0]?.messageId ?? null };
        break;
      case 'loadPlugins': result = { plugins: [] }; break;
      case 'loadSkills': result = { skills: [] }; break;
      case 'listAgentModes': result = []; break;
      case 'deleteThread':
        if (body.params.thread !== threadId) fail('Deleted a different native thread');
        created = false;
        result = {};
        break;
      case 'setThreadMeta':
      case 'getThreadProject':
      case 'listAutomations':
      case 'threadDisplayCostInfo':
      case 'splashDispatch':
      case 'notices':
      case 'logSplashDispatchAction':
        result = {};
        break;
      default:
        fail(`Unsupported HTTP method: ${body.method}`);
        return Response.json({ ok: false, error: 'Unsupported fixture method' });
    }
    return Response.json({ ok: true, result });
  },
  websocket: {
    message(ws, data) {
      if (String(data) === 'ping') { ws.send('pong'); return; }
      // User-actor subscriptions carry account notifications, not thread lifecycle.
      if (!ws.data.isThread) return;
      const request = JSON.parse(String(data));
      const notify = (method, params) => ws.send(JSON.stringify({ jsonrpc: '2.0', method, params }));
      switch (request.method) {
        case 'executor_connect':
          notify('executor_connected', { executorId: request.params.clientId, registeredToolCount: 0, guidanceInventory: [], resumeBootstrap: true });
          break;
        case 'client_append_user_msg': {
          const content = request.params?.content;
          if (pendingTurn || !Array.isArray(content) || content.length === 0
            || content.some(block => block.type !== 'text' || typeof block.text !== 'string')) {
            fail('Expected one text-only turn at a time');
            return;
          }
          if (mode === 'deterministic') {
            const turn = thread.messages.filter(message => message.role === 'assistant').length;
            if (content.map(block => block.text).join('') !== prompts[turn]) {
              fail(`Unexpected deterministic prompt for turn ${turn + 1}`);
              return;
            }
          }
          const user = { threadId, messageId: request.params.messageId, role: 'user', content };
          thread.messages.push(user);
          notify('message_added', { message: user, seq: ++thread.v });
          notify('agent_state', { state: 'working', agentMode: 'medium' });
          void completeTurn(notify);
          break;
        }
        case 'client_resume':
        case 'client_mark_message_read':
        case 'executor_environment_snapshot':
        case 'executor_skill_snapshot':
        case 'executor_guidance_snapshot':
        case 'executor_tools_register':
        case 'executor_tools_bootstrap_complete':
        case 'executor_plugin_message':
          break;
        default:
          fail(`Unsupported actor method: ${request.method}`);
          ws.close(1008, 'Unsupported fixture method');
          return;
      }
      if (request.id !== undefined) {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {} }));
      }
    },
  },
});

const home = await mkdtemp('/tmp/gardn-amp-home-');
try {
  child = Bun.spawn(['python3', harness], {
    cwd: home,
    // Amp receives only the local bridge token, never the OpenRouter key or host login.
    env: {
      PATH: process.env.PATH,
      HOME: home,
      XDG_CONFIG_HOME: `${home}/config`,
      XDG_CACHE_HOME: `${home}/cache`,
      XDG_DATA_HOME: `${home}/data`,
      AMP_API_KEY: token,
      AMP_URL: server.url.origin,
      RIVET_PUBLIC_ENDPOINT: `${server.url.origin}/actors`,
      GARDN_REPO_DIR: process.env.GARDN_REPO_DIR ?? '/repo',
      GARDN_AMP_STATUS_TIMEOUT: process.env.GARDN_AMP_STATUS_TIMEOUT ?? (mode === 'openrouter' ? '180' : '30'),
    },
    stdout: 'inherit', stderr: 'inherit',
  });
  console.log(`Amp integration mode: ${mode}; actor service and Gardn receiver are test doubles`);
  process.exitCode = await child.exited;
  if (protocolFailed) process.exitCode = 1;
} finally {
  shutdown.abort();
  await server.stop(true);
  await rm(home, { recursive: true, force: true });
}
