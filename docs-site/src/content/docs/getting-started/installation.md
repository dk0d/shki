---
title: Installation
description: Install shki as a prebuilt binary, with cargo binstall, or from source.
---

Install the latest release as a prebuilt binary — no Rust toolchain needed. The
script detects your platform and downloads the matching build.

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

To pin a version, replace `latest/download` with `download/v<version>`, e.g.
`.../releases/download/v0.9.6/shki-installer.sh`.

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

Prebuilt binaries cover macOS (Apple Silicon and Intel), Linux (x86_64 and
aarch64, glibc), and Windows (x86_64). Anywhere else, build from source:

```bash
cargo install --git https://github.com/dk0d/shki
```

Or locally:

```bash
git clone https://github.com/dk0d/shki
cd shki
cargo install --path .
```
