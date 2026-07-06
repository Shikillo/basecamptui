# basecamptui

A terminal user interface (TUI) for [Basecamp](https://basecamp.com), built in Rust
with [ratatui](https://ratatui.rs). Browse projects, manage to-dos, read and post to
Campfire chats, log time, and more — without leaving your terminal.

## Requirements

`basecamptui` is a frontend for the official **`basecamp` CLI**. You need it installed
and authenticated first:

- Install the `basecamp` CLI: https://basecamp.com/install-cli
- Authenticate it once (follow the CLI's login flow).

You also need a Rust toolchain to install from source: https://rustup.rs

## Installation

Install directly from GitHub with Cargo:

```sh
cargo install --git https://github.com/shikillo/basecamptui
```

This builds and installs the `basecamptui` binary into `~/.cargo/bin` (make sure that
directory is on your `PATH`).

## Usage

Once installed and with the `basecamp` CLI authenticated, just run:

```sh
basecamptui
```

## Features

- Browse your Basecamp projects
- View and manage to-dos
- Read and post messages in Campfire chats
- Log time entries
- Favorites and local caching for faster navigation

## Development

```sh
git clone https://github.com/shikillo/basecamptui
cd basecamptui
cargo run
```

Local data (cache, favorites, staged entries) is stored under your platform's data
directory (e.g. `~/.local/share/`).

## Credits

Based on and inspired by
[arturtcoelho/basecamp-timesheets-filler](https://github.com/arturtcoelho/basecamp-timesheets-filler/).
