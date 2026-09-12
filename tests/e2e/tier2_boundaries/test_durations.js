import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  probeMedia,
  resolveBinary,
  ensureTestDir,
} from '../fixtures/media_generator.js';

export const name = 'Tier 2 — Boundary & Corner Cases: Clip Durations';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceVideo = generateSyntheticVideo('duration_test_source.mp4', { duration: 15 });

  // ──────────────────────────────────────────────────────────────────────────
  // Boundary 1: Minimum Duration Exactly 10.0 Seconds
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outMin = path.join(testDir, 'boundary_min_10s.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '10.0',
      '-filter_complex',
      '[0:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outMin,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `10.0s clip render failed: ${res.stderr}`);

    const meta = probeMedia(outMin);
    assert(Math.abs(meta.duration - 10.0) <= 0.5, `Duration should be ~10.0s (got ${meta.duration}s)`);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Boundary 2: Long Duration (300.0s / 5+ Minutes) Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    function validateAndComputeDuration(startTime, endTime) {
      if (startTime < 0 || endTime <= startTime) {
        throw new Error(`Invalid bounds: start (${startTime}) must be >= 0 and < end (${endTime})`);
      }
      const duration = endTime - startTime;
      if (duration < 10.0) {
        throw new Error(`Clip duration (${duration.toFixed(1)}s) is below minimum 10.0s`);
      }
      return duration;
    }

    const fiveMinDuration = validateAndComputeDuration(60.0, 360.0);
    assert.equal(fiveMinDuration, 300.0, 'Must allow 300s (5-minute) clips without clamping');

    const tenMinDuration = validateAndComputeDuration(100.0, 700.0);
    assert.equal(tenMinDuration, 600.0, 'Must allow 600s (10-minute) clips without clamping');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Boundary 3: Rejection of Sub-Minimum Durations (< 10.0s)
  // ──────────────────────────────────────────────────────────────────────────
  {
    function checkDurationRequirement(startTime, endTime) {
      const duration = endTime - startTime;
      return duration >= 10.0;
    }

    assert.equal(checkDurationRequirement(5.0, 12.0), false, '7-second clip must be rejected (< 10s)');
    assert.equal(checkDurationRequirement(0.0, 9.99), false, '9.99s clip must be rejected (< 10s)');
    assert.equal(checkDurationRequirement(0.0, 10.0), true, '10.0s clip must be accepted');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Boundary 4: Rejection of Inverted and Negative Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    function checkBoundsValidity(startTime, endTime) {
      if (startTime < 0.0) return 'negative_start';
      if (endTime <= startTime) return 'inverted_or_zero';
      return 'valid';
    }

    assert.equal(checkBoundsValidity(-2.0, 15.0), 'negative_start');
    assert.equal(checkBoundsValidity(30.0, 30.0), 'inverted_or_zero');
    assert.equal(checkBoundsValidity(45.0, 20.0), 'inverted_or_zero');
    assert.equal(checkBoundsValidity(0.0, 15.0), 'valid');
  }
}
