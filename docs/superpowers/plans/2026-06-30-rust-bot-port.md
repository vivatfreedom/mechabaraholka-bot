# Rust Bot Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the TypeScript Telegram moderation bot with a Rust bot that preserves behavior, PostgreSQL data compatibility, and the existing Docker redeploy workflow.

**Architecture:** The Rust application is split into small modules: config parsing, text/word logic, PostgreSQL access, voteban state, Telegram handlers, and runtime wiring. The bot uses long polling, reads the same `.env`, and keeps active votebans in memory just like the current implementation.

**Tech Stack:** Rust 2021, `teloxide 0.17`, `tokio`, `sqlx 0.8.6` with PostgreSQL, `dotenvy`, `anyhow`, `tracing`, Docker multi-stage build.

---

## File Structure

- Create `Cargo.toml`: Rust package metadata and dependency versions.
- Create `src/lib.rs`: module exports for tests and the binary.
- Create `src/config.rs`: parse `.env`-style values into typed configuration.
- Create `src/text.rs`: split `/addword` input and check forbidden-word matches.
- Create `src/db.rs`: create/check existing `"Word"` table and read/write forbidden words.
- Create `src/voteban.rs`: pure in-memory vote state and vote transitions.
- Create `src/bot.rs`: Telegram command, callback, moderation, and admin-log handlers.
- Replace `src/main.ts` with `src/main.rs`: initialize runtime, database pool, bot state, dispatching, and shutdown.
- Modify `Dockerfile`: replace Node image with Rust multi-stage build.
- Modify `docker-compose.yml`: keep service names, database volume, and network; pass `.env` into the bot service.
- Modify `.gitignore`: remove Node-only assumptions and add Rust build output.
- Modify `README.md`: update build/run instructions for Rust while preserving env variable documentation.
- Remove `package.json`, `package-lock.json`, `tsconfig.json`, `prisma/schema.prisma` after Rust runtime no longer depends on them.
- Keep `prisma/migrations/20250305163726_init/migration.sql` as historical schema documentation.

