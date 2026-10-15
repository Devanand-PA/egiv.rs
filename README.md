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