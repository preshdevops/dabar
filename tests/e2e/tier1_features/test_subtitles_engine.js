import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  probeMedia,
  resolveBinary,
  ensureTestDir,
  buildDrawtextFilter,
} from '../fixtures/media_generator.js';

export const name = 'Tier 1 — Subtitles Engine & Style Presets (F5, F21)';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceVideo = generateSyntheticVideo('subtitles_source.mp4', { duration: 5, width: 1920, height: 1080 });

  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F5 & F21): Warm Amber Subtitle Style Burn-in
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outWarmAmber = path.join(testDir, 'warm_amber_subtitles.mp4');
    const amberCaptionText = "God is faithful: Romans 8\\:28";
    const safeText = amberCaptionText
      .replace(/\\/g, '\\\\')
      .replace(/'/g, "\\'")
      .replace(/:/g, '\\:')
      .replace(/%/g, '\\%');

    const drawtextFilter = buildDrawtextFilter(safeText, {
      fontSize: 42,
      fontColor: '0xD4913A',
      boxColor: 'black@0.75',
      borderW: 12,
      yPos: 'h*0.82',
    });

    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      `[0:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,${drawtextFilter}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k',
      outWarmAmber,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Warm Amber subtitle burn failed: ${res.stderr}`);

    const meta = probeMedia(outWarmAmber);
    assert.equal(meta.hasVideo, true, 'Rendered clip must have video');
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1920);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F5): Kinetic Subtitle Preset Burn-in (Sans-serif, Bold, Dynamic)
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outKinetic = path.join(testDir, 'kinetic_subtitles.mp4');
    const caption = "STAND FIRM IN THE FAITH!";
    const drawtextFilter = buildDrawtextFilter(caption, {
      fontSize: 48,
      fontColor: 'white',
      boxColor: '0xD4913A@0.85',
      borderW: 14,
      yPos: 'h*0.80',
    });

    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      `[0:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,${drawtextFilter}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outKinetic,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Kinetic subtitle burn failed: ${res.stderr}`);

    const meta = probeMedia(outKinetic);
    assert.equal(meta.hasVideo, true);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F5): Sacred Editorial Subtitle Preset (Serif, Elegant, High-contrast)
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outEditorial = path.join(testDir, 'sacred_editorial_subtitles.mp4');
    const caption = "For by grace you have been saved through faith.";
    const drawtextFilter = buildDrawtextFilter(caption, {
      fontSize: 40,
      fontColor: '0xF5F0E6',
      boxColor: '0x0F1117@0.80',
      borderW: 10,
      yPos: 'h*0.84',
    });

    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      `[0:v]scale=1080:1080:force_original_aspect_ratio=decrease,pad=1080:1080:(ow-iw)/2:(oh-ih)/2,${drawtextFilter}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outEditorial,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Sacred Editorial subtitle burn failed: ${res.stderr}`);

    const meta = probeMedia(outEditorial);
    assert.equal(meta.hasVideo, true);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1080);
  }
}
