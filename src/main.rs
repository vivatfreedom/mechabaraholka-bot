use anyhow::Result;
use mechabaraholka_bot::{bot, config::Config, db};
use std::{collections::HashMap, sync::Arc};
use teloxide::prelude::*;
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = Arc::new(Config::from_env()?);
    let pool = db::connect(&config.database_url).await?;
    db::ensure_schema(&pool).await?;

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
