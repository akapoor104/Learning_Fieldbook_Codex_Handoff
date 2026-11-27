use std::time::Duration;

use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};

pub(crate) mod uploads;
pub use uploads::{sweep as sweep_uploads, PrivateMedia};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn connect_lazy(options: PgConnectOptions) -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(PROBE_TIMEOUT)
        .connect_lazy_with(options)
}

pub async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}

pub(crate) async fn is_ready(pool: &PgPool) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, check_schema(pool)).await,
        Ok(Ok(true))
    )
}

async fn check_schema(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    let applied: Vec<(i64, Vec<u8>, bool)> =
        sqlx::query_as("SELECT version, checksum, success FROM _sqlx_migrations")
            .fetch_all(&mut *connection)
            .await?;

    if applied.iter().any(|(_, _, success)| !success)
        || !MIGRATOR
            .iter()
            .filter(|migration| !migration.migration_type.is_down_migration())
            .all(|migration| {
                applied.iter().any(|(version, checksum, _)| {
                    *version == migration.version && checksum.as_slice() == &*migration.checksum
                })
            })
    {
        return Ok(false);
    }

    sqlx::query("SELECT id, label FROM households LIMIT 0")
        .execute(&mut *connection)
        .await?;
    Ok(true)
}