### Task 1: Rust Project, Config, And Text Logic

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/config.rs`
- Create: `src/text.rs`

- [ ] **Step 1: Write failing tests for config and text behavior**

Create `Cargo.toml`:

```toml
[package]
name = "mechabaraholka-bot"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
dotenvy = "0.15.7"
sqlx = { version = "0.8.6", default-features = false, features = ["runtime-tokio-rustls", "postgres"] }
teloxide = { version = "0.17.0", default-features = false, features = ["rustls"] }
tokio = { version = "1.48", features = ["macros", "rt-multi-thread", "signal"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
temp-env = "0.3"
```

Create `src/lib.rs`:

```rust
pub mod config;
pub mod text;
```

Create `src/config.rs` with tests first:

```rust
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
```

Create `src/text.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_addword_args_accepts_commas_semicolons_and_spaces() {
        assert_eq!(
            split_addword_args("Spam,Scam; Bad  Words"),
            vec!["spam", "scam", "bad", "words"]
        );
    }

    #[test]
    fn split_addword_args_drops_empty_parts() {
        assert_eq!(split_addword_args(" , ; spam ;; "), vec!["spam"]);
    }

    #[test]
    fn contains_ban_word_uses_case_insensitive_substring_matching() {
        let words = vec!["spam".to_string(), "fraud".to_string()];
        assert!(contains_ban_word("This has SPAM inside", &words));
        assert!(!contains_ban_word("Clean message", &words));
    }
}
```

- [ ] **Step 2: Run tests and verify they fail because functions are missing**

Run:

```sh
cargo test config text
```

Expected: compile failure naming missing functions such as `parse_admin_ids`, `parse_voteban_need_count`, `split_addword_args`, and `contains_ban_word`.

- [ ] **Step 3: Implement minimal config and text code**

Replace `src/config.rs` with:

```rust
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
```

Replace `src/text.rs` with:

```rust
pub fn split_addword_args(input: &str) -> Vec<String> {
    input
        .split(|ch| ch == ',' || ch == ';' || ch == ' ')
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

pub fn contains_ban_word(text: &str, words: &[String]) -> bool {
    let text = text.to_lowercase();
    words
        .iter()
        .map(|word| word.to_lowercase())
        .any(|word| text.contains(&word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_addword_args_accepts_commas_semicolons_and_spaces() {
        assert_eq!(
            split_addword_args("Spam,Scam; Bad  Words"),
            vec!["spam", "scam", "bad", "words"]
        );
    }

    #[test]
    fn split_addword_args_drops_empty_parts() {
        assert_eq!(split_addword_args(" , ; spam ;; "), vec!["spam"]);
    }

    #[test]
    fn contains_ban_word_uses_case_insensitive_substring_matching() {
        let words = vec!["spam".to_string(), "fraud".to_string()];
        assert!(contains_ban_word("This has SPAM inside", &words));
        assert!(!contains_ban_word("Clean message", &words));
    }
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run:

```sh
cargo test config text
```

Expected: all config and text tests pass.

- [ ] **Step 5: Commit**

```sh
git add Cargo.toml src/lib.rs src/config.rs src/text.rs
git commit -m "feat: add rust config and text logic"
```

### Task 2: PostgreSQL Word Repository

**Files:**
- Modify: `src/lib.rs`
- Create: `src/db.rs`

- [ ] **Step 1: Write failing database tests**

Add `pub mod db;` to `src/lib.rs`.

Create `src/db.rs`:

```rust
use sqlx::PgPool;

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn word_repository_uses_existing_mixed_case_word_table(pool: PgPool) -> anyhow::Result<()> {
        ensure_schema(&pool).await?;

        let inserted = add_words(&pool, &["spam".to_string(), "spam".to_string(), "scam".to_string()]).await?;
        assert_eq!(inserted, 2);
        assert_eq!(list_words(&pool).await?, vec!["spam".to_string(), "scam".to_string()]);

        assert!(contains_word(&pool, "message with SPAM").await?);
        assert!(!contains_word(&pool, "clean message").await?);

        assert_eq!(remove_word(&pool, "spam").await?, 1);
        assert_eq!(list_words(&pool).await?, vec!["scam".to_string()]);
        Ok(())
    }
}
```

- [ ] **Step 2: Run database test and verify it fails because repository functions are missing**

Run with a local PostgreSQL `DATABASE_URL`:

```sh
cargo test word_repository_uses_existing_mixed_case_word_table
```

Expected: compile failure naming missing functions `ensure_schema`, `add_words`, `list_words`, `contains_word`, and `remove_word`.

- [ ] **Step 3: Implement the repository**

Replace `src/db.rs` with:

```rust
use crate::text;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS "Word" (
            "id" SERIAL NOT NULL,
            "word" TEXT NOT NULL,
            CONSTRAINT "Word_pkey" PRIMARY KEY ("id")
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_words(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(r#"SELECT "word" FROM "Word" ORDER BY "id""#)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(word,)| word).collect())
}

pub async fn add_words(pool: &PgPool, words: &[String]) -> Result<usize, sqlx::Error> {
    let mut added = 0;
    for word in words {
        let trimmed = word.trim().to_lowercase();
        if trimmed.is_empty() {
            continue;
        }

        let exists = sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Word" WHERE "word" = $1"#)
            .bind(&trimmed)
            .fetch_one(pool)
            .await?;

        if exists == 0 {
            sqlx::query(r#"INSERT INTO "Word" ("word") VALUES ($1)"#)
                .bind(&trimmed)
                .execute(pool)
                .await?;
            added += 1;
        }
    }
    Ok(added)
}

pub async fn remove_word(pool: &PgPool, word: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(r#"DELETE FROM "Word" WHERE "word" = $1"#)
        .bind(word.trim())
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn contains_word(pool: &PgPool, message_text: &str) -> Result<bool, sqlx::Error> {
    let words = list_words(pool).await?;
    Ok(text::contains_ban_word(message_text, &words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn word_repository_uses_existing_mixed_case_word_table(pool: PgPool) -> anyhow::Result<()> {
        ensure_schema(&pool).await?;

        let inserted = add_words(&pool, &["spam".to_string(), "spam".to_string(), "scam".to_string()]).await?;
        assert_eq!(inserted, 2);
        assert_eq!(list_words(&pool).await?, vec!["spam".to_string(), "scam".to_string()]);

        assert!(contains_word(&pool, "message with SPAM").await?);
        assert!(!contains_word(&pool, "clean message").await?);

        assert_eq!(remove_word(&pool, "spam").await?, 1);
        assert_eq!(list_words(&pool).await?, vec!["scam".to_string()]);
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests and verify repository behavior**

Run:

```sh
cargo test word_repository_uses_existing_mixed_case_word_table
cargo test config text
```

Expected: repository, config, and text tests pass.

- [ ] **Step 5: Commit**

```sh
git add src/lib.rs src/db.rs
git commit -m "feat: add postgres word repository"
```

### Task 3: Voteban State Machine

**Files:**
- Modify: `src/lib.rs`
- Create: `src/voteban.rs`

- [ ] **Step 1: Write failing voteban tests**

Add `pub mod voteban;` to `src/lib.rs`.

Create `src/voteban.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_vote_starts_with_initiator_for_ban() {
        let vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.counts(), VoteCounts { for_ban: 1, against: 0 });
    }

    #[test]
    fn target_user_cannot_vote_on_own_ban() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(10, true), VoteResult::TargetCannotVote);
    }

    #[test]
    fn duplicate_same_direction_vote_is_reported() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(40, true), VoteResult::AlreadyVoted);
    }

    #[test]
    fn user_can_switch_vote_direction() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(50, false), VoteResult::Recorded);
        assert_eq!(vote.counts(), VoteCounts { for_ban: 1, against: 1 });
        assert_eq!(vote.record_vote(50, true), VoteResult::Recorded);
        assert_eq!(vote.counts(), VoteCounts { for_ban: 2, against: 0 });
    }
}
```

- [ ] **Step 2: Run voteban tests and verify they fail because types are missing**

Run:

```sh
cargo test voteban
```

Expected: compile failure naming missing `ActiveVoteban`, `VoteCounts`, and `VoteResult`.

- [ ] **Step 3: Implement voteban state**

Replace `src/voteban.rs` with:

```rust
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveVoteban {
    pub target_user_id: i64,
    pub target_message_id: i32,
    pub voteban_message_id: i32,
    pub initiator_id: i64,
    pub target_username: String,
    voters: HashMap<i64, bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteCounts {
    pub for_ban: usize,
    pub against: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteResult {
    Recorded,
    AlreadyVoted,
    TargetCannotVote,
}

impl ActiveVoteban {
    pub fn new(
        target_user_id: i64,
        target_message_id: i32,
        voteban_message_id: i32,
        initiator_id: i64,
        target_username: String,
    ) -> Self {
        let mut voters = HashMap::new();
        voters.insert(initiator_id, true);
        Self {
            target_user_id,
            target_message_id,
            voteban_message_id,
            initiator_id,
            target_username,
            voters,
        }
    }

    pub fn record_vote(&mut self, user_id: i64, for_ban: bool) -> VoteResult {
        if user_id == self.target_user_id {
            return VoteResult::TargetCannotVote;
        }

        if self.voters.get(&user_id).copied() == Some(for_ban) {
            return VoteResult::AlreadyVoted;
        }

        self.voters.insert(user_id, for_ban);
        VoteResult::Recorded
    }

    pub fn counts(&self) -> VoteCounts {
        let for_ban = self.voters.values().filter(|vote| **vote).count();
        let against = self.voters.values().filter(|vote| !**vote).count();
        VoteCounts { for_ban, against }
    }

    pub fn for_voters(&self) -> Vec<i64> {
        self.voters
            .iter()
            .filter_map(|(user_id, vote)| (*vote).then_some(*user_id))
            .collect()
    }

    pub fn against_voters(&self) -> Vec<i64> {
        self.voters
            .iter()
            .filter_map(|(user_id, vote)| (!*vote).then_some(*user_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_vote_starts_with_initiator_for_ban() {
        let vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.counts(), VoteCounts { for_ban: 1, against: 0 });
    }

    #[test]
    fn target_user_cannot_vote_on_own_ban() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(10, true), VoteResult::TargetCannotVote);
    }

    #[test]
    fn duplicate_same_direction_vote_is_reported() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(40, true), VoteResult::AlreadyVoted);
    }

    #[test]
    fn user_can_switch_vote_direction() {
        let mut vote = ActiveVoteban::new(10, 20, 30, 40, "target".to_string());
        assert_eq!(vote.record_vote(50, false), VoteResult::Recorded);
        assert_eq!(vote.counts(), VoteCounts { for_ban: 1, against: 1 });
        assert_eq!(vote.record_vote(50, true), VoteResult::Recorded);
        assert_eq!(vote.counts(), VoteCounts { for_ban: 2, against: 0 });
    }
}
```

- [ ] **Step 4: Run voteban tests and full unit tests**

Run:

```sh
cargo test voteban
cargo test --lib
```

Expected: all non-database unit tests pass.

- [ ] **Step 5: Commit**

```sh
git add src/lib.rs src/voteban.rs
git commit -m "feat: add voteban state machine"
```

### Task 4: Telegram Handler Helpers

**Files:**
- Modify: `src/lib.rs`
- Create: `src/bot.rs`

- [ ] **Step 1: Write failing formatting tests**

Add `pub mod bot;` to `src/lib.rs`.

Create `src/bot.rs` with pure helper tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::voteban::{ActiveVoteban, VoteCounts};

    #[test]
    fn format_voteban_text_preserves_current_message_shape() {
        let vote = ActiveVoteban::new(11, 22, 33, 44, "target".to_string());
        let text = format_voteban_text(&vote, &["@starter".to_string()], &[], 2);
        assert_eq!(
            text,
            "🗳️ Голосування за бан @target\n\n✅ За (1/2): @starter\n❌ Проти (0/2): немає"
        );
    }

    #[test]
    fn vote_threshold_returns_ban_or_cancel_action() {
        assert_eq!(threshold_action(VoteCounts { for_ban: 2, against: 0 }, 2), VoteThresholdAction::Ban);
        assert_eq!(threshold_action(VoteCounts { for_ban: 1, against: 2 }, 2), VoteThresholdAction::Cancel);
        assert_eq!(threshold_action(VoteCounts { for_ban: 1, against: 1 }, 2), VoteThresholdAction::Continue);
    }
}
```

