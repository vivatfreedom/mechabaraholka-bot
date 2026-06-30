# SQLite Migration Design

## Goal

Move the bot's persistent word storage from PostgreSQL to SQLite while preserving current bot behavior and providing a one-time import path from the existing PostgreSQL `"Word"` table.

## Chosen Approach

SQLite becomes the primary runtime database. The bot stores its database file at `SQLITE_PATH`, with `/data/mechabaraholka.sqlite` as the Docker-oriented default. Docker Compose mounts a named volume at `/data`, so image rebuilds and container recreation keep the SQLite file intact. The data is removed only when the named volume is explicitly deleted, for example with `docker compose down -v`.

PostgreSQL is used only as an optional first-run migration source. If `POSTGRES_MIGRATION_URL` is set and the SQLite `"Word"` table is empty, startup imports distinct words from PostgreSQL into SQLite. After a successful first run, `POSTGRES_MIGRATION_URL` can be removed and the PostgreSQL service can be stopped or removed.

## Data Model

SQLite keeps the same logical table name and columns:

```sql
CREATE TABLE IF NOT EXISTS "Word" (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "word" TEXT NOT NULL
);
```

The application still lowercases new words, avoids duplicate inserts, lists words by insertion id, deletes by exact word, and checks forbidden words with case-insensitive substring matching in Rust.

## Configuration

Required environment variables remain:

- `BOT_TOKEN`
- `ADMIN_IDS`

Database-related variables become:

- `SQLITE_PATH=/data/mechabaraholka.sqlite`
- `POSTGRES_MIGRATION_URL=postgresql://postgres:postgres@db:5432/antispambot?schema=public` for the first migration run only
- `VOTEBAN_NEED_COUNT=2`

For compatibility during transition, the code may also accept `DATABASE_URL` when it points to SQLite, but production docs should use `SQLITE_PATH`.

## Docker Flow

Compose mounts `bot_data:/data` into `tgbot`. The PostgreSQL service is moved behind a `migration` profile so regular bot runs do not start PostgreSQL after migration.

First migration run:

```sh
docker compose --profile migration up -d --build
```

After logs confirm the bot started, remove `POSTGRES_MIGRATION_URL` from `.env` and run:

```sh
docker compose up -d --build
docker compose stop db
```

Normal future updates:

```sh
git pull
docker compose up -d --build
```

## Testing

Unit tests cover SQLite schema creation and repository behavior with a temp file. A PostgreSQL-backed ignored test covers importing from the old `"Word"` table when a real PostgreSQL URL is available. Existing text/config/voteban tests remain.
