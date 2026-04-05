#![allow(dead_code)]

use dbsurveyor_core::{Result, error::DbSurveyorError};
use std::time::Duration;

#[cfg(feature = "postgresql")]
pub async fn wait_for_postgres_ready(database_url: &str, max_attempts: u32) -> Result<()> {
    use sqlx::PgPool;

    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = PgPool::connect(database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                return Ok(());
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    Err(DbSurveyorError::connection_failed(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!(
            "PostgreSQL failed to become ready after {} attempts",
            max_attempts
        ),
    )))
}

#[cfg(feature = "mysql")]
pub async fn wait_for_mysql_ready(database_url: &str, max_attempts: u32) -> Result<()> {
    use sqlx::MySqlPool;

    let mut attempts = 0;
    while attempts < max_attempts {
        if let Ok(pool) = MySqlPool::connect(database_url).await {
            if sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok() {
                pool.close().await;
                return Ok(());
            }
            pool.close().await;
        }
        attempts += 1;
        if attempts < max_attempts {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    Err(DbSurveyorError::connection_failed(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!(
            "MySQL failed to become ready after {} attempts",
            max_attempts
        ),
    )))
}
