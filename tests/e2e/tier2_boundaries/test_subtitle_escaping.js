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
import { ADVERSARIAL_SUBTITLES } from '../fixtures/mock_data.js';

export const name = 'Tier 2 — Boundary & Corner Cases: Subtitle Escaping & Special Chars';

export function escapeSubtitleText(text) {
  return text
    .replace(/\\/g, '\\\\')
    .replace(/'/g, "\\'")
    .replace(/:/g, '\\:')
    .replace(/%/g, '\\%');
}

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceVideo = generateSyntheticVideo('sub_escape_source.mp4', { duration: 5 });

  for (let i = 0; i < ADVERSARIAL_SUBTITLES.length; i++) {
    const rawSubtitle = ADVERSARIAL_SUBTITLES[i];
    const safeSubtitle = escapeSubtitleText(rawSubtitle);
    const outFile = path.join(testDir, `adversarial_sub_${i}.mp4`);

    const drawtextFilter = buildDrawtextFilter(safeSubtitle, {
      fontSize: 36,
      fontColor: 'white',
      boxColor: 'black@0.65',
      borderW: 10,
      yPos: 'h*0.82',
    });

    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      `[0:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,${drawtextFilter}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outFile,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(
      res.status,
      0,
      `Adversarial subtitle #${i} failed to burn in: "${rawSubtitle}". Error: ${res.stderr}`
    );

    const meta = probeMedia(outFile);
    assert.equal(meta.hasVideo, true);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1920);
  }
}