- [ ] **Step 2: Run tests and verify they fail because helper functions are missing**

Run:

```sh
cargo test bot
```

Expected: compile failure naming missing `format_voteban_text`, `threshold_action`, and `VoteThresholdAction`.

- [ ] **Step 3: Implement helper functions and Telegram runtime types**

Implement `src/bot.rs` with:

```rust
use crate::{
    config::Config,
    db,
    text,
    voteban::{ActiveVoteban, VoteCounts, VoteResult},
};
use anyhow::Result;
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use teloxide::{
    payloads::SendMessageSetters,
    prelude::*,
    types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, Message, MessageId, UserId},
};
use tokio::sync::Mutex;
use tracing::{error, info};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub active_votebans: Arc<Mutex<HashMap<i32, ActiveVoteban>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteThresholdAction {
    Continue,
    Ban,
    Cancel,
}

pub fn threshold_action(counts: VoteCounts, need_count: usize) -> VoteThresholdAction {
    if counts.for_ban >= need_count {
        VoteThresholdAction::Ban
    } else if counts.against >= need_count {
        VoteThresholdAction::Cancel
    } else {
        VoteThresholdAction::Continue
    }
}

pub fn format_voteban_text(
    vote: &ActiveVoteban,
    pro_usernames: &[String],
    against_usernames: &[String],
    need_count: usize,
) -> String {
    let counts = vote.counts();
    format!(
        "🗳️ Голосування за бан @{}\n\n✅ За ({}/{}): {}\n❌ Проти ({}/{}): {}",
        vote.target_username,
        counts.for_ban,
        need_count,
        if pro_usernames.is_empty() { "немає".to_string() } else { pro_usernames.join(", ") },
        counts.against,
        need_count,
        if against_usernames.is_empty() { "немає".to_string() } else { against_usernames.join(", ") },
    )
}

pub fn voteban_keyboard(counts: VoteCounts, need_count: usize) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback(format!("✅ За ({}/{})", counts.for_ban, need_count), "vote_ban"),
        InlineKeyboardButton::callback(format!("❌ Проти ({}/{})", counts.against, need_count), "vote_against"),
    ]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voteban::{ActiveVoteban, VoteCounts};

    #[test]
    fn format_voteban_text_preserves_current_message_shape() {
        let vote = ActiveVoteban::new(11, 22, 33, 44, "target".to_string());
        let text = format_voteban_text(&vote, &["@starter".to_string()], &[], 2);
        assert_eq!(
            text,
            "🗳️ Голосування за бан @target\n\n✅ За (1/2): @starter\n❌ Проти (0/2): немає"
        );
    }

    #[test]
    fn vote_threshold_returns_ban_or_cancel_action() {
        assert_eq!(threshold_action(VoteCounts { for_ban: 2, against: 0 }, 2), VoteThresholdAction::Ban);
        assert_eq!(threshold_action(VoteCounts { for_ban: 1, against: 2 }, 2), VoteThresholdAction::Cancel);
        assert_eq!(threshold_action(VoteCounts { for_ban: 1, against: 1 }, 2), VoteThresholdAction::Continue);
    }
}
```

