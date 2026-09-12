import assert from 'node:assert/strict';

export const name = 'Tier 1 — Clip Selection, In/Out Marking & Boundary Nudging (F14-F17)';

// Nudge boundary contract helper
export function nudgeBoundary(bounds, boundary, deltaSeconds, maxDuration = 7200) {
  let { start, end } = bounds;
  const MIN_CLIP_DURATION = 10.0;

  if (boundary === 'start') {
    const newStart = Math.max(0, start + deltaSeconds);
    if (end - newStart >= MIN_CLIP_DURATION) {
      start = Number(newStart.toFixed(2));
    }
  } else if (boundary === 'end') {
    const newEnd = Math.min(maxDuration, end + deltaSeconds);
    if (newEnd - start >= MIN_CLIP_DURATION) {
      end = Number(newEnd.toFixed(2));
    }
  }

  return { start, end, duration: Number((end - start).toFixed(2)) };
}

export async function run() {
  // ──────────────────────────────────────────────────────────────────────────
  // Test 1 (F14): Arbitrary Duration Selection (10s to 300s+)
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Verify valid durations: 10s short snippet, 47s custom, 300s (5min) altar call
    const testCases = [
      { start: 0, end: 10, valid: true },
      { start: 10, end: 57, valid: true },
      { start: 100, end: 400, valid: true }, // 300s (5m)
      { start: 50, end: 55, valid: false }, // 5s < 10s minimum
      { start: 20, end: 20, valid: false }, // 0s
      { start: 30, end: 15, valid: false }, // inverted
    ];

    function validateClipBounds(start, end) {
      if (start < 0 || end <= start) return false;
      const duration = end - start;
      return duration >= 10.0; // 10s minimum, arbitrary upper bound
    }

    for (const tc of testCases) {
      assert.equal(
        validateClipBounds(tc.start, tc.end),
        tc.valid,
        `Bounds [${tc.start}, ${tc.end}] validity should be ${tc.valid}`
      );
    }
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 2 (F15): Interactive In / Out Marking Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    class MarkRangeState {
      constructor() {
        this.markIn = null;
        this.markOut = null;
      }

      setIn(timestamp) {
        this.markIn = timestamp;
        if (this.markOut !== null && this.markOut <= this.markIn) {
          this.markOut = null; // reset out if before in
        }
      }

      setOut(timestamp) {
        if (this.markIn !== null && timestamp > this.markIn) {
          this.markOut = timestamp;
        }
      }

      getRange() {
        if (this.markIn !== null && this.markOut !== null) {
          return {
            start: this.markIn,
            end: this.markOut,
            duration: this.markOut - this.markIn,
          };
        }
        return null;
      }
    }

    const state = new MarkRangeState();
    state.setIn(15.5);
    state.setOut(45.5);
    const range = state.getRange();
    assert.deepEqual(range, { start: 15.5, end: 45.5, duration: 30.0 });

    // Mark Out before Mark In should be rejected
    const invalidState = new MarkRangeState();
    invalidState.setIn(50.0);
    invalidState.setOut(20.0);
    assert.equal(invalidState.getRange(), null, 'Mark Out before Mark In must not create a valid range');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 3 (F16): ±1s Boundary Nudging on Clip Bounds
  // ──────────────────────────────────────────────────────────────────────────
  {
    let bounds = { start: 10.0, end: 40.0 };

    // Expand start by -1s (starts earlier)
    bounds = nudgeBoundary(bounds, 'start', -1.0);
    assert.equal(bounds.start, 9.0);
    assert.equal(bounds.duration, 31.0);

    // Trim start by +1s (starts later)
    bounds = nudgeBoundary(bounds, 'start', 1.0);
    assert.equal(bounds.start, 10.0);
    assert.equal(bounds.duration, 30.0);

    // Expand end by +1s (ends later)
    bounds = nudgeBoundary(bounds, 'end', 1.0);
    assert.equal(bounds.end, 41.0);
    assert.equal(bounds.duration, 31.0);

    // Trim end by -1s (ends earlier)
    bounds = nudgeBoundary(bounds, 'end', -1.0);
    assert.equal(bounds.end, 40.0);
    assert.equal(bounds.duration, 30.0);

    // Cannot nudge start below 0
    let zeroBounds = { start: 0.5, end: 20.0 };
    zeroBounds = nudgeBoundary(zeroBounds, 'start', -1.0);
    assert.equal(zeroBounds.start, 0.0, 'Start boundary must clamp at 0');

    // Cannot nudge duration below 10s
    let minBounds = { start: 10.0, end: 20.0 };
    minBounds = nudgeBoundary(minBounds, 'start', 1.0);
    assert.equal(minBounds.start, 10.0, 'Cannot nudge start forward if remaining duration would be < 10s');
  }

  // ──────────────────────────────────────────────────────────────────────────
  // Test 4 (F17): Backend Nudged Bounds Honor Contract
  // ──────────────────────────────────────────────────────────────────────────
  {
    // Contract: when render_clip is called with both highlight_id and (start_time, end_time),
    // the backend MUST honor the passed start_time and end_time (the nudged bounds)
    // rather than using the original highlight timestamps from the database.

    function resolveEffectiveClipBounds(highlightFromDb, passedStart, passedEnd) {
      if (passedStart !== null && passedStart !== undefined && passedEnd !== null && passedEnd !== undefined) {
        return { start: passedStart, end: passedEnd, source: 'nudged_user_input' };
      }
      if (highlightFromDb) {
        return { start: highlightFromDb.start_time, end: highlightFromDb.end_time, source: 'database_highlight' };
      }
      throw new Error('No valid bounds');
    }

    const dbHighlight = { start_time: 20.0, end_time: 50.0 };
    const resolvedNudged = resolveEffectiveClipBounds(dbHighlight, 18.0, 52.0);
    assert.equal(resolvedNudged.start, 18.0, 'Must honor nudged start_time');
    assert.equal(resolvedNudged.end, 52.0, 'Must honor nudged end_time');
    assert.equal(resolvedNudged.source, 'nudged_user_input');

    const resolvedDefault = resolveEffectiveClipBounds(dbHighlight, null, null);
    assert.equal(resolvedDefault.start, 20.0, 'Falls back to DB highlight start if not nudged');
    assert.equal(resolvedDefault.end, 50.0, 'Falls back to DB highlight end if not nudged');
  }
}
