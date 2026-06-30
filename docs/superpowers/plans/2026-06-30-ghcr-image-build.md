# GHCR Image Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GitHub Actions publishing for a prebuilt Docker image and update deploy docs to pull that image on the server.

**Architecture:** A workflow under `.github/workflows/` validates Rust code, logs into GHCR with `GITHUB_TOKEN`, builds the existing Dockerfile with BuildKit, and pushes `latest` plus SHA tags. Compose references the GHCR image while keeping local build fallback.

**Tech Stack:** GitHub Actions, Docker Buildx, GitHub Container Registry, Docker Compose.

---

### Task 1: Add Workflow

**Files:**
- Create: `.github/workflows/docker-image.yml`

- [ ] Add a workflow triggered by pushes to `main` and manual `workflow_dispatch`.
- [ ] Grant `contents: read` and `packages: write`.
- [ ] Add Rust validation steps: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --lib`.
- [ ] Add Docker login to `ghcr.io` using `${{ github.actor }}` and `${{ secrets.GITHUB_TOKEN }}`.
- [ ] Add Docker metadata tags for `latest` and short SHA.
- [ ] Add Docker build/push using `docker/build-push-action@v6`.

### Task 2: Update Compose

**Files:**
- Modify: `docker-compose.yml`

- [ ] Add `image: ghcr.io/vivatfreedom/mechabaraholka-bot:latest` to service `tgbot`.
- [ ] Keep `build: .` so local development can still build if needed.

### Task 3: Update Docs

**Files:**
- Modify: `README.md`

- [ ] Document that production deploy uses `docker compose pull tgbot` and `docker compose up -d`.
- [ ] Warn not to use `--build` on the small production server.
- [ ] Document optional GHCR login for private packages.

### Task 4: Verify

**Files:**
- All changed files

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test --lib`.
- [ ] Run `docker compose config`.
- [ ] Commit the workflow and docs.
