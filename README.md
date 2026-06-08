# me-sh

command-line tools for me.sh.

`mesh` lets you inspect contacts, groups, activity, routes, and local snapshots from the terminal. reads are easy to run. writes need `--yes`, and most write paths can print a `--dry-run` plan first.

status: alpha. the tool works locally and has unit coverage, but the release path is still being built.

## limits

- requires a me.sh account and api access.
- tested locally on macos. linux and windows are not claimed until ci/release builds cover them.
- `mesh login` binds a local oauth callback on `127.0.0.1:6374`.
- `mesh login --open` uses the macos `open` command. without it, copy the printed url into a browser and paste the callback url or code.
- tokens are stored in `~/.config/mesh.json` with `0600` permissions on unix. older `mesh-cli.json` and `clay-cli.json` configs are migrated when read.
- the observed me.sh route set does not expose hard delete for contacts or groups.
- notes are create/read through the observed routes. events, emails, and reminders are read-only.
- search has an observed server-side `limit <= 1000` cap. `offset`, `page`, and `skip` did not paginate in live checks; the tool uses `exclude_contact_ids` for full exports.

## install

published releases install from crates.io:

```sh
cargo install me-sh --locked
```

until the first crates.io release, install from this checkout:

```sh
cargo install --path . --locked
```

the package is `me-sh`; the installed command is `mesh`.

## quick start

```sh
mesh login
mesh doctor
mesh contacts:search --limit 10 --format table
```

use `--help` anywhere:

```sh
mesh --help
mesh contacts:export --help
mesh snapshot:create --help
```

## common work

```sh
mesh status
mesh whoami
mesh config:path
mesh config:show
```

```sh
mesh contacts:count
mesh contacts:search --limit 25 --format table
mesh contacts:export --all --format jsonl --output contacts.jsonl --resume
mesh contacts:resolve --name "ada" --one --format compact-json
mesh contacts:profile --contact-ids 123 --format table
mesh contacts:activity --contact-ids 123 --sections notes,events,emails --start 2026-01-01 --end 2026-12-31
```

```sh
mesh groups
mesh groups:find --query investors --format table
mesh groups:members --group-ids starred --include-fields email,phone --format table
mesh groups:compare --left-query investors --right-group-id starred --format table
```

```sh
mesh snapshot:create --dir before-edit
mesh snapshot:verify --dir before-edit
mesh snapshot:diff --old before-edit --new after-edit --details
mesh snapshot:pack --dir before-edit --archive before-edit.tar.zst
mesh snapshot:verify-archive --archive before-edit.tar.zst --require-index
```

## writes

write commands require `--yes` unless `--dry-run` is used.

```sh
mesh contacts:create --first-name Ada --last-name Lovelace --email ada@example.invalid --dry-run
mesh contacts:update --contact-id 123 --title "designer" --dry-run
mesh groups:sync --group-id 343852 --input desired-members.txt --mode add-only --dry-run
mesh plan:audit --input write-plan.json --max-writes 10 --strict --format table
```

write paths include contact create/update/archive/restore, group create/update/sync, note creation, bulk apply commands, merge, and snapshot restore.

## output

global output controls:

```sh
mesh contacts:search --limit 10 --format table
mesh contacts:search --limit 10 --output contacts.json
mesh contacts:search --timeout 60 --retries 5
```

formats:

```text
json
compact-json
jsonl
csv
tsv
table
```

long snapshot and archive commands draw progress on stderr only when stderr is a terminal. set `MESH_NO_PROGRESS=1` to silence it. `MESHX_NO_PROGRESS=1` still works for older setups.

## config

the default config path is:

```text
~/.config/mesh.json
```

environment variables:

```text
MESH_CONFIG       config file path
MESH_ACCESS_TOKEN use this access token instead of stored auth
MESH_API_BASE     api base url, default https://api.me.sh
MESH_MCP_BASE     mcp tool base url, default https://mcp.me.sh
MESH_NO_PROGRESS  disable terminal progress lines
```

`mesh config:show` redacts access and refresh tokens. diagnostics do not print tokens.

## generated files

safe to delete after you are done with a local run:

```text
target/
work/
*.log
```

snapshot packaging may create `.meshx-index/` sidecars and `.meshx-package.json` metadata inside archives. export resume writes `*.meshx-state.json`. those names are part of the current local file format.

## development

```sh
git clone <repo-url>
cd me-sh
cargo build
make check
```

focused commands:

```sh
make fmt
make lint
make test
make build
make smoke
```

`make check` runs format check, clippy, tests, release build, and binary smoke checks.

ci runs the same command on pull requests and pushes to `main`.

## release

release is verification, not just a tag.

```sh
make check
cargo publish --dry-run
```

then tag, publish, and test the published install:

```sh
git tag v0.2.0
git push origin main v0.2.0
cargo publish
cargo install me-sh --version 0.2.0 --locked
mesh --version
mesh --help
```

see `RELEASE.md` for the checklist.

## license

copyright (c) 2026 sigkillme0.

licensed under gpl-3.0-or-later. see `LICENSE`.
