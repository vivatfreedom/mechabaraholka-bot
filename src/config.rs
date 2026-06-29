use anyhow::{Context, Result};
use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub bot_token: String,
    pub admin_ids: Vec<i64>,
    pub database_url: String,
    pub voteban_need_count: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let bot_token = env::var("BOT_TOKEN").context("BOT_TOKEN is required")?;
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
        let admin_ids = parse_admin_ids(&env::var("ADMIN_IDS").unwrap_or_default());
        let voteban_need_count =
            parse_voteban_need_count(env::var("VOTEBAN_NEED_COUNT").ok().as_deref());

        Ok(Self {
            bot_token,
            admin_ids,
            database_url,
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
}
