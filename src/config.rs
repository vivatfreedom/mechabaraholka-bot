use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub bot_token: String,
    pub admin_ids: Vec<i64>,
    pub sqlite_path: String,
    pub postgres_migration_url: Option<String>,
    pub voteban_need_count: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let bot_token = env::var("BOT_TOKEN").context("BOT_TOKEN is required")?;
        let legacy_database_url = env::var("DATABASE_URL")
            .ok()
            .filter(|url| !url.trim().is_empty());
        let sqlite_path = env::var("SQLITE_PATH")
            .ok()
            .filter(|path| !path.trim().is_empty())
            .or_else(|| {
                legacy_database_url
                    .as_ref()
                    .filter(|url| url.starts_with("sqlite:"))
                    .cloned()
            })
            .unwrap_or_else(|| "/data/mechabaraholka.sqlite".to_string());
        let postgres_migration_url = env::var("POSTGRES_MIGRATION_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
            .or_else(|| {
                legacy_database_url.filter(|url| {
                    url.starts_with("postgres://") || url.starts_with("postgresql://")
                })
            });
        let admin_ids = parse_admin_ids(&env::var("ADMIN_IDS").unwrap_or_default());
        let voteban_need_count =
            parse_voteban_need_count(env::var("VOTEBAN_NEED_COUNT").ok().as_deref());

        Ok(Self {
            bot_token,
            admin_ids,
            sqlite_path,
            postgres_migration_url,
            voteban_need_count,
        })
    }

    pub fn is_bot_admin(&self, user_id: i64) -> bool {
        self.admin_ids.contains(&user_id)
    }
}

pub fn parse_admin_ids(value: &str) -> Vec<i64> {
    value
        .split(',')
        .filter_map(|id| id.trim().parse::<i64>().ok())
        .collect()
}

pub fn parse_voteban_need_count(value: Option<&str>) -> usize {
    value
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_admin_ids_trims_ignores_empty_values_and_preserves_order() {
        assert_eq!(parse_admin_ids(" 123,456, ,789 "), vec![123, 456, 789]);
    }

    #[test]
    fn parse_admin_ids_ignores_non_numeric_values_like_current_string_membership_does() {
        assert_eq!(parse_admin_ids("123,abc,456"), vec![123, 456]);
    }

    #[test]
    fn parse_voteban_need_count_defaults_to_two_when_missing_or_invalid() {
        assert_eq!(parse_voteban_need_count(None), 2);
        assert_eq!(parse_voteban_need_count(Some("")), 2);
        assert_eq!(parse_voteban_need_count(Some("abc")), 2);
    }

    #[test]
    fn parse_voteban_need_count_uses_positive_numbers() {
        assert_eq!(parse_voteban_need_count(Some("4")), 4);
    }

    #[test]
    fn from_env_uses_sqlite_default_and_optional_postgres_migration_url() {
        temp_env::with_vars(
            [
                ("BOT_TOKEN", Some("token")),
                ("ADMIN_IDS", Some("123,456")),
                ("DATABASE_URL", None),
                ("SQLITE_PATH", None),
                (
                    "POSTGRES_MIGRATION_URL",
                    Some("postgresql://postgres:postgres@db:5432/antispambot?schema=public"),
                ),
                ("VOTEBAN_NEED_COUNT", Some("3")),
            ],
            || {
                let config = Config::from_env().expect("config should parse");
                assert_eq!(config.bot_token, "token");
                assert_eq!(config.admin_ids, vec![123, 456]);
                assert_eq!(config.sqlite_path, "/data/mechabaraholka.sqlite");
                assert_eq!(
                    config.postgres_migration_url.as_deref(),
                    Some("postgresql://postgres:postgres@db:5432/antispambot?schema=public")
                );
                assert_eq!(config.voteban_need_count, 3);
            },
        );
    }

    #[test]
    fn from_env_uses_explicit_sqlite_path_and_ignores_blank_postgres_url() {
        temp_env::with_vars(
            [
                ("BOT_TOKEN", Some("token")),
                ("ADMIN_IDS", Some("123")),
                ("DATABASE_URL", None),
                ("SQLITE_PATH", Some("/tmp/bot.sqlite")),
                ("POSTGRES_MIGRATION_URL", Some("")),
                ("VOTEBAN_NEED_COUNT", None),
            ],
            || {
                let config = Config::from_env().expect("config should parse");
                assert_eq!(config.sqlite_path, "/tmp/bot.sqlite");
                assert_eq!(config.postgres_migration_url, None);
                assert_eq!(config.voteban_need_count, 2);
            },
        );
    }

    #[test]
    fn from_env_uses_old_postgres_database_url_as_migration_source() {
        temp_env::with_vars(
            [
                ("BOT_TOKEN", Some("token")),
                ("ADMIN_IDS", Some("123")),
                (
                    "DATABASE_URL",
                    Some("postgresql://postgres:postgres@db:5432/antispambot?schema=public"),
                ),
                ("SQLITE_PATH", None),
                ("POSTGRES_MIGRATION_URL", None),
                ("VOTEBAN_NEED_COUNT", None),
            ],
            || {
                let config = Config::from_env().expect("config should parse");
                assert_eq!(config.sqlite_path, "/data/mechabaraholka.sqlite");
                assert_eq!(
                    config.postgres_migration_url.as_deref(),
                    Some("postgresql://postgres:postgres@db:5432/antispambot?schema=public")
                );
            },
        );
    }

    #[test]
    fn from_env_accepts_sqlite_database_url_as_sqlite_path() {
        temp_env::with_vars(
            [
                ("BOT_TOKEN", Some("token")),
                ("ADMIN_IDS", Some("123")),
                ("DATABASE_URL", Some("sqlite:///tmp/bot.sqlite?mode=rwc")),
                ("SQLITE_PATH", None),
                ("POSTGRES_MIGRATION_URL", None),
                ("VOTEBAN_NEED_COUNT", None),
            ],
            || {
                let config = Config::from_env().expect("config should parse");
                assert_eq!(config.sqlite_path, "sqlite:///tmp/bot.sqlite?mode=rwc");
                assert_eq!(config.postgres_migration_url, None);
            },
        );
    }
}
