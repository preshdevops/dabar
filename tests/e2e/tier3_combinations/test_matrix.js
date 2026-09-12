import assert from 'node:assert/strict';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  generateSyntheticAudio,
  probeMedia,
  resolveBinary,
  ensureTestDir,
  buildDrawtextFilter,
} from '../fixtures/media_generator.js';
import { escapeSubtitleText } from '../tier2_boundaries/test_subtitle_escaping.js';
import { nudgeBoundary } from '../tier1_features/test_clip_ergonomics.js';

export const name = 'Tier 3 — Cross-Feature Combinations Matrix';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceVideo = generateSyntheticVideo('combo_source_video.mp4', { duration: 10, width: 1920, height: 1080 });
  const sourceAudio = generateSyntheticAudio('combo_source_audio.mp3', { duration: 10 });

  // ──────────────────────────────────────────────────────────────────────────
  // Combination 1: Video Source + 9:16 Vertical + Warm Amber Captions + Nudged Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    const initialBounds = { start: 10.0, end: 24.0 };
    const nudgedBounds = nudgeBoundary(initialBounds, 'start', -1.0);
    assert.equal(nudgedBounds.start, 9.0);
    assert.equal(nudgedBounds.duration, 15.0);

    const outC1 = path.join(testDir, 'combo_1_9x16_amber_nudged.mp4');
    const safeCaption = escapeSubtitleText("Romans 8:28 — 'All things work together'");
    const drawtext = buildDrawtextFilter(safeCaption, {
      fontSize: 40,
      fontColor: '0xD4913A',
      boxColor: 'black@0.7',
      borderW: 10,
      yPos: 'h*0.82',
    });

    const args = [
      '-y',
      '-ss', '0.0',
      '-i', sourceVideo,
      '-t', '1.0',
      '-filter_complex',
      `[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,${drawtext}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outC1,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Combo 1 failed: ${res.stderr}`);

    const meta = probeMedia(outC1);
    assert.equal(meta.hasVideo, true);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1920);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Combination 2: Video Source + 1:1 Square + Kinetic Captions + Blur Background
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outC2 = path.join(testDir, 'combo_2_1x1_kinetic_blur.mp4');
    const safeCaption = escapeSubtitleText("THE BATTLE BELONGS TO GOD!");
    const drawtext = buildDrawtextFilter(safeCaption, {
      fontSize: 46,
      fontColor: 'white',
      boxColor: '0xD4913A@0.9',
      borderW: 12,
      yPos: 'h*0.80',
    });

    const args = [
      '-y',
      '-ss', '0.0',
      '-i', sourceVideo,
      '-t', '1.0',
      '-filter_complex',
      `[0:v]split[fg_in][bg_in];[bg_in]scale=108:108:force_original_aspect_ratio=increase,crop=108:108,boxblur=5:2,scale=1080:1080:flags=bilinear[bg];[fg_in]scale=1080:1080:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,${drawtext}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outC2,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Combo 2 failed: ${res.stderr}`);

    const meta = probeMedia(outC2);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1080);
    assert.equal(meta.width % 2, 0);
    assert.equal(meta.height % 2, 0);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Combination 3: Video Source + 16:9 Widescreen + Sacred Editorial Captions
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outC3 = path.join(testDir, 'combo_3_16x9_editorial.mp4');
    const safeCaption = escapeSubtitleText("Peace I leave with you; my peace I give you.");
    const drawtext = buildDrawtextFilter(safeCaption, {
      fontSize: 38,
      fontColor: '0xF5F0E6',
      boxColor: '0x0F1117@0.75',
      borderW: 8,
      yPos: 'h*0.86',
    });

    const args = [
      '-y',
      '-ss', '0.0',
      '-i', sourceVideo,
      '-t', '1.0',
      '-filter_complex',
      `[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black,${drawtext}[v]`,
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac',
      outC3,
    ];

    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Combo 3 failed: ${res.stderr}`);

    const meta = probeMedia(outC3);
    assert.equal(meta.width, 1920);
    assert.equal(meta.height, 1080);
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Combination 4: Audio-Only Source Divergence vs Video Source
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outAudioCard = path.join(testDir, 'combo_4_audio_card.mp4');
    const waveArgs = [
      '-y', '-ss', '0.0', '-i', sourceAudio, '-t', '1.0',
      '-filter_complex',
      'color=c=0x080c14:s=1080x1920:d=1.0:r=30[bg];[0:a]asplit[a_wave][a_out];[a_wave]showwaves=s=950x422:mode=cline:colors=0xd4913a:r=30[wave];[bg][wave]overlay=(W-w)/2:(H-h)/2:shortest=1[v]',
      '-map', '[v]', '-map', '[a_out]',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k', '-shortest',
      outAudioCard,
    ];

    const res = spawnSync(ffmpeg, waveArgs, { encoding: 'utf8' });
    assert.equal(res.status, 0, `Combo 4 failed: ${res.stderr}`);

    const meta = probeMedia(outAudioCard);
    assert.equal(meta.hasVideo, true);
    assert.equal(meta.width, 1080);
    assert.equal(meta.height, 1920);
  }
}
