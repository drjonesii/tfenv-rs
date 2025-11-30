Purpose

This repository is a Rust-based CLI for managing Terraform and OpenTofu versions, ported from the original Bash tfenv. These instructions help AI coding agents become productive quickly by explaining the project's structure, conventions, developer workflows, and where to look for examples.

Big picture

- The project is a Rust CLI: `src/main.rs` is the entrypoint, dispatching to subcommands and logic in `src/installer.rs`, `src/version.rs`, etc.
- Shared helpers and utilities live in `src/` (e.g. `installer.rs`, `version.rs`). Tests live in `tests/` as Rust unit/integration tests.
- Version data and configuration are stored under a config directory (see README for details). Installed Terraform/OpenTofu versions are under `versions/<version>`, and the active/default version is tracked in `version`.

Key files to read first

- `src/main.rs` — top-level dispatcher and CLI argument parsing.
- `src/installer.rs` — download, verify, extract logic for installing versions.
- `src/version.rs` — version resolution, remote listing, heuristics for min-required/latest-allowed.
- `README.md` — usage, architecture, and product support documentation.
- `.github/workflows/ci.yml` — CI workflow for build/test.
- `tests/installer_tests.rs` — example unit tests for asset mapping and installer logic.

Conventions and patterns (concrete)

- Command wiring: `src/main.rs` uses `clap` for CLI parsing and dispatches to subcommands. Keep CLI usage simple and familiar (e.g., `tfenv install`, `tfenv use`).
- Rust style: idiomatic error handling with `anyhow`, modular code in `src/`, and clear separation of concerns.
- Version resolution: use environment variables, version files, and heuristics (min-required/latest-allowed) as in the original Bash tfenv.
- Product support: support both Terraform and OpenTofu via a product parameter (see `TFENV_PRODUCT`).
- Verification: download assets, verify SHA256 checksums, and optionally verify PGP signatures if supported upstream.
- Testing: unit tests for mapping, asset URL building, and installer logic; integration tests for end-to-end flows.

Testing and developer workflow

- Build: `cargo build` (requires Rust toolchain and Windows C++ build tools).
- Run tests: `cargo test` (runs all unit/integration tests).
- CI: GitHub Actions workflow in `.github/workflows/ci.yml` runs build and test on push/PR.
- To iterate locally: run the CLI from a working copy, use environment variables to mock config, and run tests for validation.

Integration, external dependencies, and side effects

- Network: installer logic downloads assets from HashiCorp/OpenTofu releases; tests may mock network calls.
- OS differences: asset mapping and extraction logic handle Windows, macOS, and Linux.
- C++ build tools: Windows builds require Visual Studio C++ build tools (see README troubleshooting).

Guidance for common changes

- Add a new subcommand: update `src/main.rs` (add to `clap`), implement logic in a new or existing module in `src/`, and add tests in `tests/`.
- Modify version resolution: update `src/version.rs` and expand tests in `tests/installer_tests.rs`.
- Add tests: add Rust unit/integration tests in `tests/`.

Do NOT assume

- Do not assume a language runtime other than Rust is available. This is a Rust-first repo.
- Do not change the config directory logic without updating CLI and documentation.

Examples (where to look)

- CLI dispatcher: `src/main.rs` uses `clap` to parse and dispatch commands.
- Installer example: `src/installer.rs` handles download, verification, and extraction.
- Version resolution: `src/version.rs` implements heuristics and remote listing.
- Test example: `tests/installer_tests.rs` shows asset mapping and installer logic tests.

If anything here is unclear or you want more detail on a specific file or workflow (for example, how tests mock network calls), tell me which area and I will expand or update this file.
