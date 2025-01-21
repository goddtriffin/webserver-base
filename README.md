# webserver-base

[![Version](https://img.shields.io/crates/v/webserver-base)](https://crates.io/crates/webserver-base)
[![Docs](https://docs.rs/webserver-base/badge.svg)](https://docs.rs/webserver-base)
[![JSR](https://jsr.io/badges/@todd/webserver-base)](https://jsr.io/@todd/webserver-base)


A Rust library which contains shared logic for all of my webserver projects.

## Features

- common settings
- HTML templates
- integration: Axum + Plausible Analytics
- `POST`ing frontend Typescript `Error`s to a Rust API endpoint
- Deno script to transpile+bundle `.ts` -> `.js`

## Developers

**Project is under active maintenance - even if there are no recent commits! Please submit an issue / bug request if the library needs updating for any reason!**

### Commands

- `make lint`
- `make test`
- `make fix`

## Credits

Made with 🤬 and 🥲 by [Todd Everett Griffin](https://www.toddgriffin.me/).
