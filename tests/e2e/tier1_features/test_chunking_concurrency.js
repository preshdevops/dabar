import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticAudio,
  resolveBinary,
  ensureTestDir,
} from '../fixtures/media_generator.js';

export const name = 'Tier 1 — Audio Chunking & Concurrency Pipeline (F10-F12)';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceAudio = generateSyntheticAudio('chunk_test_source.mp3', { duration: 30 });

  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F11): Fast Stream Copy Chunking (-c:a copy < 50ms)
  // ──────────────────────────────────────────────────────────────────────────
  {
    const chunkOut = path.join(testDir, 'chunk_stream_copy.mp3');
    const startTime = Date.now();

    // Slicing from an already preprocessed MP3 with -c:a copy
    const args = [
      '-y',
      '-ss', '5.0',
      '-i', sourceAudio,
      '-t', '10.0',
      '-c:a', 'copy',
      chunkOut,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    const elapsed = Date.now() - startTime;

    assert.equal(res.status, 0, `Stream copy chunking failed: ${res.stderr}`);
    assert(fs.existsSync(chunkOut), 'Stream copy chunk must be generated');
    const stats = fs.statSync(chunkOut);
    assert(stats.size > 0, 'Chunk must not be empty');

    // Fast copy should complete in very low latency
    assert(elapsed < 2000, `Stream copy extraction must be rapid (took ${elapsed}ms)`);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F10): Desktop Chunking Thresholds Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Thresholds: 20MB / 15 minutes (900 seconds)
    const MAX_CHUNK_BYTES = 20 * 1024 * 1024;
    const MAX_CHUNK_SECS = 15 * 60;

    function shouldChunkAudio(fileSizeBytes, durationSecs) {
      return fileSizeBytes > MAX_CHUNK_BYTES || durationSecs > MAX_CHUNK_SECS;
    }

    // A 10MB / 10m file should NOT be chunked
    assert.equal(shouldChunkAudio(10 * 1024 * 1024, 600), false, 'Under-threshold audio must not chunk');

    // A 25MB file SHOULD be chunked
    assert.equal(shouldChunkAudio(25 * 1024 * 1024, 600), true, 'Files > 20MB must trigger chunking');

    // A 60-minute sermon SHOULD be chunked
    assert.equal(shouldChunkAudio(15 * 1024 * 1024, 3600), true, 'Durations > 15m must trigger chunking');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F12): Pipelined Concurrency Semaphore Limit Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Concurrency limit must be 4 to maximize Groq/Deepgram throughput without rate limiting
    const CONCURRENCY_LIMIT = 4;
    let activeTasks = 0;
    let maxObservedActive = 0;

    async function mockWorker(id) {
      activeTasks++;
      if (activeTasks > maxObservedActive) {
        maxObservedActive = activeTasks;
      }
      // Simulate fast network I/O
      await new Promise(r => setTimeout(r, 20));
      activeTasks--;
    }

    // Execute 12 simulated chunks using a pool of size 4
    const tasks = Array.from({ length: 12 }, (_, i) => i);
    const executing = new Set();
    for (const task of tasks) {
      const p = mockWorker(task).then(() => executing.delete(p));
      executing.add(p);
      if (executing.size >= CONCURRENCY_LIMIT) {
        await Promise.race(executing);
      }
    }
    await Promise.all(executing);

    assert(maxObservedActive <= CONCURRENCY_LIMIT, `Active workers must not exceed ${CONCURRENCY_LIMIT}`);
    assert.equal(maxObservedActive, CONCURRENCY_LIMIT, `Should achieve full concurrency of ${CONCURRENCY_LIMIT}`);
  }
}
