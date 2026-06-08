# release

release is verification, not decoration.

## before the tag

- choose the version and update `Cargo.toml`.
- update `Cargo.lock`.
- update `README.md` if install, config, commands, or limits changed.
- update this checklist if the release path changed.
- start from a clean tree.

## verify

```sh
make check
cargo package --list
cargo publish --dry-run
```

inspect the package list. it should include the readme, license, manifest, lockfile, makefile, release checklist, and source. it should not include `target/`, `work/`, logs, local snapshots, secrets, or editor junk.

## publish

```sh
git tag vX.Y.Z
git push origin main vX.Y.Z
cargo publish
```

## verify the published install

```sh
cargo install me-sh --version X.Y.Z --locked
mesh --version
mesh --help
MESH_CONFIG=/tmp/mesh-empty-config.json mesh status
```

## github binaries

once github release binaries exist, every release must also verify:

- macos arm64 archive installs and runs `mesh --help`.
- macos x64 archive installs and runs `mesh --help`.
- linux x64 archive installs and runs `mesh --help`.
- checksums are attached.
- release notes include install commands and user-visible changes.
- every release asset url works.

## rollback

crates.io versions cannot be overwritten. if a release is broken:

- yank the bad version.
- publish a fixed patch version.
- leave a short note in the github release and changelog.
