#!/usr/bin/env node
import assert from 'node:assert/strict';
import { isInferenceUrl } from './qoder-openrouter-proxy.mjs';

const cases = [
  {
    name: 'old agent_chat_generation endpoint is intercepted',
    url: '/api/v1/agent_chat_generation',
    expected: true,
  },
  {
    name: 'agent chat completions endpoint is intercepted',
    url: '/api/agent/chat/completions',
    expected: true,
  },
  {
    name: 'model list endpoint stays on the model-list route',
    url: '/model/list',
    expected: false,
  },
];

for (const { name, url, expected } of cases) {
  assert.equal(isInferenceUrl(url), expected, `${name}: ${url}`);
}

console.log('qoder proxy inference URL matching test ok');
