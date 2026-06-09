# sftp-rust

A command-line file-transfer tool built on SFTP. It is a self-learning Rust
project whose long-term goal is to reverse-engineer and recreate `scp`. Modern
`scp` (OpenSSH 9.0+) runs over the SFTP protocol, so this tool mirrors its
architecture.

## Status

Uploads only (local to remote). Implemented: single-file and recursive
transfers, key and password authentication, the `scp` basename convention,
permission/timestamp preservation, chunked streaming for large files, and an
indented per-file progress tree. Not yet implemented: downloads (remote to
local) and the legacy rcp protocol.

## Build

```sh
cargo build --release
```

The binary is `target/release/sftp-rust`.

## Usage

```sh
sftp-rust [OPTIONS] <ORIGIN> <DESTINATION>
```

`ORIGIN` is a local path. `DESTINATION` is `user@host:path`.

### Options

- `-P, --port <PORT>` — SSH port (default: 22).
- `-i, --identity <KEY_PATH>` — path to a private key. If omitted, the tool
  tries `~/.ssh/id_ed25519`, then `~/.ssh/id_rsa`, then prompts for a password.
- `-r, --recursive` — transfer directories.
- `-p, --preserve` — preserve permissions and modification/access times.

### Examples

Send a file:

```sh
sftp-rust ./report.pdf user@host:/srv/docs/report.pdf
```

Send a directory, preserving metadata, on a non-default port:

```sh
sftp-rust -r -p -P 2222 ./project user@host:/tmp
```

As with `scp`, when the destination is an existing directory the source's
basename is appended, so the command above creates `/tmp/project`.

## How it works

A transfer runs in two phases. First, `build_tree` walks the local filesystem
into a flat list of nodes (paths and sizes) without touching the network. Then
`transfer_tree` iterates that list: it creates directories, streams files in
fixed-size chunks, and applies directory metadata in a final pass. Files report
progress through a callback so rendering stays separate from transfer.

## Dependencies

`ssh2`, `clap`, `rpassword`.
