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

        let exists =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Word" WHERE "word" = $1"#)
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
    #[ignore = "requires a reachable PostgreSQL DATABASE_URL"]
    async fn word_repository_uses_existing_mixed_case_word_table(
        pool: PgPool,
    ) -> anyhow::Result<()> {
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
        assert_eq!(list_words(&pool).await?, vec!["scam".to_string()]);
        Ok(())
    }
}
