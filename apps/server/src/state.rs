use anyhow::Context;
use chrono::{DateTime, Utc};
use dabar_core::{Highlight, Sermon, SermonStatus, TranscriptSegment};
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    db: AnyPool,
}

impl AppState {
    pub async fn connect() -> anyhow::Result<Self> {
        sqlx::any::install_default_drivers();

        let database_url = database_url();
        let db = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .context("connecting to database")?;

        sqlx::migrate!("./migrations")
            .run(&db)
            .await
            .context("running database migrations")?;

        Ok(Self { db })
    }

    pub async fn insert_sermon(&self, sermon: Sermon) -> anyhow::Result<Sermon> {
        sqlx::query(
            "INSERT INTO sermons (id, title, youtube_url, status, created_at, error_message)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(sermon.id.to_string())
        .bind(&sermon.title)
        .bind(&sermon.youtube_url)
        .bind(status_to_str(&sermon.status))
        .bind(sermon.created_at.to_rfc3339())
        .bind(&sermon.error_message)
        .execute(&self.db)
        .await
        .context("inserting sermon")?;

        Ok(sermon)
    }

    pub async fn list_sermons(&self) -> anyhow::Result<Vec<Sermon>> {
        let rows = sqlx::query(
            "SELECT id, title, youtube_url, status, created_at, error_message
             FROM sermons
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.db)
        .await
        .context("listing sermons")?;

        rows.into_iter().map(row_to_sermon_summary).collect()
    }

    pub async fn get_sermon(&self, id: Uuid) -> anyhow::Result<Option<Sermon>> {
        let row = sqlx::query(
            "SELECT id, title, youtube_url, status, created_at, error_message
             FROM sermons
             WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.db)
        .await
        .context("fetching sermon")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut sermon = row_to_sermon_summary(row)?;
        sermon.highlights = self.list_highlights(id).await?;
        sermon.transcript_segments = self.list_transcript_segments(id).await?;
        Ok(Some(sermon))
    }

    pub async fn get_highlight_with_sermon(
        &self,
        highlight_id: Uuid,
    ) -> anyhow::Result<Option<(Highlight, Sermon)>> {
        let hl_row = sqlx::query(
            "SELECT id, sermon_id, title, start_time, end_time, score, reason, suggested_hook_text
             FROM highlights
             WHERE id = ?",
        )
        .bind(highlight_id.to_string())
        .fetch_optional(&self.db)
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

    async fn list_highlights(&self, sermon_id: Uuid) -> anyhow::Result<Vec<Highlight>> {
        let rows = sqlx::query(
            "SELECT id, title, start_time, end_time, score, reason, suggested_hook_text
             FROM highlights
             WHERE sermon_id = ?
             ORDER BY score DESC, start_time ASC",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.db)
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

    async fn list_transcript_segments(
        &self,
        sermon_id: Uuid,
    ) -> anyhow::Result<Vec<TranscriptSegment>> {
        let rows = sqlx::query(
            "SELECT start_time, end_time, text
             FROM transcript_segments
             WHERE sermon_id = ?
             ORDER BY ordinal ASC",
        )
        .bind(sermon_id.to_string())
        .fetch_all(&self.db)
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

    pub async fn update_status(&self, id: Uuid, status: SermonStatus) -> anyhow::Result<()> {
        sqlx::query("UPDATE sermons SET status = ? WHERE id = ?")
            .bind(status_to_str(&status))
            .bind(id.to_string())
            .execute(&self.db)
            .await
            .context("updating sermon status")?;
        Ok(())
    }

    pub async fn update_title(&self, id: Uuid, title: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE sermons SET title = ? WHERE id = ?")
            .bind(title)
            .bind(id.to_string())
            .execute(&self.db)
            .await
            .context("updating sermon title")?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid, error_message: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE sermons SET status = ?, error_message = ? WHERE id = ?")
            .bind("failed")
            .bind(error_message)
            .bind(id.to_string())
            .execute(&self.db)
            .await
            .context("marking sermon as failed")?;
        Ok(())
    }

    pub async fn save_sermon_results(
        &self,
        id: Uuid,
        title: Option<&str>,
        highlights: &[Highlight],
        segments: &[TranscriptSegment],
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin().await.context("beginning transaction")?;

        if let Some(t) = title {
            sqlx::query("UPDATE sermons SET title = ? WHERE id = ?")
                .bind(t)
                .bind(id.to_string())
                .execute(&mut *tx)
                .await
                .context("updating sermon title in transaction")?;
        }

        for (ordinal, seg) in segments.iter().enumerate() {
            sqlx::query(
                "INSERT INTO transcript_segments (id, sermon_id, start_time, end_time, text, ordinal)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(id.to_string())
            .bind(seg.start as f64)
            .bind(seg.end as f64)
            .bind(&seg.text)
            .bind(ordinal as i64)
            .execute(&mut *tx)
            .await
            .context("inserting transcript segment")?;
        }

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

        sqlx::query("UPDATE sermons SET status = ? WHERE id = ?")
            .bind("ready")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .context("marking sermon as ready")?;

        tx.commit()
            .await
            .context("committing sermon result transaction")?;
        Ok(())
    }
}

fn database_url() -> String {
    let url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://dabar.sqlite3?mode=rwc".into());

    if url.starts_with("sqlite:") {
        let mut formatted = if !url.contains('?') {
            format!("{url}?mode=rwc")
        } else {
            url
        };

        // If invoked from apps/server, anchor relative path to workspace root
        if (formatted.starts_with("sqlite://dabar.sqlite3")
            || formatted.starts_with("sqlite:dabar.sqlite3"))
            && std::path::Path::new("../../Cargo.toml").exists()
        {
            formatted = formatted.replace("dabar.sqlite3", "../../dabar.sqlite3");
        }

        formatted
    } else {
        url
    }
}

fn row_to_sermon_summary(row: sqlx::any::AnyRow) -> anyhow::Result<Sermon> {
    let status = row.try_get::<String, _>("status")?;
    let created_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc);

    Ok(Sermon {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        title: row.try_get("title")?,
        youtube_url: row.try_get("youtube_url")?,
        status: status_from_str(&status),
        created_at,
        error_message: row.try_get("error_message")?,
        highlights: Vec::new(),
        chapters: Vec::new(),
        transcript_segments: Vec::new(),
        audio_path: None,
        highlight_status: None,
        highlight_error: None,
        total_candidates: None,
        passed_candidates: None,
    })
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
        SermonStatus::Cancelled => "cancelled",
    }
}

fn status_from_str(status: &str) -> SermonStatus {
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
