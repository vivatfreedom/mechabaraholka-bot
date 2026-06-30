use crate::text;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::{str::FromStr, time::Duration};

pub async fn connect_sqlite(path_or_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let database_url = sqlite_database_url(path_or_url);
    let options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

fn sqlite_database_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("sqlite:") {
        path_or_url.to_string()
    } else {
        format!("sqlite://{path_or_url}?mode=rwc")
    }
}

pub async fn ensure_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS "Word" (
            "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            "word" TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(r#"DELETE FROM "Word" WHERE trim("word") = ''"#)
        .execute(pool)
        .await?;
    sqlx::query(
        r#"
        DELETE FROM "Word"
        WHERE "id" NOT IN (
            SELECT MIN("id")
            FROM "Word"
            GROUP BY lower(trim("word"))
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(r#"UPDATE "Word" SET "word" = lower(trim("word"))"#)
        .execute(pool)
        .await?;
    sqlx::query(r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_word_word_unique" ON "Word" ("word")"#)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_words(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(r#"SELECT "word" FROM "Word" ORDER BY "id""#)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(word,)| word).collect())
}

pub async fn add_words(pool: &SqlitePool, words: &[String]) -> Result<usize, sqlx::Error> {
    let mut added = 0;
    for word in words {
        let trimmed = word.trim().to_lowercase();
        if trimmed.is_empty() {
            continue;
        }

        let result = sqlx::query(r#"INSERT OR IGNORE INTO "Word" ("word") VALUES (?)"#)
            .bind(&trimmed)
            .execute(pool)
            .await?;
        added += result.rows_affected() as usize;
    }
    Ok(added)
}

pub async fn remove_word(pool: &SqlitePool, word: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(r#"DELETE FROM "Word" WHERE "word" = ?"#)
        .bind(word.trim())
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn contains_word(pool: &SqlitePool, message_text: &str) -> Result<bool, sqlx::Error> {
    let words = list_words(pool).await?;
    Ok(text::contains_ban_word(message_text, &words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_repository_persists_words_in_file() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("words.sqlite");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = connect_sqlite(&database_url).await?;
        ensure_schema(&pool).await?;

        let inserted = add_words(
            &pool,
            &["spam".to_string(), "spam".to_string(), "scam".to_string()],
        )
        .await?;
        assert_eq!(inserted, 2);
        assert_eq!(
            list_words(&pool).await?,
            vec!["spam".to_string(), "scam".to_string()]
        );
        assert!(contains_word(&pool, "message with SPAM").await?);
        assert!(!contains_word(&pool, "clean message").await?);

        assert_eq!(remove_word(&pool, "spam").await?, 1);
        pool.close().await;

        let reopened = connect_sqlite(&database_url).await?;
        ensure_schema(&reopened).await?;
        assert_eq!(list_words(&reopened).await?, vec!["scam".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn ensure_schema_deduplicates_existing_words_and_enforces_uniqueness(
    ) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("words.sqlite");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = connect_sqlite(&database_url).await?;

        sqlx::query(
            r#"
            CREATE TABLE "Word" (
                "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                "word" TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(r#"INSERT INTO "Word" ("word") VALUES ('Spam'), (' spam '), ('scam')"#)
            .execute(&pool)
            .await?;

        ensure_schema(&pool).await?;

        assert_eq!(
            list_words(&pool).await?,
            vec!["spam".to_string(), "scam".to_string()]
        );
        assert!(
            sqlx::query(r#"INSERT INTO "Word" ("word") VALUES ('spam')"#)
                .execute(&pool)
                .await
                .is_err()
        );
        assert_eq!(
            add_words(&pool, &["spam".to_string(), "fraud".to_string()]).await?,
            1
        );
        assert_eq!(
            list_words(&pool).await?,
            vec!["spam".to_string(), "scam".to_string(), "fraud".to_string()]
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_add_words_keeps_single_copy() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("words.sqlite");
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = connect_sqlite(&database_url).await?;
        ensure_schema(&pool).await?;

        let first_pool = pool.clone();
        let second_pool = pool.clone();
        let first = tokio::spawn(async move {
            add_words(&first_pool, &["spam".to_string(), "scam".to_string()]).await
        });
        let second = tokio::spawn(async move {
            add_words(&second_pool, &["spam".to_string(), "fraud".to_string()]).await
        });
        let first_added = first.await??;
        let second_added = second.await??;

        assert_eq!(first_added + second_added, 3);
        let mut words = list_words(&pool).await?;
        words.sort();
        assert_eq!(
            words,
            vec!["fraud".to_string(), "scam".to_string(), "spam".to_string()]
        );
        Ok(())
    }

    #[test]
    fn sqlite_database_url_accepts_url_or_plain_path() {
        assert_eq!(
            sqlite_database_url("/data/mechabaraholka.sqlite"),
            "sqlite:///data/mechabaraholka.sqlite?mode=rwc"
        );
        assert_eq!(
            sqlite_database_url("sqlite:///data/mechabaraholka.sqlite?mode=rwc"),
            "sqlite:///data/mechabaraholka.sqlite?mode=rwc"
        );
    }
}
