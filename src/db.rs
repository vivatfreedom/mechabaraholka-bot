use crate::text;
use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions, PgPool, SqlitePool};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    NotConfigured,
    SkippedSqliteHasWords { existing_count: i64 },
    Imported { count: usize },
}

pub async fn connect_sqlite(path_or_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let database_url = sqlite_database_url(path_or_url);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
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

        let exists =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Word" WHERE "word" = ?"#)
                .bind(&trimmed)
                .fetch_one(pool)
                .await?;

        if exists == 0 {
            sqlx::query(r#"INSERT INTO "Word" ("word") VALUES (?)"#)
                .bind(&trimmed)
                .execute(pool)
                .await?;
            added += 1;
        }
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

pub async fn sqlite_word_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Word""#)
        .fetch_one(pool)
        .await
}

pub async fn import_words_into_sqlite(
    pool: &SqlitePool,
    words: &[String],
) -> Result<usize, sqlx::Error> {
    add_words(pool, words).await
}

pub async fn migrate_from_postgres_if_empty(
    sqlite_pool: &SqlitePool,
    postgres_url: Option<&str>,
) -> Result<MigrationOutcome, sqlx::Error> {
    let Some(postgres_url) = postgres_url else {
        return Ok(MigrationOutcome::NotConfigured);
    };

    let existing_count = sqlite_word_count(sqlite_pool).await?;
    if existing_count > 0 {
        return Ok(MigrationOutcome::SkippedSqliteHasWords { existing_count });
    }

    let pg_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(postgres_url)
        .await?;
    let words = postgres_words(&pg_pool).await?;
    let count = import_words_into_sqlite(sqlite_pool, &words).await?;
    Ok(MigrationOutcome::Imported { count })
}

async fn postgres_words(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String,)>(r#"SELECT "word" FROM "Word" ORDER BY "id""#)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(word,)| word).collect())
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

    #[tokio::test]
    async fn migration_skips_postgres_when_sqlite_already_has_words() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("words.sqlite").display()
        );
        let pool = connect_sqlite(&database_url).await?;
        ensure_schema(&pool).await?;
        add_words(&pool, &["existing".to_string()]).await?;

        let outcome =
            migrate_from_postgres_if_empty(&pool, Some("postgresql://invalid.invalid/db")).await?;

        assert_eq!(
            outcome,
            MigrationOutcome::SkippedSqliteHasWords { existing_count: 1 }
        );
        assert_eq!(list_words(&pool).await?, vec!["existing".to_string()]);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires POSTGRES_MIGRATION_URL pointing at the old PostgreSQL database"]
    async fn migration_imports_words_from_existing_postgres_word_table() -> anyhow::Result<()> {
        let postgres_url = std::env::var("POSTGRES_MIGRATION_URL")?;
        let pg_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&postgres_url)
            .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS "Word" (
                "id" SERIAL NOT NULL,
                "word" TEXT NOT NULL,
                CONSTRAINT "Word_pkey" PRIMARY KEY ("id")
            )
            "#,
        )
        .execute(&pg_pool)
        .await?;

        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let first = format!("migration-{suffix}-spam");
        let second = format!("migration-{suffix}-scam");
        sqlx::query(r#"INSERT INTO "Word" ("word") VALUES ($1), ($2), ($1)"#)
            .bind(&first)
            .bind(&second)
            .execute(&pg_pool)
            .await?;

        let dir = tempfile::tempdir()?;
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("words.sqlite").display()
        );
        let sqlite_pool = connect_sqlite(&database_url).await?;
        ensure_schema(&sqlite_pool).await?;

        let outcome = migrate_from_postgres_if_empty(&sqlite_pool, Some(&postgres_url)).await?;
        let words = list_words(&sqlite_pool).await?;

        sqlx::query(r#"DELETE FROM "Word" WHERE "word" = $1 OR "word" = $2"#)
            .bind(&first)
            .bind(&second)
            .execute(&pg_pool)
            .await?;

        assert!(matches!(outcome, MigrationOutcome::Imported { count } if count >= 2));
        assert!(words.contains(&first));
        assert!(words.contains(&second));
        Ok(())
    }
}
