import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

const TEST_DIR = path.resolve(process.cwd(), 'tmp_test_artifacts');

export function resolveBinary(name) {
  const envVar = `${name.toUpperCase().replace(/-/g, '_')}_PATH`;
  if (process.env[envVar] && fs.existsSync(process.env[envVar])) {
    return process.env[envVar];
  }

  const exeName = process.platform === 'win32' && !name.endsWith('.exe') ? `${name}.exe` : name;

  // 1. Repo bin/
  const repoBin = path.resolve(process.cwd(), 'bin', exeName);
  if (fs.existsSync(repoBin)) {
    return repoBin;
  }

  // 2. AppData
  const appData = process.env.LOCALAPPDATA || process.env.APPDATA;
  if (appData) {
    const appBin = path.resolve(appData, 'com.dabar.app', 'bin', exeName);
    if (fs.existsSync(appBin)) {
      return appBin;
    }
  }

  // 3. Fall back to system PATH
  return name;
}

export function ensureTestDir() {
  if (!fs.existsSync(TEST_DIR)) {
    fs.mkdirSync(TEST_DIR, { recursive: true });
  }
  return TEST_DIR;
}

export function getFontFilePath() {
  if (process.platform === 'win32') {
    const windir = process.env.WINDIR || 'C:\\Windows';
    const arial = path.join(windir, 'Fonts', 'arial.ttf').replace(/\\/g, '/');
    if (fs.existsSync(arial)) {
      return arial.replace(':', '\\:');
    }
  }
  return null;
}

export function buildDrawtextFilter(text, { fontSize = 42, fontColor = 'white', boxColor = 'black@0.75', borderW = 10, yPos = 'h*0.82' } = {}) {
  const fontFile = getFontFilePath();
  const fontArg = fontFile ? `fontfile='${fontFile}':` : '';
  ensureTestDir();
  const hash = Buffer.from(text).toString('hex').slice(0, 16);
  const textFile = path.resolve(TEST_DIR, `sub_${hash}.txt`).replace(/\\/g, '/');
  fs.writeFileSync(textFile, text, 'utf8');
  const escapedTextFile = textFile.replace(':', '\\:');
  return `drawtext=${fontArg}textfile='${escapedTextFile}':fontsize=${fontSize}:fontcolor=${fontColor}:box=1:boxcolor=${boxColor}:boxborderw=${borderW}:x=(w-text_w)/2:y=${yPos}`;
}

export function probeMedia(filePath) {
  const ffprobe = resolveBinary('ffprobe');
  const result = spawnSync(ffprobe, [
    '-v', 'error',
    '-show_entries', 'stream=codec_type,codec_name,width,height,duration,r_frame_rate',
    '-show_entries', 'format=duration,size,bit_rate',
    '-of', 'json',
    filePath,
  ], { encoding: 'utf8' });

  if (result.status !== 0) {
    throw new Error(`ffprobe failed on ${filePath}: ${result.stderr}`);
  }

  const data = JSON.parse(result.stdout);
  const videoStream = data.streams?.find(s => s.codec_type === 'video');
  const audioStream = data.streams?.find(s => s.codec_type === 'audio');
  const duration = parseFloat(data.format?.duration || videoStream?.duration || audioStream?.duration || 0);
  const size = parseInt(data.format?.size || 0, 10);

  return {
    hasVideo: !!videoStream,
    hasAudio: !!audioStream,
    videoCodec: videoStream?.codec_name,
    audioCodec: audioStream?.codec_name,
    width: videoStream?.width,
    height: videoStream?.height,
    duration,
    size,
    raw: data,
  };
}

export function generateSyntheticVideo(fileName, { duration = 20, width = 1920, height = 1080 } = {}) {
  ensureTestDir();
  const outputPath = path.join(TEST_DIR, fileName);
  if (fs.existsSync(outputPath)) {
    return outputPath;
  }

  const ffmpeg = resolveBinary('ffmpeg');
  const isOdd = (width % 2 !== 0) || (height % 2 !== 0);
  const videoCodecArgs = isOdd
    ? ['-c:v', 'mjpeg', '-q:v', '2']
    : ['-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p'];

  const args = [
    '-y',
    '-f', 'lavfi',
    '-i', `testsrc=size=${width}x${height}:rate=30`,
    '-f', 'lavfi',
    '-i', 'sine=frequency=440:sample_rate=48000',
    '-t', `${duration}`,
    ...videoCodecArgs,
    '-c:a', 'aac',
    '-b:a', '128k',
    outputPath,
  ];

  const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
  if (res.status !== 0) {
    throw new Error(`Failed to generate synthetic video ${outputPath}: ${res.stderr}`);
  }

  return outputPath;
}

export function generateSyntheticAudio(fileName, { duration = 20 } = {}) {
  ensureTestDir();
  const outputPath = path.join(TEST_DIR, fileName);
  if (fs.existsSync(outputPath)) {
    return outputPath;
  }

  const ffmpeg = resolveBinary('ffmpeg');
  const args = [
    '-y',
    '-f', 'lavfi',
    '-i', 'sine=frequency=440:sample_rate=48000',
    '-t', `${duration}`,
    '-c:a', 'libmp3lame',
    '-b:a', '128k',
    outputPath,
  ];

  const res = spawnSync(ffmpeg, args, { encoding: 'utf8' });
  if (res.status !== 0) {
    throw new Error(`Failed to generate synthetic audio ${outputPath}: ${res.stderr}`);
  }

  return outputPath;
}

export function cleanupTestArtifacts() {
  if (process.env.DABAR_TEST_KEEP_ARTIFACTS === 'true') {
    return;
  }
  if (fs.existsSync(TEST_DIR)) {
    try {
      fs.rmSync(TEST_DIR, { recursive: true, force: true });
    } catch {
      // Ignore directory cleanup locks on Windows
    }
  }
}
