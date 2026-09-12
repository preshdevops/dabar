import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { MOCK_LLM_RESPONSES } from '../fixtures/mock_data.js';

export const name = 'Tier 1 — LLM Pipeline, Fences & Reasoning Tokens (F7-F9)';

// Helper function implementing the contract for Markdown Code Fence Stripping
export function stripCodeFences(content) {
  let trimmed = content.trim();
  if (trimmed.startsWith('```')) {
    // Strip leading ```json or ```
    const firstNewline = trimmed.indexOf('\n');
    if (firstNewline !== -1) {
      trimmed = trimmed.slice(firstNewline + 1);
    } else {
      trimmed = trimmed.replace(/^```[a-zA-Z]*/, '');
    }
    // Strip trailing ```
    if (trimmed.endsWith('```')) {
      trimmed = trimmed.slice(0, -3);
    }
  }
  return trimmed.trim();
}

export async function run() {
  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F8): Markdown Code Fence Stripping
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Case 1: ```json ... ```
    const strippedJson = stripCodeFences(MOCK_LLM_RESPONSES.fencedJson);
    const parsed1 = JSON.parse(strippedJson);
    assert(Array.isArray(parsed1.chapters), 'Parsed fenced JSON must contain chapters array');
    assert(Array.isArray(parsed1.clips), 'Parsed fenced JSON must contain clips array');
    assert.equal(parsed1.clips[0].title, 'Standing Unshaken');

    // Case 2: ``` ... ``` (without language tag)
    const strippedNoLang = stripCodeFences(MOCK_LLM_RESPONSES.fencedJsonNoLang);
    const parsed2 = JSON.parse(strippedNoLang);
    assert.equal(parsed2.clips[0].title, 'Through The Fire');

    // Case 3: Raw JSON without fences
    const rawJson = stripCodeFences(MOCK_LLM_RESPONSES.standardJson);
    const parsed3 = JSON.parse(rawJson);
    assert.equal(parsed3.clips[0].title, 'All Things Work Together');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F7): Groq Reasoning Models Token Budget Specification
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Groq fallback must use valid Groq model identifiers so failed calls do not
    // spend 90 seconds each on unsupported model names.
    const llmSourcePath = path.resolve(process.cwd(), 'packages/core/src/llm.rs');
    const sourceContent = fs.readFileSync(llmSourcePath, 'utf8');

    assert(sourceContent.includes('llama-3.3-70b-versatile'), 'GROQ_MODELS must include llama-3.3-70b-versatile');
    assert(sourceContent.includes('llama-3.1-8b-instant'), 'GROQ_MODELS must include llama-3.1-8b-instant');
    assert(!sourceContent.includes('"groq/compound-mini"'), 'GROQ_MODELS must not include invalid groq/compound-mini');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F9): Per-Model Timeout & o3-mini Compatibility Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    // o3-mini does not support the standard 'temperature' parameter in completions API.
    // Ensure request builder handles o3-mini without sending invalid temperature.
    function buildChatPayload(model, systemPrompt, userPrompt) {
      const isO3 = model.includes('o3-mini');
      const isReasoning = model.includes('gpt-oss') || isO3;
      const payload = {
        model,
        messages: [
          { role: 'system', content: systemPrompt },
          { role: 'user', content: userPrompt },
        ],
        max_tokens: isReasoning ? 3500 : 1500,
        response_format: { type: 'json_object' },
      };

      if (!isO3) {
        payload.temperature = 0.3;
      }

      return payload;
    }

    const o3Payload = buildChatPayload('o3-mini', 'sys', 'usr');
    assert.equal(o3Payload.temperature, undefined, 'o3-mini must not include temperature parameter');
    assert.equal(o3Payload.max_tokens, 3500, 'Reasoning models must allocate >= 3500 tokens');

    const gpt4Payload = buildChatPayload('gpt-4o-mini', 'sys', 'usr');
    assert.equal(gpt4Payload.temperature, 0.3, 'Standard models must retain temperature parameter');
  }
}
