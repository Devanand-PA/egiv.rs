//! egiv — combined image viewer and selector, written in Rust with
//! egui/eframe (a port of the tkiv.py Tk application).

use std::io::Read;
use std::path::{Path, PathBuf};

use egiv_lib::app::{opts_from_selector, opts_from_viewer, Opts};
use egiv_lib::app_main::App;
use egiv_lib::cli::{self};
use egiv_lib::config::{PROGNAME, VERSION};
use egiv_lib::model::{self, FileEntry};

fn splash_usage(prog: &str) {
    eprintln!("usage: {prog} {{img|select}} [OPTIONS] ...\n\n  {prog} img    [OPTIONS] FILES...   image viewer\n  {prog} select [OPTIONS] PATHS...   image selector / dmenu");
}

fn print_viewer_help() {
    println!("{PROGNAME} img {VERSION}");
    println!("usage: egiv img [-abcgHhiopqrvZ0] [-A FRAMERATE] [-e WID] [-G GAMMA] [--geometry GEOMETRY] [-N NAME] [-n NUM] [-S DELAY] [-s MODE] [-T SIZE] [--gallery-rows N] [--gallery-cols N] [--gallery-aspect N] [--sort MODE] [-R] [--no-searchbar] FILES...");
    println!("\nModes: image (full view), gallery (thumbnail grid), list (file list + preview).");
    println!("Tab / Shift+Tab cycles between them; Return in gallery/list opens the image.");
    println!("\nKey bindings:");
    println!("  Tab / Shift+Tab  cycle mode (image -> gallery -> list)");
    println!("  Return           switch image <-> gallery (or list -> image)");
    println!("  Esc              clear filter, or quit if empty");
    println!("  Ctrl+F           show / hide the search bar");
    println!("  Ctrl+Q           quit");
    println!("  F11              fullscreen");
    println!("  Ctrl+B           toggle status bar");
    println!("  Ctrl+R           reload");
    println!("  Ctrl+D           remove current or marked files");
    println!("  Ctrl+N / Ctrl+P  next / previous (respects filter)");
    println!("  Ctrl+H/J/K/L     pan");
    println!("  Ctrl+plus/-/0    zoom the images: picture levels in image mode,");
    println!("                   thumbnail size in gallery mode (hold to repeat)");
    println!("  Ctrl+M           mark    Ctrl+U  unmark all");
    println!("  Ctrl+S           slideshow    Ctrl+Space  toggle animation");
    println!("  Ctrl+E           fit width   Ctrl+W  fit-down");
    println!("  Ctrl+Shift+W     fit         Ctrl+Shift+F  fill");
    println!("  Ctrl+Y / Ctrl+Shift+Y  cycle sort / reverse sort");
    println!("  Ctrl+{{ / Ctrl+}}  gamma down/up");
    println!("  Ctrl+( / Ctrl+)  contrast down/up");
    println!("  Ctrl+1 / Ctrl+2  brightness down/up");
    println!("  Mouse wheel      zoom (image mode)   drag  pan   click edges  navigate");
    println!("\nThe search bar starts hidden.  With it hidden, keys act directly");
    println!("(unmodified letters also trigger their bound actions); Ctrl+F shows");
    println!("the bar so typed text filters the file list.");
    println!("\nConfig file: ~/.config/egiv/config (or $XDG_CONFIG_HOME/egiv/config;");
    println!("the old ~/.config/tkiv.py/config is still read when the new one is absent).");
    println!("  INI-like sections: [keys], [theme], [behavior]  (key_repeat_ms,");
    println!("  key_hold_delay_ms, slideshow_delay, max_load_dim).  Any syntax error");
    println!("  ignores the whole file.");
    println!("\nOptions:");
    println!("  -g/-t, --gallery            start in gallery mode");
    println!("  -l, --list-mode             start in list mode");
    println!("  -T N, --gallery-tile-size   gallery tile size in pixels");
    println!("  --gallery-rows N            target rows visible");
    println!("  --gallery-cols N            target cols visible");
    println!("  --gallery-aspect N          tile width/height ratio");
    println!("  --lazy                      populate file list as it is scanned");
    println!("  --sort MODE                 initial sort: name, mtime, size, none");
    println!("  -R, --sort-reverse          reverse the sort direction");
}

