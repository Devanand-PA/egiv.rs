# egiv

**egiv** (previously `tkiv.rs`) is a combined image viewer and image selector
written in Rust with [egui/eframe](https://www.egui.rs/). It is a port of the
Tk application `tkiv.py`, combining an **nsxiv-like image viewer**, a
**thumbnail gallery**, and a **dmenu-like image selector** in one program with
three switchable modes.

## Modes

| Mode | What it shows | Switch |
|---|---|---|
| **image** | the current picture, zoomable and pannable | `Tab` |
| **gallery** | thumbnail grid; thumbnails resize with the zoom keys | `Tab` |
| **list** | file list with a live preview pane | `Tab` |

`Return` in gallery/list opens the selected image. Click the left/right third
of the image in image mode to navigate.

## Usage

```bash
egiv img [OPTIONS] FILES...      # image viewer
egiv select [OPTIONS] PATHS...   # image selector (prints choice(s) to stdout)
```

Run `egiv img --help` or `egiv select --help` for the full option list.

### Common viewer flags

| Flag | Meaning |
|---|---|
| `-g/-t, --gallery` | start in gallery mode |
| `-l, --list-mode` | start in list mode |
| `-T N, --gallery-tile-size` | gallery tile size in pixels |
| `--gallery-rows N` / `--gallery-cols N` | target rows/columns visible |
| `--gallery-aspect N` | tile width/height ratio |
| `-s MODE` | initial scale mode (`z` zoom, `w` fit-width, …) |
| `-z percent` / `-Z` | initial zoom level / 100% zoom |
| `-G GAMMA` | initial gamma |
| `-S SECONDS` | slideshow delay |
| `-f` | start fullscreen |
| `--geometry WxH` | window size |
| `-n NUM, --start-at` | start at the N-th file |
| `--sort MODE` / `-R, --sort-reverse` | initial sort (`name`, `mtime`, `size`, `none`) |
| `-r` / `-H` | recurse into directories / show hidden files |
| `--lazy` | populate the file list while scanning |
| `--no-searchbar` | accepted for compatibility (the bar now always starts hidden) |
| `-N NAME, --name` | window title |
| `-q, --quiet` | suppress messages |
| `-v, --version` | print version |
| `-h, --help` | help |

Selector-only flags include `--pre-select`, `--pre-select-file` and
`-o/--stdout` for integration into scripts and pipelines.

## Key bindings

| Keys | Action |
|---|---|
| `Tab` / `Shift+Tab` | cycle mode (image → gallery → list) |
| `Return` | open selection / toggle image ↔ gallery |
| `Esc` | clear filter, or quit if empty |
| `Ctrl+F` | show / hide the search bar (starts hidden) |
| `Ctrl+Q` | quit |
| `F11` | fullscreen |
| `Ctrl+B` | toggle status bar |
| `Ctrl+R` | reload |
| `Ctrl+D` | remove current or marked files |
| `Ctrl+N` / `Ctrl+P` | next / previous (respects filter) |
| `Ctrl+H/J/K/L` | pan |
| `Ctrl+plus` / `Ctrl+minus` / `Ctrl+0` | zoom in / out / 100% (image mode: picture levels; gallery mode: thumbnail size) — hold to repeat |
| `Ctrl+M` / `Ctrl+U` | mark / unmark all |
| `Ctrl+S` | slideshow |
| `Ctrl+Space` | toggle animation |
| `Ctrl+E` / `Ctrl+W` | fit width / fit down |
| `Ctrl+Shift+W` / `Ctrl+Shift+F` | fit / fill |
| `Ctrl+Y` / `Ctrl+Shift+Y` | cycle sort / reverse sort |
| `Ctrl+{` / `Ctrl+}` | gamma down / up |
| `Ctrl+(` / `Ctrl+)` | contrast down / up |
| `Ctrl+1` / `Ctrl+2` | brightness down / up |
| `Ctrl+I` | toggle antialias |
| Mouse wheel | zoom (image mode) |
| Drag / click edges | pan / navigate (image mode) |

In **selector** mode, `Return` prints the chosen file(s) — all marked files if
any are marked — and `Esc` cancels.

## Config file

`~/.config/egiv/config` (or `$XDG_CONFIG_HOME/egiv/config`; the old
`~/.config/tkiv.py/config` is still read when the new location is absent).
INI-like, all-or-nothing: any syntax error discards the whole file.

```ini
[keys]
; action = Tk-style binding list
zoom_in = ["Ctrl+Plus"]
zoom_out = ["Ctrl+Minus"]

[theme]
bg_primary = #2e3440
accent = #88c0d0

[behavior]
key_repeat_ms = 30        ; repeat pace once a held key starts repeating
key_hold_delay_ms = 300   ; how long a key must be held before repeating
slideshow_delay = 5       ; seconds
max_load_dim = 4096       ; cap on decoded image dimension
```

Every action name in the source (`ACTION_NAMES`) can be rebound; run
`egiv img --help` for the default key list.

## Building

```bash
cargo build --release
```

The binary is `target/release/egiv`.

### Optional libvips accelerator

By default egiv builds with the `vips` feature, which uses libvips to speed up
loading and thumbnailing of large images (and adds codec coverage like
HEIC/JPEG2000 when the host libvips supports them). This requires the libvips
and glib development packages at build time, and the libraries at runtime:

```bash
# Debian/Ubuntu
sudo apt install libvips-dev
# macOS
brew install vips
```

Build without it — the pure-Rust `image` fallback is used instead:

```bash
cargo build --release --no-default-features
```

Prebuilt release binaries are built with `--no-default-features` so they have
no libvips/glib runtime dependency.

## License

MIT — see [LICENSE](LICENSE).
