import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  probeMedia,
  resolveBinary,
  ensureTestDir,
} from '../fixtures/media_generator.js';

export const name = 'Tier 2 — Boundary & Corner Cases: Odd Pixel Dimensions';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');

  const sourceOdd = generateSyntheticVideo('odd_source_1919x1079.mp4', {
    duration: 5,
    width: 1919,
    height: 1079,
  });

  // ──────────────────────────────────────────────────────────────────────────
  // Test 1: Odd Input Rendered to 9:16 Vertical
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out9x16 = path.join(testDir, 'odd_to_9x16.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceOdd, '-t', '1.0',
      '-filter_complex',
      '[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      out9x16,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Odd input to 9:16 failed: ${res.stderr}`);

    const meta = probeMedia(out9x16);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1920);
    assert.equal(meta.width % 2, 0, 'Width must be divisible by 2');
    assert.equal(meta.height % 2, 0, 'Height must be divisible by 2');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2: Odd Input Rendered to 1:1 Square
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out1x1 = path.join(testDir, 'odd_to_1x1.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceOdd, '-t', '1.0',
      '-filter_complex',
      '[0:v]split[fg_in][bg_in];[bg_in]scale=108:108:force_original_aspect_ratio=increase,crop=108:108,boxblur=5:2,scale=1080:1080:flags=bilinear[bg];[fg_in]scale=1080:1080:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      out1x1,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Odd input to 1:1 failed: ${res.stderr}`);

    const meta = probeMedia(out1x1);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1080);
    assert.equal(meta.width % 2, 0, 'Width must be divisible by 2');
    assert.equal(meta.height % 2, 0, 'Height must be divisible by 2');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3: Odd Input Rendered to 16:9 Widescreen
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out16x9 = path.join(testDir, 'odd_to_16x9.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceOdd, '-t', '1.0',
      '-filter_complex',
      '[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      out16x9,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Odd input to 16:9 failed: ${res.stderr}`);

    const meta = probeMedia(out16x9);
    assert.equal(meta.width, 1920);
    assert.equal(meta.height, 1080);
    assert.equal(meta.width % 2, 0, 'Width must be divisible by 2');
    assert.equal(meta.height % 2, 0, 'Height must be divisible by 2');
  }
}