- [ ] **Step 4: Add async Telegram handlers around the tested helpers**

Extend `src/bot.rs` with handler functions:

```rust
pub async fn log_to_admins(bot: &Bot, state: &AppState, message: impl AsRef<str>) {
    let message = message.as_ref();
    info!("{message}");
    for admin_id in &state.config.admin_ids {
        if let Err(err) = bot.send_message(ChatId(*admin_id), message.to_string()).await {
            error!("Помилка при надсиланні повідомлення адміну {admin_id}: {err}");
        }
    }
}

pub async fn is_group_admin(bot: &Bot, state: &AppState, chat_id: ChatId, user_id: UserId) -> bool {
    match bot.get_chat_member(chat_id, user_id).await {
        Ok(member) => matches!(
            member.status(),
            teloxide::types::ChatMemberStatus::Administrator | teloxide::types::ChatMemberStatus::Owner
        ),
        Err(err) => {
            log_to_admins(bot, state, format!("Помилка при перевірці прав користувача {}: {err}", user_id.0)).await;
            false
        }
    }
}
```

Then implement the command and message handlers with the same replies as `src/main.ts`.

- [ ] **Step 5: Run helper tests and compile handlers**

Run:

```sh
cargo test bot
cargo check
```

Expected: helper tests pass and Telegram handler code compiles.

