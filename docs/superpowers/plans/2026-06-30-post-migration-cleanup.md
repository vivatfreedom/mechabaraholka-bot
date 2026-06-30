# Post-Migration Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove temporary PostgreSQL migration support and make production runtime SQLite-only with GHCR image pulls.

**Architecture:** `src/config.rs` reads only bot settings and SQLite path. `src/db.rs` owns only SQLite storage. `docker-compose.yml` runs a single image-backed bot service with `bot_data`.

**Tech Stack:** Rust, sqlx SQLite, Docker Compose, GHCR image.

---

### Task 1: Remove Migration Config

**Files:**
- Modify: `src/config.rs`

- [ ] Replace PostgreSQL migration config tests with a test proving PostgreSQL `DATABASE_URL` is ignored.
- [ ] Remove `postgres_migration_url` from `Config`.
- [ ] Keep `SQLITE_PATH` and optional SQLite `DATABASE_URL` alias.

### Task 2: Remove PostgreSQL Migration Code

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/db.rs`
- Modify: `src/main.rs`

- [ ] Remove `postgres`, `macros`, and `migrate` sqlx features.
- [ ] Remove `MigrationOutcome`, PostgreSQL imports, and migration functions/tests.
- [ ] Remove migration startup call from `main`.
- [ ] Run `cargo test --lib`.

### Task 3: Simplify Compose and Docs

**Files:**
- Modify: `docker-compose.yml`
- Modify: `.env example`
- Modify: `README.md`

- [ ] Remove `build: .`, `db`, migration profile, custom network, and PostgreSQL volume from compose.
- [ ] Remove PostgreSQL migration variables from `.env example`.
- [ ] Replace README migration instructions with a post-migration cleanup/deploy note.

### Task 4: Verify and Commit

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test --lib`.
- [ ] Run `docker compose config --quiet`.
- [ ] Commit docs.
- [ ] Commit implementation.