fn print_selector_help() {
    println!("{PROGNAME} select {VERSION}");
    println!("usage: egiv select [OPTIONS] PATHS...");
    println!("\nKey bindings match the viewer. Press Return to accept and print, Esc to cancel. Ctrl+M (or Ctrl+Enter) toggles a mark; when any file is marked, all marked files are printed on accept.");
    println!("Ctrl+Y / Ctrl+Shift+Y  cycle sort / reverse sort");
    println!("Ctrl+F                 show / hide the search bar");
    println!("\nConfig file: ~/.config/egiv/config (or $XDG_CONFIG_HOME/egiv/config).");
}

fn parse_geometry(s: &str) -> Option<egui::Vec2> {
    let m = s.split_once(['x', 'X'])?;
    let w: f32 = m.0.parse().ok()?;
    let h: f32 = m.1.split('+').next()?.parse().ok()?;
    if w > 0.0 && h > 0.0 {
        Some(egui::vec2(w, h))
    } else {
        None
    }
}

fn run_app(
    opts: Opts,
    files: Vec<FileEntry>,
    scan_paths: Vec<String>,
    pre_select: Option<Vec<String>>,
) -> Result<i32, eframe::Error> {
    let title = opts.name.clone().unwrap_or_else(|| PROGNAME.to_string());
    let mut native = eframe::NativeOptions::default();
    let mut vb = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(
            opts.geometry
                .as_deref()
                .and_then(parse_geometry)
                .unwrap_or(egui::vec2(900.0, 700.0)),
        )
        .with_min_inner_size(
            if opts.purpose == egiv_lib::app::Purpose::Select || opts.floating_window {
                egui::vec2(600.0, 400.0)
            } else {
                egui::vec2(100.0, 100.0)
            },
        )
        .with_fullscreen(opts.fullscreen);
    if opts.purpose == egiv_lib::app::Purpose::Select || opts.floating_window {
        // tkiv.py parity: select-mode and --floating-window windows get
        // '-type dialog' (which makes tiling WMs float them) and -topmost.
        // The window is created hidden so egiv_lib::floating::setup_dialog
        // can set _NET_WM_WINDOW_TYPE_DIALOG before it is mapped; it is
        // shown (and sized) on the first frame.
        vb = vb.with_always_on_top().with_visible(false);
    }
    native.viewport = vb;
    eframe::run_native(
        PROGNAME,
        native,
        Box::new(move |cc| Ok(Box::new(App::new(cc, opts, files, scan_paths, pre_select)))),
    )?;
    Ok(0)
}

fn collect_inputs(
    paths: &[String],
    recursive: bool,
    hidden: bool,
    time: bool,
    quiet: bool,
    selector: bool,
) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    for f in paths {
        let p = Path::new(f);
        if !p.exists() {
            if !quiet {
                eprintln!("{PROGNAME}: {f}: No such file or directory");
            }
            continue;
        }
        if p.is_dir() {
            files.extend(
                model::collect_dir(p, recursive, hidden, time)
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned()),
            );
        } else if !selector || model::file_is_image(p) {
            files.push(f.clone());
        }
    }
    (files, Vec::new())
}

