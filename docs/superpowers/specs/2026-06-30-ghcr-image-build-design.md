# GHCR Image Build Design

## Goal

Move Docker image builds from the small production server to GitHub Actions so deploys only pull a prebuilt image.

## Approach

GitHub Actions builds and publishes the Docker image on pushes to `main`. The image is published to:

```text
ghcr.io/vivatfreedom/mechabaraholka-bot
```

Each successful build publishes:

- `latest` for the current `main`
- the short commit SHA for traceable rollbacks

The workflow also runs the same lightweight Rust checks used locally:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --lib`

## Runtime Deploy

`docker-compose.yml` references the published image. It keeps `build: .` as a local fallback, but documented production deploys use:

```sh
docker compose pull tgbot
docker compose up -d
```

The production server should not use `docker compose up -d --build` after this change. That command still builds locally and defeats the purpose of GHCR.

## Registry Access

The workflow uses GitHub's `GITHUB_TOKEN` with `packages: write` permission. If the GHCR package is public, the server can pull without login. If it is private, the server must log in once:

```sh
echo TOKEN | docker login ghcr.io -u vivatfreedom --password-stdin
```

The token must have package read access.