- [ ] **Step 6: Commit**

```sh
git add src/lib.rs src/bot.rs
git commit -m "feat: add telegram bot handlers"
```

### Task 5: Runtime Wiring

**Files:**
- Delete: `src/main.ts`
- Create: `src/main.rs`

- [ ] **Step 1: Write runtime entrypoint**

Create `src/main.rs`:

```rust
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
```

- [ ] **Step 2: Run compile check**

Run:

```sh
cargo check
```

Expected: binary and library compile.

- [ ] **Step 3: Commit**

```sh
git add src/main.rs
git rm src/main.ts
git commit -m "feat: wire rust bot runtime"
```

### Task 6: Docker And Compose Compatibility

**Files:**
- Modify: `Dockerfile`
- Modify: `docker-compose.yml`
- Modify: `.gitignore`

- [ ] **Step 1: Replace Dockerfile**

Replace `Dockerfile` with:

```dockerfile
FROM rust:1.93-slim AS builder

WORKDIR /opt/app
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/app
COPY --from=builder /opt/app/target/release/mechabaraholka-bot /usr/local/bin/mechabaraholka-bot
CMD ["mechabaraholka-bot"]
```

- [ ] **Step 2: Keep compose service names and pass env file**

Update `docker-compose.yml` so `tgbot` includes:

