use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dabar_core::{Chapter, Highlight, Sermon, SermonStatus, TranscriptSegment};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

/// Local SQLite database state for the desktop app.
/// Uses SqlitePool (not AnyPool) for simpler, faster queries.
#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

impl Db {
    /// Connect to (or create) the local SQLite database in the app data directory.
    pub async fn connect(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_path)
            .await
            .with_context(|| format!("connecting to SQLite at {db_path}"))?;

        // Run embedded migrations with resilient fallback if dev modified the file
        if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
            tracing::warn!("sqlx migration warning ({e}); applying schema definitions directly...");
            let ddl = include_str!("../migrations/001_initial.sql");
            for statement in ddl.split(';') {
                let stmt = statement.trim();
                if !stmt.is_empty() {
                    let _ = sqlx::query(stmt).execute(&pool).await;
                }
            }
            // Clear stale migration checksum entry so future migrations are clean
            let _ = sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
                .execute(&pool)
                .await;
        }

        // Apply non-breaking table and column additions for existing local sqlite databases
        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS chapters (
                id          TEXT PRIMARY KEY,
                sermon_id   TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
                title       TEXT NOT NULL,
                summary     TEXT NOT NULL DEFAULT '',
                start_time  REAL NOT NULL,
                end_time    REAL NOT NULL
            )",
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_chapters_sermon_id ON chapters(sermon_id, start_time)",
        )
        .execute(&pool)
        .await;
        let _ = sqlx::query("ALTER TABLE sermons ADD COLUMN audio_path TEXT")
            .execute(&pool)
            .await;
        let _ = sqlx::query("ALTER TABLE sermons ADD COLUMN highlight_status TEXT")
            .execute(&pool)
            .await;
        let _ = sqlx::query("ALTER TABLE sermons ADD COLUMN highlight_error TEXT")
            .execute(&pool)
            .await;
        let _ = sqlx::query("ALTER TABLE sermons ADD COLUMN total_candidates INTEGER")
            .execute(&pool)
            .await;
        let _ = sqlx::query("ALTER TABLE sermons ADD COLUMN passed_candidates INTEGER")
            .execute(&pool)
            .await;

        tracing::info!("Database connected and migrations applied: {db_path}");
        Ok(Self { pool })
    }

    // ── Sermon operations ─────────────────────────────────────────────────────

    pub async fn insert_sermon(&self, sermon: &Sermon) -> Result<()> {
        sqlx::query(
            "INSERT INTO sermons (id, title, source_url, source_type, status, created_at, error_message, audio_path, highlight_status, highlight_error, total_candidates, passed_candidates)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(sermon.id.to_string())
        .bind(&sermon.title)
        .bind(&sermon.youtube_url)
        .bind("youtube")
        .bind(status_to_str(&sermon.status))
        .bind(sermon.created_at.to_rfc3339())
        .bind(&sermon.error_message)
        .bind(&sermon.audio_path)
        .bind(&sermon.highlight_status)
        .bind(&sermon.highlight_error)
        .bind(sermon.total_candidates.map(|v| v as i64))
        .bind(sermon.passed_candidates.map(|v| v as i64))
        .execute(&self.pool)
        .await
        .context("inserting sermon")?;
        Ok(())
    }

    pub async fn list_sermons(&self) -> Result<Vec<Sermon>> {
        let rows = sqlx::query(
            "SELECT id, title, source_url, status, created_at, error_message, audio_path, highlight_status, highlight_error, total_candidates, passed_candidates
             FROM sermons ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("listing sermons")?;

        rows.into_iter().map(row_to_sermon).collect()
    }

    pub async fn get_sermon(&self, id: Uuid) -> Result<Option<Sermon>> {
        let row = sqlx::query(
            "SELECT id, title, source_url, status, created_at, error_message, audio_path, highlight_status, highlight_error, total_candidates, passed_candidates
             FROM sermons WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("fetching sermon")?;

        let Some(row) = row else { return Ok(None) };
        let mut sermon = row_to_sermon(row)?;
        sermon.highlights = self.list_highlights(id).await.unwrap_or_default();
        sermon.chapters = self.list_chapters(id).await.unwrap_or_default();
        sermon.transcript_segments = self.list_transcript_segments(id).await.unwrap_or_default();
        Ok(Some(sermon))
    }

    pub async fn list_chapters(&self, sermon_id: Uuid) -> Result<Vec<Chapter>> {
        let rows = sqlx::query(
            "SELECT id, title, summary, start_time, end_time
             FROM chapters WHERE sermon_id = ?
             ORDER BY start_time ASC",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.pool)
        .await
        .context("listing chapters")?;

        rows.into_iter()
            .map(|row| {
                Ok(Chapter {
                    id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    start_time: row.try_get::<f64, _>("start_time")? as f32,
                    end_time: row.try_get::<f64, _>("end_time")? as f32,
                })
            })
            .collect()
    }

    pub async fn get_highlight_with_sermon(
        &self,
        highlight_id: Uuid,
    ) -> Result<Option<(Highlight, Sermon)>> {
        let hl_row = sqlx::query(
            "SELECT id, sermon_id, title, start_time, end_time, score, reason, suggested_hook_text
             FROM highlights WHERE id = ?",
        )
        .bind(highlight_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .context("fetching highlight")?;

        let Some(hl_row) = hl_row else {
            return Ok(None);
        };

        let highlight = Highlight {
            id: Uuid::parse_str(hl_row.try_get::<String, _>("id")?.as_str())?,
            title: hl_row.try_get("title")?,
            start_time: hl_row.try_get::<f64, _>("start_time")? as f32,
            end_time: hl_row.try_get::<f64, _>("end_time")? as f32,
            score: hl_row.try_get::<f64, _>("score")? as f32,
            reason: hl_row.try_get("reason").unwrap_or_default(),
            suggested_hook_text: hl_row.try_get("suggested_hook_text").unwrap_or_default(),
        };

        let sermon_id_str: String = hl_row.try_get("sermon_id")?;
        let sermon_id = Uuid::parse_str(&sermon_id_str)?;
        let sermon = self
            .get_sermon(sermon_id)
            .await?
            .context("parent sermon for highlight missing")?;

        Ok(Some((highlight, sermon)))
    }

    async fn list_highlights(&self, sermon_id: Uuid) -> Result<Vec<Highlight>> {
        let rows = sqlx::query(
            "SELECT id, title, start_time, end_time, score, reason, suggested_hook_text
             FROM highlights WHERE sermon_id = ?
             ORDER BY score DESC, start_time ASC",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.pool)
        .await
        .context("listing highlights")?;

        rows.into_iter()
            .map(|row| {
                Ok(Highlight {
                    id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
                    title: row.try_get("title")?,
                    start_time: row.try_get::<f64, _>("start_time")? as f32,
                    end_time: row.try_get::<f64, _>("end_time")? as f32,
                    score: row.try_get::<f64, _>("score")? as f32,
                    reason: row.try_get("reason").unwrap_or_default(),
                    suggested_hook_text: row.try_get("suggested_hook_text").unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn list_transcript_segments(&self, sermon_id: Uuid) -> Result<Vec<TranscriptSegment>> {
        let rows = sqlx::query(
            "SELECT start_time, end_time, text
             FROM transcript_segments WHERE sermon_id = ?
             ORDER BY ordinal ASC",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.pool)
        .await
        .context("listing transcript segments")?;

        rows.into_iter()
            .map(|row| {
                Ok(TranscriptSegment {
                    start: row.try_get::<f64, _>("start_time")? as f32,
                    end: row.try_get::<f64, _>("end_time")? as f32,
                    text: row.try_get("text")?,
                })
            })
            .collect()
    }

    pub async fn update_status(&self, id: Uuid, status: SermonStatus) -> Result<()> {
        let id_str = id.to_string();
        let status_str = status_to_str(&status);
        sqlx::query("UPDATE sermons SET status = ? WHERE id = ?")
            .bind(status_str)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .with_context(|| format!("updating status for sermon {id} to {status_str}"))?;
        Ok(())
    }

    pub async fn delete_sermon(&self, id: Uuid) -> Result<()> {
        let id_str = id.to_string();
        let _ = sqlx::query("DELETE FROM highlights WHERE sermon_id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("DELETE FROM chapters WHERE sermon_id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("DELETE FROM transcript_segments WHERE sermon_id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("DELETE FROM checkpoints WHERE sermon_id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await;
        sqlx::query("DELETE FROM sermons WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .with_context(|| format!("deleting sermon {id}"))?;
        Ok(())
    }

    pub async fn update_title(&self, id: Uuid, title: &str) -> Result<()> {
        sqlx::query("UPDATE sermons SET title = ? WHERE id = ?")
            .bind(title)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("updating sermon title")?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, error_message: &str) -> Result<()> {
        sqlx::query("UPDATE sermons SET status = 'failed', error_message = ? WHERE id = ?")
            .bind(error_message)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("marking sermon as failed")?;
        Ok(())
    }

    pub async fn save_sermon_results(
        &self,
        id: Uuid,
        title: Option<&str>,
        audio_path: Option<&str>,
        highlights: &[Highlight],
        chapters: &[Chapter],
        segments: &[TranscriptSegment],
        highlight_status: Option<&str>,
        highlight_error: Option<&str>,
        total_candidates: Option<u32>,
        passed_candidates: Option<u32>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.context("beginning transaction")?;

        if let Some(t) = title {
            sqlx::query("UPDATE sermons SET title = ? WHERE id = ?")
                .bind(t)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .context("updating sermon title in transaction")?;
        }

        if let Some(ap) = audio_path {
            sqlx::query("UPDATE sermons SET audio_path = ? WHERE id = ?")
                .bind(ap)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .context("updating sermon audio path in transaction")?;
        }

        sqlx::query(
            "UPDATE sermons SET highlight_status = ?, highlight_error = ?, total_candidates = ?, passed_candidates = ? WHERE id = ?"
        )
        .bind(highlight_status)
        .bind(highlight_error)
        .bind(total_candidates.map(|v| v as i64))
        .bind(passed_candidates.map(|v| v as i64))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .context("updating highlight status in transaction")?;

        insert_transcript_segments_batch(&mut tx, id, segments).await?;

        for hl in highlights {
            sqlx::query(
                "INSERT INTO highlights (id, sermon_id, title, start_time, end_time, score, reason, suggested_hook_text)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(hl.id.to_string())
            .bind(id.to_string())
            .bind(&hl.title)
            .bind(hl.start_time as f64)
            .bind(hl.end_time as f64)
            .bind(hl.score as f64)
            .bind(&hl.reason)
            .bind(&hl.suggested_hook_text)
            .execute(&mut *tx)
            .await
            .context("inserting highlight")?;
        }

        sqlx::query("DELETE FROM chapters WHERE sermon_id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .context("clearing existing chapters")?;

        for chapter in chapters {
            sqlx::query(
                "INSERT INTO chapters (id, sermon_id, title, summary, start_time, end_time)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(chapter.id.to_string())
            .bind(id.to_string())
            .bind(&chapter.title)
            .bind(&chapter.summary)
            .bind(chapter.start_time as f64)
            .bind(chapter.end_time as f64)
            .execute(&mut *tx)
            .await
            .context("inserting chapter")?;
        }

        sqlx::query("UPDATE sermons SET status = 'ready' WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .context("marking sermon as ready")?;

        tx.commit()
            .await
            .context("committing sermon result transaction")?;
        Ok(())
    }

    pub async fn update_highlights(
        &self,
        id: Uuid,
        highlights: &[Highlight],
        status: &str,
        error: Option<&str>,
        total_candidates: Option<u32>,
        passed_candidates: Option<u32>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.context("beginning transaction")?;

        sqlx::query("DELETE FROM highlights WHERE sermon_id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .context("clearing existing highlights")?;

        for hl in highlights {
            sqlx::query(
                "INSERT INTO highlights (id, sermon_id, title, start_time, end_time, score, reason, suggested_hook_text)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(hl.id.to_string())
            .bind(id.to_string())
            .bind(&hl.title)
            .bind(hl.start_time as f64)
            .bind(hl.end_time as f64)
            .bind(hl.score as f64)
            .bind(&hl.reason)
            .bind(&hl.suggested_hook_text)
            .execute(&mut *tx)
            .await
            .context("inserting highlight")?;
        }

        sqlx::query(
            "UPDATE sermons SET highlight_status = ?, highlight_error = ?, total_candidates = ?, passed_candidates = ? WHERE id = ?"
        )
        .bind(status)
        .bind(error)
        .bind(total_candidates.map(|v| v as i64))
        .bind(passed_candidates.map(|v| v as i64))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .context("updating sermon highlight status")?;

        tx.commit().await.context("committing highlight update")?;
        Ok(())
    }

    // ── Settings operations ───────────────────────────────────────────────────

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .context("fetching setting")?;
        Ok(row.and_then(|r| r.try_get::<Option<String>, _>("value").ok().flatten()))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .context("saving setting")?;
        Ok(())
    }

    // ── Pipeline checkpoint operations ────────────────────────────────────────

    pub async fn save_checkpoint(
        &self,
        sermon_id: Uuid,
        stage: &str,
        temp_path: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO pipeline_checkpoints (sermon_id, last_stage, temp_path, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(sermon_id) DO UPDATE SET
               last_stage = excluded.last_stage,
               temp_path = excluded.temp_path,
               updated_at = excluded.updated_at",
        )
        .bind(sermon_id.to_string())
        .bind(stage)
        .bind(temp_path)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .context("saving pipeline checkpoint")?;
        Ok(())
    }

    pub async fn delete_checkpoint(&self, sermon_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM pipeline_checkpoints WHERE sermon_id = ?")
            .bind(sermon_id.to_string())
            .execute(&self.pool)
            .await
            .context("deleting pipeline checkpoint")?;
        Ok(())
    }

    pub async fn list_incomplete_pipelines(&self) -> Result<Vec<(Uuid, String, String)>> {
        let rows = sqlx::query("SELECT sermon_id, last_stage, temp_path FROM pipeline_checkpoints")
            .fetch_all(&self.pool)
            .await
            .context("listing incomplete checkpoints")?;

        rows.into_iter()
            .map(|r| {
                let id_str: String = r.try_get("sermon_id")?;
                let stage: String = r.try_get("last_stage")?;
                let path: String = r.try_get("temp_path")?;
                Ok((Uuid::parse_str(&id_str)?, stage, path))
            })
            .collect()
    }
}

async fn insert_transcript_segments_batch(
    tx: &mut Transaction<'_, Sqlite>,
    sermon_id: Uuid,
    segments: &[TranscriptSegment],
) -> Result<()> {
    const BATCH_SIZE: usize = 100;
    let sermon_id = sermon_id.to_string();

    for (batch_idx, batch) in segments.chunks(BATCH_SIZE).enumerate() {
        let mut query_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "INSERT INTO transcript_segments (id, sermon_id, start_time, end_time, text, ordinal) ",
        );

        query_builder.push_values(batch.iter().enumerate(), |mut row, (offset, seg)| {
            let ordinal = batch_idx * BATCH_SIZE + offset;
            row.push_bind(Uuid::new_v4().to_string())
                .push_bind(&sermon_id)
                .push_bind(seg.start as f64)
                .push_bind(seg.end as f64)
                .push_bind(&seg.text)
                .push_bind(ordinal as i64);
        });

        query_builder
            .build()
            .execute(&mut **tx)
            .await
            .context("batch inserting transcript segments")?;
    }

    Ok(())
}

pub fn row_to_sermon(row: sqlx::sqlite::SqliteRow) -> Result<Sermon> {
    let created_at_str: String = row.try_get("created_at")?;
    let status_str: String = row.try_get("status")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc);

    Ok(Sermon {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        title: row.try_get("title")?,
        youtube_url: row.try_get("source_url")?,
        status: status_from_str(&status_str),
        created_at,
        error_message: row.try_get("error_message")?,
        highlights: Vec::new(),
        chapters: Vec::new(),
        transcript_segments: Vec::new(),
        audio_path: row.try_get("audio_path").ok().flatten(),
        highlight_status: row.try_get("highlight_status").ok().flatten(),
        highlight_error: row.try_get("highlight_error").ok().flatten(),
        total_candidates: row
            .try_get::<Option<i64>, _>("total_candidates")
            .ok()
            .flatten()
            .map(|v| v as u32),
        passed_candidates: row
            .try_get::<Option<i64>, _>("passed_candidates")
            .ok()
            .flatten()
            .map(|v| v as u32),
    })
}

pub fn status_to_str(status: &SermonStatus) -> &'static str {
    match status {
        SermonStatus::Queued => "queued",
        SermonStatus::Downloading => "downloading",
        SermonStatus::Transcribing => "transcribing",
        SermonStatus::Detecting => "detecting",
        SermonStatus::Processing => "processing",
        SermonStatus::Ready => "ready",
        SermonStatus::Failed => "failed",
        SermonStatus::Cancelled => "cancelled",
    }
}

pub fn status_from_str(status: &str) -> SermonStatus {
    match status {
        "downloading" => SermonStatus::Downloading,
        "transcribing" => SermonStatus::Transcribing,
        "detecting" => SermonStatus::Detecting,
        "processing" => SermonStatus::Processing,
        "ready" => SermonStatus::Ready,
        "failed" => SermonStatus::Failed,
        "cancelled" => SermonStatus::Cancelled,
        _ => SermonStatus::Queued,
    }
}
