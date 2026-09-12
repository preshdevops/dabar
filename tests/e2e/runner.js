#!/usr/bin/env node

import path from 'node:path';
import { cleanupTestArtifacts } from './fixtures/media_generator.js';

// Import Tier 1 suites
import * as t1Video from './tier1_features/test_video_rendering.js';
import * as t1Subtitles from './tier1_features/test_subtitles_engine.js';
import * as t1Tools from './tier1_features/test_tool_discovery.js';
import * as t1Llm from './tier1_features/test_llm_pipeline.js';
import * as t1Chunking from './tier1_features/test_chunking_concurrency.js';
import * as t1Sqlite from './tier1_features/test_sqlite_batch.js';
import * as t1Ergonomics from './tier1_features/test_clip_ergonomics.js';
import * as t1ExportModal from './tier1_features/test_export_modal.js';

// Import Tier 2 suites
import * as t2Durations from './tier2_boundaries/test_durations.js';
import * as t2OddDim from './tier2_boundaries/test_odd_dimensions.js';
import * as t2SubEscaping from './tier2_boundaries/test_subtitle_escaping.js';
import * as t2Resilience from './tier2_boundaries/test_resilience.js';

// Import Tier 3 suites
import * as t3Matrix from './tier3_combinations/test_matrix.js';

// Import Tier 4 suites
import * as t4Sunday from './tier4_scenarios/test_sunday_service.js';
import * as t4BibleStudy from './tier4_scenarios/test_bible_study.js';
import * as t4YouTube from './tier4_scenarios/test_youtube_clipping.js';

const SUITES = [
  // Tier 1
  { tier: 1, ...t1Video },
  { tier: 1, ...t1Subtitles },
  { tier: 1, ...t1Tools },
  { tier: 1, ...t1Llm },
  { tier: 1, ...t1Chunking },
  { tier: 1, ...t1Sqlite },
  { tier: 1, ...t1Ergonomics },
  { tier: 1, ...t1ExportModal },

  // Tier 2
  { tier: 2, ...t2Durations },
  { tier: 2, ...t2OddDim },
  { tier: 2, ...t2SubEscaping },
  { tier: 2, ...t2Resilience },

  // Tier 3
  { tier: 3, ...t3Matrix },

  // Tier 4
  { tier: 4, ...t4Sunday },
  { tier: 4, ...t4BibleStudy },
  { tier: 4, ...t4YouTube },
];

// ANSI colors
const c = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  gray: '\x1b[90m',
};

async function main() {
  const args = process.argv.slice(2);
  let requestedTier = null;
  let filterPattern = null;

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--tier' && args[i + 1]) {
      requestedTier = parseInt(args[i + 1], 10);
      i++;
    } else if (args[i] === '--filter' && args[i + 1]) {
      filterPattern = args[i + 1].toLowerCase();
      i++;
    }
  }

  console.log(`\n${c.bold}${c.cyan}======================================================${c.reset}`);
  console.log(`${c.bold}${c.cyan}       DABAR AUTOMATED E2E TEST SUITE RUNNER          ${c.reset}`);
  console.log(`${c.bold}${c.cyan}======================================================${c.reset}\n`);

  let targetSuites = SUITES;
  if (requestedTier !== null) {
    targetSuites = targetSuites.filter(s => s.tier === requestedTier);
    console.log(`${c.yellow}Filtering to Tier ${requestedTier} suites only.${c.reset}\n`);
  }
  if (filterPattern) {
    targetSuites = targetSuites.filter(s => s.name.toLowerCase().includes(filterPattern));
    console.log(`${c.yellow}Filtering suites matching: "${filterPattern}"${c.reset}\n`);
  }

  const results = [];
  const globalStart = Date.now();

  for (let idx = 0; idx < targetSuites.length; idx++) {
    const suite = targetSuites[idx];
    const prefix = `[${idx + 1}/${targetSuites.length}] [Tier ${suite.tier}]`;
    process.stdout.write(`${c.gray}${prefix}${c.reset} ${suite.name} ... `);

    const start = Date.now();
    try {
      await suite.run();
      const duration = ((Date.now() - start) / 1000).toFixed(2);
      console.log(`${c.green}${c.bold}PASS${c.reset} ${c.gray}(${duration}s)${c.reset}`);
      results.push({ name: suite.name, tier: suite.tier, pass: true, duration });
    } catch (err) {
      const duration = ((Date.now() - start) / 1000).toFixed(2);
      console.log(`${c.red}${c.bold}FAIL${c.reset} ${c.gray}(${duration}s)${c.reset}`);
      console.error(`\n${c.red}  Error details: ${err.message}${c.reset}`);
      if (err.stack) {
        console.error(`${c.gray}${err.stack.split('\n').slice(1, 4).join('\n')}${c.reset}\n`);
      }
      results.push({ name: suite.name, tier: suite.tier, pass: false, error: err.message, duration });
    }
  }

  const totalTime = ((Date.now() - globalStart) / 1000).toFixed(2);
  const passedCount = results.filter(r => r.pass).length;
  const failedCount = results.filter(r => !r.pass).length;

  console.log(`\n${c.bold}------------------------------------------------------${c.reset}`);
  console.log(`${c.bold}TEST EXECUTION SUMMARY${c.reset}`);
  console.log(`${c.bold}------------------------------------------------------${c.reset}`);
  console.log(`Total Suites : ${results.length}`);
  console.log(`Passed       : ${c.green}${passedCount}${c.reset}`);
  console.log(`Failed       : ${failedCount > 0 ? c.red : c.gray}${failedCount}${c.reset}`);
  console.log(`Duration     : ${totalTime}s`);
  console.log(`${c.bold}------------------------------------------------------${c.reset}\n`);

  // Cleanup test video/audio artifacts unless disabled
  cleanupTestArtifacts();

  if (failedCount > 0) {
    console.log(`${c.red}${c.bold}FAILED: ${failedCount} test suite(s) encountered errors.${c.reset}\n`);
    process.exit(1);
  } else {
    console.log(`${c.green}${c.bold}SUCCESS: All ${passedCount} test suite(s) passed successfully.${c.reset}\n`);
    process.exit(0);
  }
}

main().catch(err => {
  console.error('Fatal test runner error:', err);
  process.exit(1);
});
