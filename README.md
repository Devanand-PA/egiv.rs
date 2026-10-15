# tkiv

Combined image viewer (nsxiv-like) and image selector (dmenu-like) — Rust port of tkiv.py

## Description

tkiv is a terminal-based image viewer and selector that allows you to browse and select images from the command line. It features:

- Image viewing with zoom and pan support
- Image selection with dmenu-style interface
- Support for multiple image formats (PNG, JPEG, WebP, GIF, BMP, ICO, TIFF, etc.)
- Cross-platform support (Linux, macOS, Windows)

## Building

```bash
cargo build --release
```

### Optional libvips accelerator

By default tkiv builds with the `vips` feature, which uses libvips to speed up
loading and thumbnailing of large images (and adds codec coverage like
HEIC/JPEG2000 when the host libvips supports them). This requires the libvips
and glib development packages at build time, and the libraries at runtime:

```bash
# Debian/Ubuntu
sudo apt install libvips-dev
# macOS
brew install vips
```

If you don't need it, build without it — the pure-Rust `image` fallback is
used instead:

```bash
cargo build --release --no-default-features
```

Prebuilt release binaries are built with `--no-default-features` so they have
no libvips/glib runtime dependency.

## Running

```bash
cargo run
```

or

```bash
./target/release/tkiv
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.