fn run_viewer(args: &[String]) -> i32 {
    let o = match cli::parse_viewer_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if o.shared.help {
        print_viewer_help();
        return 0;
    }
    if o.shared.version {
        println!("{PROGNAME} img {VERSION}");
        return 0;
    }
    let pre = match cli::read_pre_select(&o.shared.pre_select, &o.shared.pre_select_file) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let mut file_list = Vec::new();
    let mut scan_paths = Vec::new();
    if o.from_stdin || (o.shared.files.len() == 1 && o.shared.files[0] == "-") {
        let mut data = Vec::new();
        if std::io::stdin().read_to_end(&mut data).is_err() {
            return 1;
        }
        let data = String::from_utf8_lossy(&data);
        for e in if o.using_null {
            data.split('\0').collect::<Vec<_>>()
        } else {
            data.split('\n').collect()
        } {
            if !e.is_empty() {
                file_list.push(e.to_string());
            }
        }
    } else if o.shared.lazy {
        for f in &o.shared.files {
            if Path::new(f).is_dir() {
                scan_paths.push(f.clone());
            } else {
                file_list.push(f.clone());
            }
        }
    } else {
        file_list = collect_inputs(
            &o.shared.files,
            o.shared.recursive,
            o.shared.hidden,
            o.shared.time,
            o.shared.quiet,
            false,
        )
        .0;
    }
    if file_list.is_empty() && scan_paths.is_empty() {
        eprintln!("{PROGNAME}: No valid image file given, aborting");
        return 1;
    }
    let mut opts = opts_from_viewer(&o);
    if o.ss_delay > 0.0 {
        opts.ss_delay_decisec = (o.ss_delay * 10.0) as i64;
    }
    let files = file_list.into_iter().map(FileEntry::new).collect();
    match run_app(opts, files, scan_paths, pre) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{PROGNAME}: cannot open display: {e}");
            1
        }
    }
}

fn lines_from_file_or_arg(
    file: &Option<String>,
    entries: &Option<String>,
    kind: &str,
) -> Result<Vec<String>, String> {
    if file.is_some() && entries.is_some() {
        return Err(format!(
            "Error: both --{kind}-file and --{kind}-entries provided."
        ));
    }
    if let Some(f) = file {
        let text =
            std::fs::read_to_string(f).map_err(|e| format!("Error reading {kind} file: {e}"))?;
        return Ok(text.lines().map(ToString::to_string).collect());
    }
    entries
        .as_ref()
        .map(|s| s.split('\n').map(ToString::to_string).collect())
        .ok_or_else(|| format!("Error: must provide either --{kind}-file or --{kind}-entries"))
}

fn run_selector(args: &[String]) -> i32 {
    let o = match cli::parse_selector_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if o.shared.help {
        print_selector_help();
        return 0;
    }
    if o.shared.version {
        println!("{PROGNAME} select {VERSION}");
        return 0;
    }
    let pre = match cli::read_pre_select(&o.shared.pre_select, &o.shared.pre_select_file) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut entries = Vec::new();
    let mut scan_paths = Vec::new();
    if o.dmenu_mode {
        let labels = match lines_from_file_or_arg(&o.list_file, &o.list_entries, "list") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        let imgs = match lines_from_file_or_arg(&o.image_file, &o.image_entries, "image") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        match model::dmenu_entries(&labels, &imgs) {
            Ok(v) => entries = v,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        }
    } else {
        let paths_owned;
        let paths: &[String] = if o.shared.files.is_empty() {
            paths_owned = vec![".".to_string()];
            &paths_owned
        } else {
            &o.shared.files
        };
        if o.shared.lazy {
            for f in paths {
                let p = Path::new(f);
                if p.is_dir() {
                    scan_paths.push(f.clone());
                } else if p.is_file() && model::file_is_image(p) {
                    entries.push(FileEntry::new(f.clone()));
                }
            }
        } else {
            entries = collect_inputs(
                paths,
                o.shared.recursive,
                o.shared.hidden,
                o.shared.time,
                o.shared.quiet,
                true,
            )
            .0
            .into_iter()
            .map(FileEntry::new)
            .collect();
        }
    }
    if entries.is_empty() && scan_paths.is_empty() {
        eprintln!("No images found.");
        return 1;
    }
    match run_app(opts_from_selector(&o), entries, scan_paths, pre) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{PROGNAME}: cannot open display: {e}");
            1
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let prog = argv
        .first()
        .and_then(|p| {
            PathBuf::from(p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| PROGNAME.into());
    if argv.len() < 2 || argv[1] == "-h" || argv[1] == "--help" {
        splash_usage(&prog);
        std::process::exit(if argv.len() >= 2 { 0 } else { 1 });
    }
    let rest = argv[2..].to_vec();
    let code = match argv[1].as_str() {
        "img" | "view" | "viewer" | "pysxiv" => run_viewer(&rest),
        "select" | "sel" | "selector" | "sel_img" => run_selector(&rest),
        mode => {
            eprintln!("egiv: unknown mode: {mode:?}");
            splash_usage(&prog);
            1
        }
    };
    std::process::exit(code);
}
