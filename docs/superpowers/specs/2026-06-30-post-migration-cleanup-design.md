# Post-Migration Cleanup Design

## Goal

Remove the temporary PostgreSQL migration path after the production data has been imported into SQLite.

## Runtime Shape

The bot is now SQLite-only at runtime. It reads `SQLITE_PATH`, defaults to `/data/mechabaraholka.sqlite`, creates the SQLite `"Word"` table if needed, and uses that database for all word operations.

PostgreSQL migration environment variables are no longer consumed:

- `POSTGRES_MIGRATION_URL`
- legacy PostgreSQL `DATABASE_URL`

The application may continue to accept a SQLite `DATABASE_URL` as a compatibility alias only if it starts with `sqlite:`, but PostgreSQL URLs should be ignored.

## Docker Compose

Compose should run only the bot container:

- `tgbot` uses the prebuilt image `ghcr.io/vivatfreedom/mechabaraholka-bot:latest`.
- `build: .` is removed to prevent accidental Rust builds on the small server.
- The PostgreSQL service, migration profile, PostgreSQL volume, and custom network are removed.
- `bot_data:/data` remains the only named volume.

Deploy after this cleanup:

```sh
git pull
docker compose pull tgbot
docker compose up -d --remove-orphans
```

`--remove-orphans` removes the old compose-managed PostgreSQL container but does not delete volumes.

## Dependencies

`sqlx` should only enable SQLite runtime features needed by the app. PostgreSQL-specific code, tests, and dependency features are removed.
