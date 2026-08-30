use anyhow::{Context, Result};
use dabar_core::{Highlight, Sermon, SermonStatus, TranscriptSegment};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

impl Db {
    pub async fn connect(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_path)
            .await
            .with_context(|| format!("connecting to SQLite at {db_path}"))?;
        Self::migrate(&pool).await?;
        Ok(Self { pool })
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sermons (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                source_url TEXT NOT NULL,
                source_type TEXT NOT NULL DEFAULT 'local',
                status TEXT NOT NULL DEFAULT 'queued',
                created_at TEXT NOT NULL,
                error_message TEXT,
                audio_path TEXT,
                highlight_status TEXT,
                highlight_error TEXT,
                total_candidates INTEGER,
                passed_candidates INTEGER
            )",
        )
        .execute(pool)
        .await
        .context("creating sermons table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS highlights (
                id TEXT PRIMARY KEY,
                sermon_id TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                start_time REAL NOT NULL,
                end_time REAL NOT NULL,
                score REAL NOT NULL DEFAULT 8.0,
                reason TEXT NOT NULL DEFAULT '',
                suggested_hook_text TEXT NOT NULL DEFAULT ''
            )",
        )
        .execute(pool)
        .await
        .context("creating highlights table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS chapters (
                id TEXT PRIMARY KEY,
                sermon_id TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                start_time REAL NOT NULL,
                end_time REAL NOT NULL
            )",
        )
        .execute(pool)
        .await
        .context("creating chapters table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS transcript_segments (
                id TEXT PRIMARY KEY,
                sermon_id TEXT NOT NULL REFERENCES sermons(id) ON DELETE CASCADE,
                start_time REAL NOT NULL,
                end_time REAL NOT NULL,
                text TEXT NOT NULL,
                ordinal INTEGER NOT NULL
            )",
        )
        .execute(pool)
        .await
        .context("creating transcript_segments table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
        )
        .execute(pool)
        .await
        .context("creating settings table")?;

        Ok(())
    }

    pub async fn insert_sermon(&self, sermon: &Sermon) -> Result<()> {
        sqlx::query(
            "INSERT INTO sermons (id, title, source_url, source_type, status, created_at) VALUES (?, ?, ?, 'local', ?, ?)",
        )
        .bind(sermon.id.to_string())
        .bind(&sermon.title)
        .bind(&sermon.youtube_url)
        .bind(status_to_str(&sermon.status))
        .bind(sermon.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("inserting sermon")?;
        Ok(())
    }

    pub async fn update_status(&self, id: Uuid, status: SermonStatus) -> Result<()> {
        sqlx::query("UPDATE sermons SET status = ? WHERE id = ?")
            .bind(status_to_str(&status))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("updating sermon status")?;
        Ok(())
    }

    pub async fn save_results(
        &self,
        sermon_id: Uuid,
        audio_path: Option<&str>,
        segments: &[TranscriptSegment],
        highlights: &[Highlight],
        chapters: &[dabar_core::Chapter],
    ) -> Result<()> {
        sqlx::query("UPDATE sermons SET status = 'ready', audio_path = ? WHERE id = ?")
            .bind(audio_path)
            .bind(sermon_id.to_string())
            .execute(&self.pool)
            .await?;

        for (i, seg) in segments.iter().enumerate() {
            sqlx::query(
                "INSERT OR REPLACE INTO transcript_segments (id, sermon_id, start_time, end_time, text, ordinal) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(sermon_id.to_string())
            .bind(seg.start as f64)
            .bind(seg.end as f64)
            .bind(&seg.text)
            .bind(i as i64)
            .execute(&self.pool)
            .await?;
        }

        for h in highlights {
            sqlx::query(
                "INSERT OR REPLACE INTO highlights (id, sermon_id, title, start_time, end_time, score, reason, suggested_hook_text) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(h.id.to_string())
            .bind(sermon_id.to_string())
            .bind(&h.title)
            .bind(h.start_time as f64)
            .bind(h.end_time as f64)
            .bind(h.score as f64)
            .bind(&h.reason)
            .bind(&h.suggested_hook_text)
            .execute(&self.pool)
            .await?;
        }

        for ch in chapters {
            sqlx::query(
                "INSERT OR REPLACE INTO chapters (id, sermon_id, title, summary, start_time, end_time) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(ch.id.to_string())
            .bind(sermon_id.to_string())
            .bind(&ch.title)
            .bind(&ch.summary)
            .bind(ch.start_time as f64)
            .bind(ch.end_time as f64)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn list_sermons(&self) -> Result<Vec<SermonSummary>> {
        let rows = sqlx::query(
            "SELECT id, title, status, created_at, audio_path FROM sermons ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("listing sermons")?;

        Ok(rows
            .iter()
            .map(|r| SermonSummary {
                id: r.get::<String, _>("id"),
                title: r.get::<String, _>("title"),
                status: r.get::<String, _>("status"),
                created_at: r.get::<String, _>("created_at"),
                audio_path: r.try_get::<String, _>("audio_path").ok(),
            })
            .collect())
    }

    pub async fn get_sermon_highlights(&self, sermon_id: Uuid) -> Result<Vec<Highlight>> {
        let rows = sqlx::query(
            "SELECT id, title, start_time, end_time, score, reason, suggested_hook_text FROM highlights WHERE sermon_id = ? ORDER BY start_time",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| Highlight {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_else(|_| Uuid::new_v4()),
                title: r.get("title"),
                start_time: r.get::<f64, _>("start_time") as f32,
                end_time: r.get::<f64, _>("end_time") as f32,
                score: r.get::<f64, _>("score") as f32,
                reason: r.get("reason"),
                suggested_hook_text: r.get("suggested_hook_text"),
            })
            .collect())
    }

    pub async fn get_sermon_segments(&self, sermon_id: Uuid) -> Result<Vec<TranscriptSegment>> {
        let rows = sqlx::query(
            "SELECT start_time, end_time, text FROM transcript_segments WHERE sermon_id = ? ORDER BY ordinal",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| TranscriptSegment {
                start: r.get::<f64, _>("start_time") as f32,
                end: r.get::<f64, _>("end_time") as f32,
                text: r.get("text"),
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct SermonSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub audio_path: Option<String>,
}

fn status_to_str(status: &SermonStatus) -> &'static str {
    match status {
        SermonStatus::Queued => "queued",
        SermonStatus::Downloading => "downloading",
        SermonStatus::Transcribing => "transcribing",
        SermonStatus::Detecting => "detecting",
        SermonStatus::Processing => "processing",
        SermonStatus::Ready => "ready",
        SermonStatus::Failed => "failed",
    }
}
