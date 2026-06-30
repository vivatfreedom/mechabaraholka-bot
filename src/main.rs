use anyhow::Result;
use mechabaraholka_bot::{bot, config::Config, db};
use std::{collections::HashMap, sync::Arc};
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = Arc::new(Config::from_env()?);
    let pool = db::connect_sqlite(&config.sqlite_path).await?;
    db::ensure_schema(&pool).await?;
    match db::migrate_from_postgres_if_empty(&pool, config.postgres_migration_url.as_deref())
        .await?
    {
        db::MigrationOutcome::NotConfigured => {}
        db::MigrationOutcome::SkippedSqliteHasWords { existing_count } => {
            info!("SQLite migration skipped: {existing_count} words already exist");
        }
        db::MigrationOutcome::Imported { count } => {
            info!("SQLite migration imported {count} words from PostgreSQL");
        }
    }

    let bot_instance = Bot::new(config.bot_token.clone());
    let state = bot::AppState {
        config,
        pool,
        active_votebans: Arc::new(Mutex::new(HashMap::new())),
    };

    bot::log_to_admins(&bot_instance, &state, "Бот успішно запущений!").await;
    bot::run(bot_instance, state).await;
    Ok(())
}
