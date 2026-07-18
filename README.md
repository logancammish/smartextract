# SmartExtract

A friendly Rust CLI for extracting ZIP, 7z, RAR, ZIPX, and common TAR archives. It chooses a sensible destination automatically, supports drag-and-drop paths in its interactive prompt, and uses 7-Zip's mature archive engine.

SmartExtract provides a colorful app-like terminal interface, animated extraction status, elapsed time, archive metadata, and a useful completion summary. Color and animation are enabled only for an interactive terminal, and `NO_COLOR=1` or `--no-animation` can disable either effect. The optimized binary remains well below 2 MB and has no Rust package dependencies.

## Install

SmartExtract needs [Rust](https://rustup.rs), 7-Zip (`p7zip-full` on Debian/Ubuntu), and `unar` for reliable RAR extraction. It automatically chooses the best available engine for each format.

```sh
curl -fsSL https://raw.githubusercontent.com/logancammish/smartextract/main/install.sh | sh
```

The installer builds the latest `main` branch and places the binary in `~/.local/bin`. Override that with `SMARTEXTRACT_INSTALL_DIR=/your/bin`.

## Use

```sh
smartextract holiday.zip
smartextract archive.7z --output ./files
smartextract list package.rar
smartextract test package.rar
```

Run `smartextract` with no arguments for the guided prompt, or `smartextract --help` for all options.

## Build from source

```sh
cargo build --release
```

The binary will be at `target/release/smartextract`.
