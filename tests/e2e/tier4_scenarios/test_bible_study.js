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
import { escapeSubtitleText } from '../tier2_boundaries/test_subtitle_escaping.js';

export const name = 'Tier 4 — Scenario: Midweek Bible Study Local Video to 1:1 Square Blur Export';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');

  const bibleStudySource = generateSyntheticVideo('bible_study_cam.mp4', {
    duration: 5,
    width: 1920,
    height: 1080,
  });

  const exportFile = path.join(testDir, 'bible_study_1x1_square_export.mp4');
  const caption = "Hebrews 4:12 — The word of God is quick and powerful.";
  const safeCaption = escapeSubtitleText(caption);
  const drawtext = buildDrawtextFilter(safeCaption, {
    fontSize: 38,
    fontColor: '0xF5F0E6',
    boxColor: '0x0F1117@0.80',
    borderW: 10,
    yPos: 'h*0.84',
  });

  const args = [
    '-y',
    '-ss', '0.0',
    '-i', bibleStudySource,
    '-t', '1.0',
    '-filter_complex',
    `[0:v]split[fg_in][bg_in];[bg_in]scale=108:108:force_original_aspect_ratio=increase,crop=108:108,boxblur=5:2,scale=1080:1080:flags=bilinear[bg];[fg_in]scale=1080:1080:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,${drawtext}[v]`,
    '-map', '[v]', '-map', '0:a?',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-b:a', '192k',
    exportFile,
  ];

  const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
  assert.equal(res.status, 0, `Bible Study 1:1 render failed: ${res.stderr}`);

  const meta = probeMedia(exportFile);
  assert.equal(meta.hasVideo, true);
  assert.equal(meta.videoCodec, 'h264');
  assert.equal(meta.width, 1080);
  assert.equal(meta.height, 1080);
  assert.equal(meta.width % 2, 0);
  assert.equal(meta.height % 2, 0);
}
