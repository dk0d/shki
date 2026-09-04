---
title: Installation
description: Install shki as a prebuilt binary, with cargo binstall, or from source.
slug: 0.10.10/getting-started/installation
---

Install shki as a prebuilt binary — no Rust toolchain needed. The script
detects your platform and downloads the matching build. On the latest docs the
commands below install the newest release; on versioned docs they install the
release the docs describe.

## macOS and Linux

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/dk0d/shki/releases/latest/download/shki-installer.sh | sh
```

## Windows (PowerShell)

```powershell
irm https://github.com/dk0d/shki/releases/latest/download/shki-installer.ps1 | iex
```

Both install `shki` into `$CARGO_HOME/bin` (`~/.cargo/bin` by default — make sure
it is on your `PATH`), along with an updater:

```bash
shki-update   # upgrade in place to the newest release
```

To install a specific version, use `download/v<version>` as the path segment,
e.g. `.../releases/download/v0.10.10/shki-installer.sh`.

## With `cargo binstall`

`shki` is not published to crates.io, so point binstall at the repository:

```bash
cargo binstall --git https://github.com/dk0d/shki shki
```

This fetches the same prebuilt binary as the scripts above instead of compiling.
There is no `@latest` specifier — binstall expects a semver requirement, and with
`--git` it reads the version from the repository, which is already the newest
release. Use `shki@*` to state "any version" explicitly, or `shki@0.9.6` to pin.
Unlike the install scripts, binstall does not install `shki-update`.

## From source

Prebuilt binaries cover macOS (Apple Silicon and Intel), Linux (x86\_64 and
aarch64, glibc), and Windows (x86\_64). Anywhere else, build from source:

```bash
cargo install --git https://github.com/dk0d/shki
```

Or locally:

```bash
git clone https://github.com/dk0d/shki
cd shki
cargo install --path .
```
