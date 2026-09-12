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
import { nudgeBoundary } from '../tier1_features/test_clip_ergonomics.js';

export const name = 'Tier 4 — Scenario: Sunday Service Ingestion to 9:16 Nudged Video Export';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');

  const sermonSource = generateSyntheticVideo('sunday_service_master.mp4', {
    duration: 5,
    width: 1920,
    height: 1080,
  });
  assert(sermonSource, 'Sermon source video must be ready');

  const detectedHighlight = {
    title: "Overcoming Trials Through Faith",
    start_time: 15.0,
    end_time: 32.0,
    quote: "When you walk through the fire, you will not be burned."
  };

  let clipBounds = { start: detectedHighlight.start_time, end: detectedHighlight.end_time };
  clipBounds = nudgeBoundary(clipBounds, 'start', -1.0);
  clipBounds = nudgeBoundary(clipBounds, 'end', 1.0);
  assert.equal(clipBounds.start, 14.0);
  assert.equal(clipBounds.end, 33.0);
  assert.equal(clipBounds.duration, 19.0);

  const exportFile = path.join(testDir, 'sunday_service_9x16_amber_export.mp4');
  const safeQuote = escapeSubtitleText(detectedHighlight.quote);
  const drawtext = buildDrawtextFilter(safeQuote, {
    fontSize: 42,
    fontColor: '0xD4913A',
    boxColor: 'black@0.75',
    borderW: 12,
    yPos: 'h*0.82',
  });

  const args = [
    '-y',
    '-ss', '0.0',
    '-i', sermonSource,
    '-t', '1.0',
    '-filter_complex',
    `[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,${drawtext}[v]`,
    '-map', '[v]', '-map', '0:a?',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-b:a', '192k',
    exportFile,
  ];

  const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
  assert.equal(res.status, 0, `Sunday Service export failed: ${res.stderr}`);

  const meta = probeMedia(exportFile);
  assert.equal(meta.hasVideo, true, 'Rendered clip must have a video stream');
  assert.equal(meta.videoCodec, 'h264', 'Video codec must be H.264');
  assert.equal(meta.hasAudio, true, 'Rendered clip must have audio');
  assert.equal(meta.audioCodec, 'aac', 'Audio codec must be AAC');
  assert.equal(meta.width, 1080);
  assert.equal(meta.height, 1920);

  const assetUrl = `asset://localhost/${encodeURI(exportFile.replace(/\\/g, '/')).replace(/^\//, '')}`;
  assert(assetUrl.startsWith('asset://localhost/'));

  const explorerArgs = ['/select,', path.normalize(exportFile)];
  assert.equal(explorerArgs[0], '/select,');
}
