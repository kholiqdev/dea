# Dea

[![CI](https://github.com/kholiqdev/dea/actions/workflows/ci.yml/badge.svg)](https://github.com/kholiqdev/dea/actions/workflows/ci.yml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/kholiqdev/dea/badge)](https://scorecard.dev/viewer/?uri=github.com/kholiqdev/dea)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Conventional Commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-fe5196.svg)](https://www.conventionalcommits.org)

Dea is an open source desktop app that brings the developer tools you open every day into one window: an API client, a database client, a terminal with SSH, and a debugger for React Native apps. A project keeps the environments, hosts, connections and secret names of one system together, so every tool uses the same setup.

> **Status:** early development. Dea is not usable yet.

## Planned for the first release

- **Requests:** HTTP client with collections, environments, and scripts and tests that run Postman collections.
- **Database:** PostgreSQL, MySQL, MariaDB and SQLite with a SQL editor, editable results and query plans.
- **Terminal:** local shells and SSH with jump hosts and port forwarding.
- **Mobile:** network, logs, state and storage inspector for React Native apps.
- **Vault:** secrets stay in your operating system's keychain, never in project files.

Dea runs on macOS, Windows and Linux, works offline, and does not need an account.

## Building from source

You need Rust (the version is pinned in `rust-toolchain.toml`), Node.js 26 or later, pnpm, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
pnpm install
pnpm --filter desktop tauri dev
```

Run the checks:

```sh
cargo test --workspace
pnpm lint
pnpm typecheck
pnpm test
```

## License

Dea is licensed under the [Apache License 2.0](LICENSE).
