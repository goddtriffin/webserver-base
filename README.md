# webserver-base

[![Version](https://img.shields.io/crates/v/webserver-base)](https://crates.io/crates/webserver-base)
[![Docs](https://docs.rs/webserver-base/badge.svg)](https://docs.rs/webserver-base)
[![License](https://img.shields.io/crates/l/webserver-base)](https://crates.io/crates/webserver-base)

A Rust library which contains shared logic for all of my webserver projects.

## Features

- common settings
- HTML templates
- integration: Axum + Plausible Analytics

## Developers

**Project is under active maintenance - even if there are no recent commits! Please submit an issue / bug request if the library needs updating for any reason!**

### Commands

- `make lint`
    - Lints the codebase via `cargo fmt`.
- `make test`
    - Tests the codebase via:
        - `cargo fmt`
        - `cargo check`
        - `cargo clippy` (with insanely strict defaults)
        - `cargo test`.

## Credits

Made with 🤬 and 🥲 by [Todd Everett Griffin](https://www.toddgriffin.me/).

`webserver-base` is open source under the [MIT License](https://github.com/goddtriffin/webserver-base/blob/main/LICENSE).