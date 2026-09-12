import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { resolveBinary } from '../fixtures/media_generator.js';

export const name = 'Tier 1 — Unified External Tool Discovery (F6)';

export async function run() {
  const tools = ['ffmpeg', 'ffprobe', 'yt-dlp'];

  for (const tool of tools) {
    const resolvedPath = resolveBinary(tool);
    assert(resolvedPath, `Tool ${tool} must resolve to a valid command or path`);

    // Verify tool execution
    const versionFlag = tool === 'ffmpeg' || tool === 'ffprobe' ? '-version' : '--version';
    const result = spawnSync(resolvedPath, [versionFlag], { encoding: 'utf8', timeout: 35000 });

    assert.equal(
      result.status,
      0,
      `Resolved tool ${tool} at '${resolvedPath}' must execute successfully with exit code 0. Error: ${result.stderr}`
    );
    assert(
      result.stdout && result.stdout.trim().length > 0,
      `Resolved tool ${tool} must output non-empty version information`
    );
  }

  // Verify Environment Override Contract
  // When FFMPEG_PATH is explicitly set, the resolution mechanism must honor it
  const repoFfmpeg = path.resolve(process.cwd(), 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg');
  if (fs.existsSync(repoFfmpeg)) {
    const originalEnv = process.env.FFMPEG_PATH;
    try {
      process.env.FFMPEG_PATH = repoFfmpeg;
      const resolved = resolveBinary('ffmpeg');
      assert.equal(resolved, repoFfmpeg, 'Custom FFMPEG_PATH environment variable must override default lookup');
    } finally {
      if (originalEnv !== undefined) {
        process.env.FFMPEG_PATH = originalEnv;
      } else {
        delete process.env.FFMPEG_PATH;
      }
    }
  }
}
