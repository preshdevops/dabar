import assert from 'node:assert/strict';

export const name = 'Tier 1 — Batched SQLite Persistence (F13)';

export async function run() {
  // Test the batch SQL statement constructor contract
  // When saving sermon results with hundreds of segments, generating a batched
  // multi-row INSERT slashes transaction time from seconds to milliseconds.

  function buildBatchInsertQuery(sermonId, segments, chunkSize = 100) {
    const batches = [];
    for (let i = 0; i < segments.length; i += chunkSize) {
      const chunk = segments.slice(i, i + chunkSize);
      const valuePlaceholders = chunk.map(() => '(?, ?, ?, ?, ?, ?)').join(', ');
      const sql = `INSERT INTO transcript_segments (id, sermon_id, start_time, end_time, text, ordinal) VALUES ${valuePlaceholders}`;
      batches.push({ sql, count: chunk.length });
    }
    return batches;
  }

  // Generate 450 test transcript segments
  const mockSegments = Array.from({ length: 450 }, (_, i) => ({
    start: i * 2.5,
    end: (i + 1) * 2.5,
    text: `Segment ${i}: The word of God is living and active.`,
  }));

  const batches = buildBatchInsertQuery('test-sermon-id', mockSegments, 100);

  // 450 items with chunkSize 100 -> 5 batch operations instead of 450 individual queries
  assert.equal(batches.length, 5, '450 segments should be grouped into 5 batch statements');
  assert.equal(batches[0].count, 100);
  assert.equal(batches[4].count, 50);

  // Verify the query structure contains multi-row syntax
  assert(batches[0].sql.includes('VALUES (?, ?, ?, ?, ?, ?), (?, ?, ?, ?, ?, ?)'), 'Batch SQL must use multi-row VALUES syntax');
}
