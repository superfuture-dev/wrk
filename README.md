# wrk

[![CI](https://github.com/superfuture-dev/wrk/actions/workflows/ci.yml/badge.svg)](https://github.com/superfuture-dev/wrk/actions/workflows/ci.yml)
[![Release](https://github.com/superfuture-dev/wrk/actions/workflows/release.yml/badge.svg)](https://github.com/superfuture-dev/wrk/actions/workflows/release.yml)

`wrk` is a small Rust CLI for keeping a daily markdown work log.

The default path is optimized for low friction:

```sh
wrk "Shipped the release checklist"
```

That writes to a per-day markdown file, grouped by year and month:

```text
<log-dir>/
├── 2026/
│   └── 03/
│       └── 2026-03-04.md
└── latest.md -> 2026/03/2026-03-04.md
```

## Features

- Plain `wrk "message"` logging with no `log` subcommand.
- Appends to the existing day file instead of creating duplicates.
- `HH:MM` timestamps on every log entry.
- Markdown file structure with a protected `## Notes` section.
- Optional project and activity type fields.
- Configurable log directory, default type, allowed types, default project, and editor via `~/.wrkrc`.
- Multiline entries from stdin or interactive entry mode.
- GitHub-style emoji shortcode expansion like `:rocket:` and `:taco:`.
- Read commands for `day`, `week`, `month`, `year`, and project filtering.
- Search, amend, edit, and doctor commands.

## Install

Build locally:

```sh
cargo build --release
```

The binary will be at:

```text
target/release/wrk
```

## Development

Install local hooks:

```sh
python -m pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push
```

Useful targets:

```sh
make fmt
make check
make package
make package-check
```

Cross-target packaging targets are also available:

```sh
make package-linux
make package-macos
make package-windows
```

These require the corresponding Rust target toolchains to be installed first if you are cross-compiling.
On Windows, if your `make` environment does not expose `python3`, run `make package PYTHON=python`.

## Usage

### Log a single entry

```sh
wrk "Investigated flaky integration test"
wrk -p api "Triaged deploy issue"
wrk -p api -t build "Cut release candidate"
```

Entry format in the file:

```markdown
- 09:15 api:note Investigated flaky integration test
```

If no project is set, the entry uses an empty project field:

```markdown
- 09:15 :note Investigated flaky integration test
```

### Pipe text from stdin

The first line becomes the top-level bullet. Additional lines are stored as indented continuation lines.

```sh
printf 'Built release\nNeed to follow up on docs\n' | wrk -p cli
```

That becomes:

```markdown
- 09:15 cli:note Built release
  Need to follow up on docs
```

### Interactive mode

Run `wrk` with no message to enter interactive entry mode:

```sh
wrk
```

Behavior:

- `Ctrl-D` on an empty prompt saves the entry.
- `Ctrl-C` cancels the entry.
- The first line becomes the main bullet.
- Additional lines become indented continuation lines.

### Read logs

```sh
wrk day
wrk week
wrk month
wrk year
```

Use a specific anchor date:

```sh
wrk week --date 2026-03-04
wrk month --date 2026-03-01
wrk year --date 2026-01-01
```

Notes:

- `week` means the work week, Monday through Friday.
- By default only top-level bullet entries are shown.
- Use `-a` or `--all` to include indented continuation lines.

Sort grouped output by project:

```sh
wrk day --sort project
wrk week --sort project -a
```

### Search and filter

Search uses a regex pattern:

```sh
wrk search release
wrk search 'flaky|timeout'
```

Show all entries for one project:

```sh
wrk project api
```

### Edit and amend

Open today’s file in your configured editor:

```sh
wrk edit
wrk edit --date 2026-03-04
```

Amend the last entry from today without changing its timestamp:

```sh
wrk amend -p api -t note "Clarified release notes"
printf 'Clarified release notes\nAdded rollout details\n' | wrk amend -p api
```

### Emoji reference

List emojis by picker-style section:

```sh
wrk emoji food-and-drink
wrk emoji objects
wrk emoji smileys-and-emotion
```

### Validate the repository

```sh
wrk doctor
```

`doctor` checks log file structure and reports formatting problems, including section layout, entry syntax, type validation, and the `latest.md` symlink.

## File format

Each day file looks like this:

```markdown
# 2026-03-04

## Work Log

- 09:15 api:note Investigated flaky integration test
  Reproduced it locally
- 11:30 :note Reviewed rollout plan

## Notes

Free-form markdown notes live here.
```

`wrk` only manages the `## Work Log` section. The `## Notes` section is preserved so you can edit it freely.

## Configuration

Configuration is read from `~/.wrkrc` as TOML.

Example:

```toml
log_dir = "~/wrk"
default_project = "api"
default_type = "note"
types = ["note", "build", "meeting", "review"]
editor = "nvim"
```

### Options

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `log_dir` | string | `~/wrk` | Root directory that holds the year/month/day markdown tree. |
| `default_project` | string | unset | Project value used when `-p/--project` is not supplied. Must not contain whitespace or `:`. |
| `default_type` | string | `note` | Default activity type for new entries. Must be present in `types`. Must not contain whitespace or `:`. |
| `types` | array of strings | `["note"]` | Allowed activity types for logging and doctor validation. Each type must not contain whitespace or `:`. |
| `editor` | string | `$VISUAL` or `$EDITOR` | Editor command used by `wrk edit`. |

### Config precedence

- `--config <path>` overrides the config file location.
- `--log-dir <path>` overrides `log_dir` from `~/.wrkrc`.
- `editor` falls back to `$VISUAL`, then `$EDITOR`, if not set in config.

## Help

```sh
wrk --help
wrk day --help
wrk amend --help
```

## CI and Releases

The repo includes:

- `pre-commit` hooks for whitespace, TOML/YAML validation, `cargo fmt`, `cargo clippy`, and `cargo test`.
- A CI workflow that runs `pre-commit`, `cargo package`, and cross-platform build/test jobs on Linux, macOS, and Windows.
- A release workflow that triggers on tags matching `v*`, packages native archives for Linux, macOS, and Windows, generates checksums, and uploads everything to a GitHub release.

## Current behavior notes

- If you want to log a message that is exactly the same as a subcommand name, use `--` to force log mode:

```sh
wrk -- day
wrk -- doctor
```

- Search matches against parsed work-log entries and their indented continuation lines.
- The `project` field is optional, but the `type` field is always present.
