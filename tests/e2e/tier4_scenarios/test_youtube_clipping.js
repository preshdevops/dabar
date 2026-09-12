import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  probeMedia,
  resolveBinary,
  ensureTestDir,
} from '../fixtures/media_generator.js';

export const name = 'Tier 4 — Scenario: YouTube Sermon Section Download to 16:9 Widescreen Export';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');

  const downloadedSection = generateSyntheticVideo('youtube_section_downloaded.mp4', {
    duration: 10,
    width: 1920,
    height: 1080,
  });

  const exportFile = path.join(testDir, 'youtube_16x9_widescreen_export.mp4');

  const args = [
    '-y',
    '-ss', '0.0',
    '-i', downloadedSection,
    '-t', '1.0',
    '-filter_complex',
    '[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black[v]',
    '-map', '[v]', '-map', '0:a?',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-b:a', '192k',
    exportFile,
  ];

  const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
  assert.equal(res.status, 0, `YouTube 16:9 render failed: ${res.stderr}`);

  const meta = probeMedia(exportFile);
  assert.equal(meta.hasVideo, true, 'YouTube export MUST contain real video footage, not an audio waveform card');
  assert.equal(meta.videoCodec, 'h264');
  assert.equal(meta.width, 1920);
  assert.equal(meta.height, 1080);
}