```yaml
    env_file:
      - .env
```

Keep `db`, `networks`, and `volumes` unchanged.

- [ ] **Step 3: Update `.gitignore`**

Ensure `.gitignore` contains:

```gitignore
target
.env
```

Remove entries that only relate to Yarn Plug'n'Play when Node files are removed.

- [ ] **Step 4: Verify compose syntax and image build**

Run:

```sh
cargo generate-lockfile
cargo test --lib
docker compose config
docker compose build tgbot
```

Expected: lockfile generated, library tests pass, compose config is valid, and `tgbot` image builds.

- [ ] **Step 5: Commit**

```sh
git add Cargo.lock Dockerfile docker-compose.yml .gitignore
git commit -m "chore: switch docker build to rust"
```

### Task 7: Remove Node And Prisma Runtime Files, Update Docs

**Files:**
- Delete: `package.json`
- Delete: `package-lock.json`
- Delete: `tsconfig.json`
- Delete: `prisma/schema.prisma`
- Modify: `README.md`

- [ ] **Step 1: Remove unused Node runtime files**

Run:

```sh
git rm package.json package-lock.json tsconfig.json prisma/schema.prisma
```

- [ ] **Step 2: Update README**

Replace Node/Prisma install instructions with Rust/Docker instructions:

```markdown
## Вимоги

- Docker & Docker Compose для продакшен-запуску
- Telegram Bot API Token
- PostgreSQL запускається через `docker-compose.yml`

## Налаштування

Створіть `.env`:

```env
BOT_TOKEN="token from BotFather"
ADMIN_IDS="id_number,id_number,id_number"
DATABASE_URL="postgresql://postgres:postgres@db:5432/antispambot?schema=public"
VOTEBAN_NEED_COUNT=2
```

## Запуск через Docker Compose

```sh
docker compose up -d --build
```

Бот автоматично створить таблицю `"Word"`, якщо база нова. Якщо база вже існує після попередньої Node/Prisma-версії, дані в `"Word"` будуть використані без ручної міграції.
```

- [ ] **Step 3: Verify no Node runtime references remain**

Run:

```sh
rg -n "npm|node|Prisma|prisma generate|ts-node|typescript|package.json" README.md Dockerfile docker-compose.yml src Cargo.toml
```

Expected: no matches for runtime instructions. Historical migration paths under `prisma/migrations` may still exist.

- [ ] **Step 4: Run final checks**

Run:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo check
docker compose config
```

Expected: all commands pass.

- [ ] **Step 5: Commit**

```sh
git add README.md
git commit -m "docs: update rust deployment instructions"
```

### Task 8: End-To-End Verification

**Files:**
- No planned file edits.

- [ ] **Step 1: Check git status**

Run:

```sh
git status --short
```

Expected: clean working tree.

- [ ] **Step 2: Summarize deploy command**

Confirm that production update remains:

```sh
git pull
docker compose up -d --build
```

Expected: Rust image rebuilds, bot reads `.env`, connects to existing Postgres volume, and uses existing `"Word"` records.
