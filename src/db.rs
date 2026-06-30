use crate::text;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

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
}
