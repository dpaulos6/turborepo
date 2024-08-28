use camino::Utf8Path;
use sqlx::{Pool, Sqlite, SqlitePool};
use thiserror::Error;
use turbopath::{AbsoluteSystemPath, AbsoluteSystemPathBuf};
use turborepo_api_client::spaces::{CreateSpaceRunPayload, RunStatus, SpaceTaskSummary};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to connect to database: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("failed to migrate database: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("failed to serialize")]
    Serialize(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct DatabaseHandle {
    pool: Pool<Sqlite>,
}

impl DatabaseHandle {
    pub async fn new(cache_dir: &Utf8Path, repo_root: &AbsoluteSystemPath) -> Result<Self, Error> {
        let cache_dir = AbsoluteSystemPathBuf::from_unknown(&repo_root, &cache_dir);
        let pool = SqlitePool::connect(&format!(
            "sqlite://{}?mode=rwc",
            cache_dir.join_component("turbo.db")
        ))
        .await?;

        sqlx::migrate!().run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn create_run(&self, payload: &CreateSpaceRunPayload) -> Result<Uuid, Error> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO runs (
              id,
              start_time,
              status,
              command,
              package_inference_root,
              context,
              git_branch,
              git_sha,
              origination_user,
              client_id,
              client_name,
              client_version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(id.to_string())
        .bind(payload.start_time)
        .bind(payload.status.as_ref())
        .bind(&payload.command)
        .bind(&payload.package_inference_root)
        .bind(payload.run_context)
        .bind(&payload.git_branch)
        .bind(&payload.git_sha)
        .bind(&payload.user)
        .bind(payload.client.id)
        .bind(payload.client.name)
        .bind(&payload.client.version)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn finish_run(&self, id: Uuid, end_time: i64, exit_code: i32) -> Result<(), Error> {
        sqlx::query("UPDATE runs SET status = $1, end_time = $2, exit_code = $3 WHERE id = $4")
            .bind(RunStatus::Completed.as_ref())
            .bind(end_time)
            .bind(exit_code)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn finish_task(&self, id: Uuid, summary: &SpaceTaskSummary) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO tasks (
              run_id,
              name,
              package,
              hash,
              start_time,
              end_time,
              cache_status,
              exit_code,
              logs
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id.to_string())
        .bind(&summary.name)
        .bind(&summary.workspace)
        .bind(&summary.hash)
        .bind(summary.start_time)
        .bind(summary.end_time)
        .bind(serde_json::to_string(&summary.cache)?)
        .bind(summary.exit_code)
        .bind(&summary.logs)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
