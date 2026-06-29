# Rust Bot Port Design

## Goal

Rewrite the Telegram antispam and moderation bot from TypeScript/Node.js to Rust while preserving runtime behavior and deployment compatibility.

After updating the repository, the operator must be able to run:

```sh
docker compose up -d --build
```

The rebuilt bot must start against the existing PostgreSQL volume, read the existing `.env`, and continue using the current `"Word"` table without a manual data migration.

## Current Behavior To Preserve

The current bot uses `grammy`, Prisma, PostgreSQL, and long polling. The Rust version must preserve these behaviors:

- Read `BOT_TOKEN`, `ADMIN_IDS`, `DATABASE_URL`, and `VOTEBAN_NEED_COUNT` from the environment.
- Treat `ADMIN_IDS` as bot-level admins for blacklist commands and admin log delivery.
- Send operational log messages to stdout and to every configured bot admin.
- Support `/addword <words>` for bot admins.
- Split `/addword` arguments by comma, semicolon, and spaces.
- Lowercase added words.
- Avoid inserting duplicate words.
- Support `/listwords` for bot admins.
- Support `/removeword <word>` for bot admins.
- Check non-admin group messages for forbidden words by case-insensitive substring matching.
- Ban and delete the triggering message when a non-admin sends forbidden text.
- Ban and delete the triggering message when a non-admin forwards a message from another chat.
- Never moderate Telegram group administrators or creators.
- Support `/voteban` only in group chats and only as a reply to a target message.
- Delete the `/voteban` command message when possible.
- Start a vote with the initiator voting for ban.
- Store active votebans in memory, not in the database.
- Let users switch between "for ban" and "against ban" votes.
- Reject duplicate same-direction votes.
- Reject the target user voting on their own ban.
- Ban the target, delete the target message, delete the vote message, and log when for-ban votes reach `VOTEBAN_NEED_COUNT`.
- Delete the vote message and log when against-ban votes reach `VOTEBAN_NEED_COUNT`.
- Ignore "message is not modified" style edit failures.
- Disconnect database and stop polling on termination signals.

User-facing Ukrainian messages should remain semantically the same as the current bot. Exact emoji, command names, and callback button labels should be preserved where the Telegram Rust library permits it directly.

## Data Compatibility

The existing Prisma schema created this table:

```sql
CREATE TABLE "Word" (
    "id" SERIAL NOT NULL,
    "word" TEXT NOT NULL,
    CONSTRAINT "Word_pkey" PRIMARY KEY ("id")
);
```

The Rust implementation must query the table with the quoted mixed-case name `"Word"`. It must not rename the table, require a new table, or require the operator to export/import data.

The project may keep the existing SQL migration file for historical compatibility. Runtime startup should ensure the table exists using SQL equivalent to the existing migration, so a fresh deployment still works without Prisma.

## Architecture

Use a small Rust application with explicit modules:

- `src/main.rs`: initialize config, database pool, shared bot state, command handlers, signal handling, and polling.
- `src/config.rs`: parse environment variables and provide typed configuration.
- `src/db.rs`: own all PostgreSQL access for forbidden words.
- `src/bot.rs`: own Telegram command handlers, callback handlers, moderation handlers, and admin logging.

The bot should use:

- `teloxide` for Telegram Bot API long polling, commands, callback queries, inline keyboards, chat member checks, message deletion, and bans.
- `sqlx` with PostgreSQL for database access.
- `tokio` for async runtime and signal handling.
- `dotenvy` for `.env` loading.
- `anyhow` or typed error returns where it keeps handler code clear.
- `tracing` or `log` plus stdout logging for operational visibility.

Shared runtime state:

- `AppState` holds config, database pool, and active votebans.
- Active votebans are stored in an async lock around `HashMap<MessageId, ActiveVoteban>`.
- Voters are stored as `HashMap<UserId, bool>`, where `true` means a vote for ban and `false` means against ban.

## Deployment

Replace the Node Docker image with a Rust multi-stage build:

- Builder stage compiles the Rust binary.
- Runtime stage contains only the compiled binary and runtime files needed for TLS/PostgreSQL connectivity.
- The container starts the binary directly.

`docker-compose.yml` should remain operational with the same service names, PostgreSQL volume, and network. The `tgbot` service should continue depending on `db`. Environment variables should be read from the existing `.env` workflow.

The final repository should no longer require:

- `npm install`
- `npm start`
- `npx prisma migrate deploy`
- `npx prisma generate`
- `node_modules`

Node-specific files can be removed if the Rust project replaces them fully:

- `package.json`
- `package-lock.json`
- `tsconfig.json`
- TypeScript source files
- Prisma schema/client generator files if no longer used by runtime

The existing SQL migration file may remain as documentation, but Rust startup must not depend on Prisma.

## Error Handling

Telegram API and database errors should be logged and, where the current bot notifies admins, sent to configured admins.

The Rust bot should continue after recoverable handler errors. Failure to notify one admin must not prevent notifying the remaining admins.

Forbidden-word lookup failures should be logged. A database lookup failure must not ban a user because the bot cannot prove a violation.

Admin checks should fail closed for moderation: if the bot cannot check whether a user is a group admin, it should log the error and treat the user as non-admin only where that matches current behavior. The current bot returns `false` on admin-check errors, so the Rust port should preserve that behavior.

## Testing

Add focused automated tests for pure behavior and database behavior:

- Config parsing from environment-style values.
- Add-word argument splitting and lowercasing.
- Forbidden-word matching with case-insensitive substring semantics.
- Voteban vote state transitions, including duplicate votes and switching sides.
- SQL compatibility for the `"Word"` table using a test PostgreSQL database when available.

Telegram network behavior should be kept behind small functions so command/message handler logic can be tested without calling the Telegram API for every case. Where direct handler tests would require heavy mocking, test the extracted pure logic and database layer instead.

## Out Of Scope

This port should not add new moderation features, persistence for votebans, caching, lemmatization, a web UI, or a new database schema.

The goal is a conservative Rust rewrite that behaves like the current bot and redeploys through the existing Docker workflow.
