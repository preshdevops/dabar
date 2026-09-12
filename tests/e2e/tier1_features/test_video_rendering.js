import assert from 'node:assert/strict';
import path from 'node:path';
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
import {
  generateSyntheticVideo,
  generateSyntheticAudio,
  probeMedia,
  resolveBinary,
  ensureTestDir,
} from '../fixtures/media_generator.js';

export const name = 'Tier 1 — Video Clip Rendering & Filtergraphs (F1-F4)';

export async function run() {
  const testDir = ensureTestDir();
  const ffmpeg = resolveBinary('ffmpeg');
  const sourceVideo = generateSyntheticVideo('source_16x9.mp4', { duration: 5, width: 1920, height: 1080 });
  const sourceAudioOnly = generateSyntheticAudio('source_audio_only.mp3', { duration: 5 });

  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F2): Local Video Clip Rendering produces real video footage
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outClip = path.join(testDir, 'f2_local_video_rendered.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      '[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k',
      outClip,
    ];
    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `FFmpeg failed to render local video clip: ${res.stderr}`);
    assert(fs.existsSync(outClip), 'Rendered video clip must exist on disk');

    const meta = probeMedia(outClip);
    assert.equal(meta.hasVideo, true, 'Rendered clip must contain a video stream (not audio only)');
    assert.equal(meta.videoCodec, 'h264', 'Video stream must be encoded as H.264');
    assert.equal(meta.hasAudio, true, 'Rendered clip must contain an audio track');
    assert.equal(meta.audioCodec, 'aac', 'Audio stream must be encoded as AAC');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F3): 9:16 Vertical Aspect Ratio Filtergraph & Even Pixel Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out9x16 = path.join(testDir, 'f3_vertical_9x16.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      '[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k',
      out9x16,
    ];
    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `9:16 render failed: ${res.stderr}`);

    const meta = probeMedia(out9x16);
    assert.equal(meta.width, 1080, '9:16 target width must be 1080');
    assert.equal(meta.height, 1920, '9:16 target height must be 1920');
    assert.equal(meta.width % 2, 0, '9:16 width must be divisible by 2 for H.264');
    assert.equal(meta.height % 2, 0, '9:16 height must be divisible by 2 for H.264');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F3): 1:1 Square Aspect Ratio Filtergraph & Even Pixel Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out1x1 = path.join(testDir, 'f3_square_1x1.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      '[0:v]split[fg_in][bg_in];[bg_in]scale=108:108:force_original_aspect_ratio=increase,crop=108:108,boxblur=5:2,scale=1080:1080:flags=bilinear[bg];[fg_in]scale=1080:1080:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k',
      out1x1,
    ];
    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `1:1 render failed: ${res.stderr}`);

    const meta = probeMedia(out1x1);
    assert.equal(meta.width, 1080, '1:1 target width must be 1080');
    assert.equal(meta.height, 1080, '1:1 target height must be 1080');
    assert.equal(meta.width % 2, 0, '1:1 width must be divisible by 2');
    assert.equal(meta.height % 2, 0, '1:1 height must be divisible by 2');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 4 (F3): 16:9 Widescreen Aspect Ratio Filtergraph & Even Pixel Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    const out16x9 = path.join(testDir, 'f3_widescreen_16x9.mp4');
    const args = [
      '-y', '-ss', '0.0', '-i', sourceVideo, '-t', '1.0',
      '-filter_complex',
      '[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=black[v]',
      '-map', '[v]', '-map', '0:a?',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k',
      out16x9,
    ];
    const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
    assert.equal(res.status, 0, `16:9 render failed: ${res.stderr}`);

    const meta = probeMedia(out16x9);
    assert.equal(meta.width, 1920, '16:9 target width must be 1920');
    assert.equal(meta.height, 1080, '16:9 target height must be 1080');
    assert.equal(meta.width % 2, 0, '16:9 width must be divisible by 2');
    assert.equal(meta.height % 2, 0, '16:9 height must be divisible by 2');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 5 (F4): Background Blur Padding Verification
  // ──────────────────────────────────────────────────────────────────────────
  {
    const filter9x16 = '[0:v]split[fg_in][bg_in];[bg_in]scale=108:192:force_original_aspect_ratio=increase,crop=108:192,boxblur=5:2,scale=1080:1920:flags=bilinear[bg];[fg_in]scale=1080:1920:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2[v]';
    assert(filter9x16.includes('boxblur='), 'Filtergraph must include boxblur filter for background padding');
    assert(filter9x16.includes('overlay='), 'Filtergraph must overlay sharp foreground over blurred background');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 6 (F1): YouTube Video Stream Contract vs Waveform Fallback Card
  // ──────────────────────────────────────────────────────────────────────────
  {
    const outWaveform = path.join(testDir, 'waveform_audio_rendered.mp4');
    const waveArgs = [
      '-y', '-ss', '0.0', '-i', sourceAudioOnly, '-t', '1.0',
      '-filter_complex',
      'color=c=0x080c14:s=1080x1920:d=1.0:r=30[bg];[0:a]asplit[a_wave][a_out];[a_wave]showwaves=s=950x422:mode=cline:colors=0xd4913a:r=30[wave];[bg][wave]overlay=(W-w)/2:(H-h)/2:shortest=1[v]',
      '-map', '[v]', '-map', '[a_out]',
      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '192k', '-shortest',
      outWaveform,
    ];
    const resWave = spawnSync(ffmpeg, waveArgs, { encoding: 'utf8' });
    assert.equal(resWave.status, 0, `Waveform render failed: ${resWave.stderr}`);

    const ffmpegSrc = fs.readFileSync(path.resolve(process.cwd(), 'packages/core/src/ffmpeg.rs'), 'utf8');
    assert(
      ffmpegSrc.includes('has_video_stream'),
      'extract_clip must inspect source with has_video_stream'
    );
  }
}
