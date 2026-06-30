# SQLite Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch runtime persistence from PostgreSQL to SQLite and add a safe first-run import from the old PostgreSQL `"Word"` table.

**Architecture:** `src/db.rs` owns SQLite connection, schema, repository operations, and optional PostgreSQL import. `src/config.rs` reads `SQLITE_PATH` plus optional `POSTGRES_MIGRATION_URL`. `src/main.rs` connects to SQLite, ensures schema, runs migration, then starts the Telegram dispatcher.

**Tech Stack:** Rust, sqlx with `sqlite` and `postgres` features, tokio, Docker Compose named volumes.

---

### Task 1: SQLite Repository Tests

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/db.rs`

- [ ] Add `tempfile` as a dev dependency.
- [ ] Add a test in `src/db.rs` named `sqlite_repository_persists_words_in_file`.
- [ ] The test should create a temp directory, connect to `sqlite://<path>?mode=rwc`, call `ensure_schema`, add duplicate words, list them, check forbidden matching, remove one word, reconnect to the same file, and verify remaining data persists.
- [ ] Run `cargo test sqlite_repository_persists_words_in_file` and verify it fails before SQLite code exists.

### Task 2: SQLite Runtime Storage

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/db.rs`
- Modify: `src/bot.rs`
- Modify: `src/main.rs`
- Modify: `src/config.rs`

- [ ] Enable `sqlx` `sqlite` feature while keeping `postgres` for migration.
- [ ] Replace runtime `PgPool` usage with `SqlitePool`.
- [ ] Implement `connect_sqlite(path_or_url: &str)`.
- [ ] Implement SQLite `ensure_schema`.
- [ ] Update CRUD SQL placeholders from `$1` to `?`.
- [ ] Update `Config` to use `SQLITE_PATH`, defaulting to `/data/mechabaraholka.sqlite`, and optional `POSTGRES_MIGRATION_URL`.
- [ ] Update `main` to connect to SQLite and ensure schema.
- [ ] Run the SQLite repository test and full `cargo test --lib`.

### Task 3: PostgreSQL Import Tests and Implementation

**Files:**
- Modify: `src/db.rs`
- Modify: `src/main.rs`

- [ ] Add a unit test showing `migrate_from_postgres_if_empty` skips import when SQLite already has words.
- [ ] Add an ignored integration test that creates old PostgreSQL `"Word"` rows and imports them into a temp SQLite file.
- [ ] Implement `sqlite_word_count`, `import_words_into_sqlite`, and `migrate_from_postgres_if_empty`.
- [ ] Call migration after SQLite schema creation in `main`.
- [ ] Run the skip test and the ignored PostgreSQL migration test with a real `POSTGRES_MIGRATION_URL`.

### Task 4: Docker and Documentation

**Files:**
- Modify: `docker-compose.yml`
- Modify: `.env example`
- Modify: `README.md`
- Modify: `.gitignore`

- [ ] Mount `bot_data:/data` into `tgbot`.
- [ ] Move `db` service behind the `migration` profile.
- [ ] Keep a network shared by `tgbot` and migration `db`.
- [ ] Replace `DATABASE_URL` in `.env example` with `SQLITE_PATH` and commented `POSTGRES_MIGRATION_URL`.
- [ ] Update README with first-run migration and normal update commands.
- [ ] Ignore local SQLite files.

### Task 5: Verification and Commits

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test --lib`.
- [ ] Run the ignored PostgreSQL migration test when the local compose DB is available.
- [ ] Run `docker compose config`.
- [ ] Run `docker compose build tgbot`.
- [ ] Commit spec and plan.
- [ ] Commit implementation.
