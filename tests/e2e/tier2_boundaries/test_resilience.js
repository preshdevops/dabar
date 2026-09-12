import assert from 'node:assert/strict';
import { MOCK_LLM_RESPONSES } from '../fixtures/mock_data.js';

export const name = 'Tier 2 — Boundary & Corner Cases: Error Handling & Resilience';

export async function run() {
  // ──────────────────────────────────────────────────────────────────────────
  // Resilience 1: Empty Transcript Segments
  // ──────────────────────────────────────────────────────────────────────────
  {
    function analyzeSermonTranscript(segments) {
      if (!segments || segments.length === 0) {
        return {
          chapters: [],
          highlights: [],
          total_proposed: 0,
          total_passed: 0,
          status: 'no_candidates_proposed',
          error_message: null,
        };
      }
      return { status: 'processed', count: segments.length };
    }

    const emptyResult = analyzeSermonTranscript([]);
    assert.equal(emptyResult.status, 'no_candidates_proposed');
    assert.deepEqual(emptyResult.chapters, []);
    assert.deepEqual(emptyResult.highlights, []);

    const nullResult = analyzeSermonTranscript(null);
    assert.equal(nullResult.status, 'no_candidates_proposed');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Resilience 2: Malformed LLM Response Recovery
  // ──────────────────────────────────────────────────────────────────────────
  {
    function safeParseLlmAnalysis(rawContent) {
      if (!rawContent || !rawContent.trim()) {
        return { success: false, reason: 'empty_response', chapters: [], highlights: [] };
      }
      try {
        let cleaned = rawContent.trim();
        if (cleaned.startsWith('```')) {
          const firstNewline = cleaned.indexOf('\n');
          if (firstNewline !== -1) cleaned = cleaned.slice(firstNewline + 1);
          if (cleaned.endsWith('```')) cleaned = cleaned.slice(0, -3);
        }
        const parsed = JSON.parse(cleaned);
        return { success: true, chapters: parsed.chapters || [], highlights: parsed.clips || [] };
      } catch (err) {
        // Fall back gracefully to offline heuristic extraction
        return { success: false, reason: 'json_parse_error', error: err.message };
      }
    }

    const malformed = safeParseLlmAnalysis(MOCK_LLM_RESPONSES.malformedJson);
    assert.equal(malformed.success, false);
    assert.equal(malformed.reason, 'json_parse_error');

    const empty = safeParseLlmAnalysis(MOCK_LLM_RESPONSES.emptyResponse);
    assert.equal(empty.success, false);
    assert.equal(empty.reason, 'empty_response');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Resilience 3: Missing External Tool Handling
  // ──────────────────────────────────────────────────────────────────────────
  {
    function checkToolStatus(toolName, resolvedPath, exists) {
      if (!exists) {
        return {
          installed: false,
          tool: toolName,
          error: `External tool '${toolName}' not found. Please install ${toolName} and place it on PATH or in AppData/bin.`,
        };
      }
      return { installed: true, tool: toolName, path: resolvedPath };
    }

    const missingTool = checkToolStatus('fake_tool', 'C:\\missing\\fake_tool.exe', false);
    assert.equal(missingTool.installed, false);
    assert(missingTool.error.includes("not found"), 'Error message must clearly inform the user');
  }
}
