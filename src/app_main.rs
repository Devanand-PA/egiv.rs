//! eframe::App implementation for the egiv GUI.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use egui::{Color32, Key, Sense, TextureHandle, TextureOptions, Vec2};

use crate::app::*;
use crate::config::{load_config, PROGNAME};
use crate::images::{self, Frames};
use crate::keys;
use crate::model::{self, FileEntry, SortMode, FF_MARK, FF_WARN};

mod ui;

const IMG_WORKERS_MAX: usize = 20;

type Job = Box<dyn FnOnce() + Send + 'static>;
static JOB_QUEUE: OnceLock<Sender<Job>> = OnceLock::new();

fn job_queue() -> Sender<Job> {
    JOB_QUEUE
        .get_or_init(|| {
            let (tx, rx) = channel::<Job>();
            let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
            for worker in 0..IMG_WORKERS_MAX {
                let rx = std::sync::Arc::clone(&rx);
                std::thread::Builder::new()
                    .name(format!("egiv-img-{worker}"))
                    .spawn(move || {
                        let rx = std::sync::Arc::clone(&rx);
                        loop {
                            let job = rx.lock().unwrap().recv();
                            let Ok(job) = job else { break };
                            job();
                        }
                    })
                    .expect("failed to start egiv image worker");
            }
            tx
        })
        .clone()
}

pub struct App {
    opts: Opts,
    theme: Theme,
    cfg_behavior: HashMap<String, i64>,
    mode: Mode,
    files: Vec<FileEntry>,
    fileidx: usize,
    markidx: usize,
    alternate: usize,
    visible: Vec<usize>,
    filter_text: String,
    no_searchbar: bool,
    show_bar: bool,

    // sorting
    sort_mode: SortMode,
    sort_reverse: bool,

    // image state
    frames: Vec<image::RgbaImage>,
    orig_frames: Vec<image::RgbaImage>, // before rotate/flip
    delays: Vec<u32>,
    img_sel: usize,
    img_animate: bool,
    img_w: u32,
    img_h: u32,
    zoom: f64,
    scale_mode: ScaleMode,
    pan: Vec2,
    gamma: i32,
    brightness: i32,
    contrast: i32,
    anti_alias: bool,
    alpha_layer: bool,
    next_frame_at: Option<Instant>,

    // slideshow
    ss_on: bool,
    ss_delay_ds: i64, // deciseconds
    next_slide_at: Option<Instant>,

    // async
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    load_token: u64,
    files_gen: u64,
    in_flight: HashSet<usize>,
    pending_load: Option<(usize, u64)>,
    prefetch_keys: Vec<usize>,
    prefetch: HashMap<usize, Frames>,
    origins: HashMap<String, (u32, u32)>,
    mtimes: HashMap<usize, u128>,
    last_autoreload: Instant,
    /// Timestamp of the last clipboard-copy action.  egui-winit delivers
    /// Ctrl+C as `Event::Copy` without the repeat flag a regular Key event
    /// carries, so holding the chord would re-spawn the clipboard helper on
    /// every autorepeat event; this cooldown damps that.
    last_copy_at: Option<Instant>,

    // gallery
    thumbs: HashMap<usize, TextureHandle>,
    thumb_in_flight: HashSet<usize>,
    gallery_tile_override: Option<f32>,
    gallery_aspect_opt: Option<f32>,
    gallery_aspect_cache: Option<f32>,
    aspect_pending: bool,
    aspect_gen: u64,
    gallery_scroll_to: Option<usize>,
    gallery_cols: usize,
    /// Vertical scroll offset of the gallery ScrollArea as of the end of
    /// the previous frame; used to compute make-visible scrolling like
    /// tkiv.py _apply_gallery_scroll.
    gallery_last_offset_y: f32,

    // list mode
    list_preview: Option<TextureHandle>,
    list_pending: i64,
    /// When set, scroll the list so this file becomes visible (tkiv.py
    /// calls listbox.see(pos) after every selection change).
    list_scroll_to: Option<usize>,
    /// Cached pixel width of the widest visible label, keyed by
    /// (files_gen, file count, filter) so it is only recomputed when the
    /// list actually changes.
    list_label_cache: (u64, usize, String, f32),

    // textures
    img_tex: Option<TextureHandle>,
    img_tex_key: Option<String>,

    // lazy scan
    lazy_scan_active: bool,

    // window
    fullscreen: bool,
    quitting: bool,

    // key bindings (action -> bindings), pre-resolved
    keymap: Vec<(Binding, Action, bool)>, // binding, action, direct-mode variant flag unused
    keymap_direct: Vec<(Binding, Action, bool)>,

    pending_pan: Option<(f32, f32)>,
    /// Image-mode zoom step requested by Ctrl+plus/minus, applied inside
    /// render_image through the same act_zoom call the mouse wheel uses.
    pending_key_zoom: Option<i64>,
    /// Keys currently held whose actions auto-repeat, mirroring tkiv.py's
    /// hold timers (tkiv.py schedules an after() chain per held binding;
    /// OS auto-repeat events are swallowed in both implementations).
    held: HashMap<String, HeldKey>,
    /// Floating windows are created hidden so the dialog window type can
    /// be set before the WM maps them; shown on the first frame.
    pending_show: bool,
    /// (w, h, x, y) to apply to a floating window on the first frame
    /// (tkiv.py: screen-100 x screen-100 +50+50).
    pending_geometry: Option<(f32, f32, f32, f32)>,
}

/// What a held key re-fires on each timer tick.
#[derive(Clone)]
enum HeldWhat {
    /// A configurable action (next, zoom, gamma, pan, ...).
    Act(Action),
    /// Image-mode zoom step re-fired while a Ctrl+plus/minus chord is
    /// held; goes through pending_key_zoom (the wheel's call site).
    Zoom(i64),
    /// A fixed navigation key; re-resolved per tick so it follows the
    /// current mode, exactly like the lambdas tkiv.py binds.
    Nav(Dir),
    /// PageUp/PageDown.
    Page(i64),
}

struct HeldKey {
    key: egui::Key,
    what: HeldWhat,
    interval: Duration,
    next_fire: Instant,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        opts: Opts,
        mut files: Vec<FileEntry>,
        scan_paths: Vec<String>,
        pre_select: Option<Vec<String>>,
    ) -> App {
        let cfg = load_config();
        let behavior = cfg.merged_behavior();
        let theme = Theme::from_config(&cfg);

        // key bindings
        let merged_keys = cfg.merged_keys();
        let mut keymap = Vec::new();
        let mut keymap_direct = Vec::new();
        for (name, specs) in &merged_keys {
            let Some((_, action)) = ACTION_NAMES.iter().find(|(n, _)| n == name) else {
                continue;
            };
            for spec in specs {
                if let Some(seq) = keys::parse_key_spec(spec) {
                    if let Some(b) = Binding::from_tk_seq(&seq) {
                        keymap.push((b.clone(), *action, false));
                        // Direct (searchbar-hidden) mode keeps the real
                        // binding as well, so Ctrl chords keep working when
                        // the bar is hidden; to_direct_key adds the
                        // plain-key variant on top (tkiv.py's direct-key
                        // UX).
                        keymap_direct.push((b, *action, true));
                    }
                    if let Some(dk) = keys::to_direct_key(&seq) {
                        if let Some(b) = Binding::from_tk_seq(&dk) {
                            keymap_direct.push((b, *action, true));
                        }
                    }
                }
            }
        }

        // pre-select
        if let Some(labels) = pre_select {
            let target: HashSet<String> = labels.into_iter().collect();
            let mut matched: HashSet<String> = HashSet::new();
            for f in files.iter_mut() {
                if target.contains(&f.label) && !matched.contains(&f.label) {
                    f.flags |= FF_MARK;
                    matched.insert(f.label.clone());
                }
            }
        }

        if files.is_empty() && !opts.lazy {
            eprintln!("no files");
            std::process::exit(1);
        }

        let fileidx = if files.is_empty() {
            0
        } else {
            (opts.start_at.max(1) - 1)
                .min(files.len() as i64 - 1)
                .max(0) as usize
        };
        let mode = if opts.thumb_mode {
            Mode::Gallery
        } else if opts.list_mode {
            Mode::List
        } else {
            Mode::Image
        };

        let (tx, rx) = channel::<Msg>();
        // Floating windows (select mode / --floating-window): tkiv.py sets
        // `-type dialog` and `-topmost` and sizes to screen-100 +50+50.
        // The window is created hidden (see run_app) so the dialog window
        // type is registered BEFORE the window manager maps it.
        let (pending_show, pending_geometry) =
            if opts.purpose == Purpose::Select || opts.floating_window {
                match crate::floating::setup_dialog(cc) {
                    Some(screen) => (true, Some(crate::floating::floating_geometry(screen))),
                    // Wayland or no libX11: still show the hidden window,
                    // the dialog type just cannot be set (like Tk).
                    None => (true, None),
                }
            } else {
                (false, None)
            };
        let ss_delay_ds = if opts.ss_on {
            opts.ss_delay_decisec
        } else {
            behavior["slideshow_delay"] * 10
        };
        // CLI -S delay comes through ss_on/ss_delay_decisec already
        let ss_delay_ds = if opts.ss_delay_decisec > 0 {
            opts.ss_delay_decisec
        } else {
            ss_delay_ds
        };

        let mut app = App {
            mode,
            fileidx,
            markidx: 0,
            alternate: 0,
            visible: (0..files.len()).collect(),
            filter_text: String::new(),
            // The search/filter bar starts hidden; Ctrl+F toggles it.
            no_searchbar: true,
            show_bar: opts.show_bar,
            sort_mode: opts.sort,
            sort_reverse: opts.sort_reverse,
            frames: vec![],
            orig_frames: vec![],
            delays: vec![],
            img_sel: 0,
            img_animate: opts.animate,
            img_w: 0,
            img_h: 0,
            zoom: opts.zoom_start,
            scale_mode: opts.scale_mode,
            pan: Vec2::ZERO,
            gamma: opts.gamma,
            brightness: 0,
            contrast: 0,
            anti_alias: opts.anti_alias,
            alpha_layer: opts.alpha_layer,
            next_frame_at: None,
            ss_on: opts.ss_on,
            ss_delay_ds,
            next_slide_at: None,
            tx,
            rx,
            load_token: 0,
            files_gen: 0,
            in_flight: HashSet::new(),
            pending_load: None,
            prefetch_keys: vec![],
            prefetch: HashMap::new(),
            origins: HashMap::new(),
            mtimes: HashMap::new(),
            last_autoreload: Instant::now(),
            last_copy_at: None,
            thumbs: HashMap::new(),
            thumb_in_flight: HashSet::new(),
            gallery_tile_override: opts.thumb_size.filter(|t| *t > 0).map(|t| t as f32),
            gallery_aspect_opt: opts.gallery_aspect.filter(|a| *a > 0.0).map(|a| a as f32),
            gallery_aspect_cache: None,
            aspect_pending: false,
            aspect_gen: 0,
            gallery_scroll_to: None,
            gallery_cols: 1,
            gallery_last_offset_y: 0.0,
            list_preview: None,
            list_pending: -1,
            list_scroll_to: None,
            list_label_cache: (0, 0, String::new(), 0.0),
            img_tex: None,
            img_tex_key: None,
            lazy_scan_active: false,
            fullscreen: opts.fullscreen,
            quitting: false,
            keymap,
            keymap_direct,
            pending_pan: None,
            pending_key_zoom: None,
            held: HashMap::new(),
            pending_show,
            pending_geometry,
            opts,
            cfg_behavior: behavior,
            theme,
            files,
        };

        if !scan_paths.is_empty() && app.opts.lazy {
            app.start_lazy_scan(scan_paths);
        }
        if app.sort_mode != SortMode::None {
            app.apply_sort(None, None, false);
        }
        app.persist_index();
        if !app.files.is_empty() {
            match app.mode {
                Mode::Image => {
                    app.load_image_async(app.fileidx);
                }
                Mode::Gallery => {
                    app.gallery_scroll_to = Some(app.fileidx);
                }
                Mode::List => {
                    app.list_pending = -2; // trigger preview load
                }
            }
        }
        let _ = cc;
        app
    }

    // ------------------------------------------------------------ lazy scan

    fn start_lazy_scan(&mut self, paths: Vec<String>) {
        self.lazy_scan_active = true;
        let tx = self.tx.clone();
        let recursive = self.opts.recursive;
        let sort_time = self.opts.time;
        std::thread::Builder::new()
            .name("egiv-scan".into())
            .spawn(move || {
                let mut seen: HashSet<String> = HashSet::new();
                let mut batch: Vec<String> = Vec::new();
                for path in &paths {
                    let p = PathBuf::from(shellexpand(path));
                    let rp = p.canonicalize().unwrap_or(p.clone());
                    if !rp.exists() {
                        continue;
                    }
                    if rp.is_dir() {
                        let mut entries = if recursive {
                            let mut v = vec![];
                            collect_recursive(&rp, &mut v);
                            v
                        } else {
                            std::fs::read_dir(&rp)
                                .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
                                .unwrap_or_default()
                        };
                        if sort_time {
                            entries.sort_by_key(|p| std::cmp::Reverse(model::mtime_nanos(p)));
                        } else {
                            entries.sort();
                        }
                        for f in entries {
                            let sp = f.to_string_lossy().into_owned();
                            let ext = f
                                .extension()
                                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                                .unwrap_or_default();
                            if !model::IMAGE_EXTS.contains(&ext.as_str()) {
                                continue;
                            }
                            if !seen.insert(sp.clone()) {
                                continue;
                            }
                            batch.push(sp);
                            if batch.len() >= LAZY_BATCH_SIZE {
                                let _ = tx.send(Msg::LazyBatch(std::mem::take(&mut batch)));
                            }
                        }
                    } else if model::file_is_image(&rp) {
                        let sp = rp.to_string_lossy().into_owned();
                        if seen.insert(sp.clone()) {
                            batch.push(sp);
                            if batch.len() >= LAZY_BATCH_SIZE {
                                let _ = tx.send(Msg::LazyBatch(std::mem::take(&mut batch)));
                            }
                        }
                    }
                }
                if !batch.is_empty() {
                    let _ = tx.send(Msg::LazyBatch(batch));
                }
                let _ = tx.send(Msg::LazyDone);
            })
            .ok();
    }

    fn commit_lazy(&mut self, paths: Vec<String>, ctx: &egui::Context) {
        if self.quitting {
            return;
        }
        let n_before = self.files.len();
        let first = n_before == 0;
        let new_entries: Vec<FileEntry> = paths.into_iter().map(FileEntry::new).collect();
        self.files.extend(new_entries);
        if self.filter_text.is_empty() {
            self.visible.extend(n_before..self.files.len());
        } else {
            self.rebuild_visible();
        }
        if self.gallery_aspect_opt.is_none() {
            let n_after = self.files.len();
            if n_after <= GALLERY_ASPECT_SAMPLE
                || (n_before < GALLERY_ASPECT_SAMPLE && n_after >= GALLERY_ASPECT_SAMPLE)
            {
                self.gallery_aspect_cache = None;
                self.aspect_gen += 1;
                self.aspect_pending = false;
            }
        }
        if first && !self.files.is_empty() {
            self.fileidx = self.fileidx.min(self.files.len() - 1);
            self.persist_index();
            match self.mode {
                Mode::Image => {
                    self.load_image_async(self.fileidx);
                }
                Mode::Gallery => self.gallery_scroll_to = Some(self.fileidx),
                Mode::List => {}
            }
        }
        ctx.request_repaint();
    }

    // ------------------------------------------------------------ helpers

    fn persist_index(&self) {
        if let Some(p) = &self.opts.idx_write_path {
            let _ = std::fs::write(p, (self.fileidx + 1).to_string());
        }
    }

    pub fn rebuild_visible(&mut self) {
        self.visible = model::filter_visible(&self.files, &self.filter_text);
    }

    fn apply_sort(&mut self, mode: Option<SortMode>, reverse: Option<bool>, refresh: bool) {
        if let Some(m) = mode {
            self.sort_mode = m;
        }
        if let Some(r) = reverse {
            self.sort_reverse = r;
        }
        if self.sort_mode == SortMode::None || self.files.is_empty() {
            return;
        }
        let cur_path = self.files.get(self.fileidx).map(|f| f.path.clone());
        let mut thumbs_by_path: HashMap<String, Vec<TextureHandle>> = HashMap::new();
        for (idx, tex) in std::mem::take(&mut self.thumbs) {
            if let Some(f) = self.files.get(idx) {
                thumbs_by_path.entry(f.path.clone()).or_default().push(tex);
            }
        }
        self.fileidx = model::apply_sort(
            &mut self.files,
            self.sort_mode,
            self.sort_reverse,
            cur_path.as_deref(),
            self.fileidx,
        );
        self.thumbs = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(idx, f)| {
                thumbs_by_path
                    .get_mut(&f.path)
                    .and_then(|v| {
                        if v.is_empty() {
                            None
                        } else {
                            Some(v.remove(0))
                        }
                    })
                    .map(|tex| (idx, tex))
            })
            .collect();
        self.markidx = self.fileidx;
        self.alternate = self.fileidx;
        self.files_gen += 1;
        self.load_token += 1;
        self.prefetch.clear();
        self.prefetch_keys.clear();
        self.in_flight.clear();
        self.pending_load = None;
        self.thumb_in_flight.clear();
        self.mtimes.clear();
        self.gallery_aspect_cache = None;
        self.aspect_gen += 1;
        self.aspect_pending = false;
        self.rebuild_visible();
        self.persist_index();
        if !refresh {
            return;
        }
        match self.mode {
            Mode::Image => {
                let _ = self.load_image_async(self.fileidx);
            }
            _ => {
                self.gallery_scroll_to = Some(self.fileidx);
                if self.mode == Mode::List {
                    self.list_pending = -2;
                }
            }
        }
    }

    // ------------------------------------------------------------ loading

    pub fn load_image_async(&mut self, n: usize) -> bool {
        if n >= self.files.len() {
            return false;
        }
        self.next_frame_at = None;
        self.next_slide_at = None;
        self.load_token += 1;
        let token = self.load_token;
        if n != self.fileidx {
            self.alternate = self.fileidx;
            self.fileidx = n;
        }
        if let Some(fr) = self.prefetch_remove(n) {
            let frames = fr.0;
            let delays = fr.1;
            self.install_image(n, frames, delays);
            self.persist_index();
            self.prefetch_neighbors(n);
            return true;
        }
        let path = self.files[n].path.clone();
        let max_dim = self.cfg_behavior["max_load_dim"] as u32;
        let gen = self.files_gen;
        let tx = self.tx.clone();
        self.pending_load = Some((n, token));
        if !self.in_flight.insert(n) {
            return true;
        }
        self.spawn_limited(move || {
            let res = images::load_frames(&path, Some(max_dim));
            let _ = tx.send(Msg::Loaded {
                token,
                idx: n,
                gen,
                res,
            });
        });
        true
    }

    fn spawn_limited(&self, f: impl FnOnce() + Send + 'static) {
        let job: Job = Box::new(f);
        if let Err(e) = job_queue().send(job) {
            // Cannot happen unless all workers died; keep the UI usable.
            e.0();
        }
    }

    fn prefetch_remove(&mut self, n: usize) -> Option<Frames> {
        if let Some(fr) = self.prefetch.remove(&n) {
            self.prefetch_keys.retain(|&k| k != n);
            Some(fr)
        } else {
            None
        }
    }

    fn install_image(&mut self, n: usize, frames: Vec<image::RgbaImage>, delays: Vec<u32>) {
        self.frames = frames;
        self.orig_frames = self.frames.clone();
        self.delays = delays;
        self.img_sel = 0;
        if let Some(f) = self.frames.first() {
            self.img_w = f.width();
            self.img_h = f.height();
        }
        if let Some(f) = self.files.get_mut(n) {
            f.flags &= !FF_WARN;
        }
        self.pan = Vec2::ZERO;
        self.img_tex = None;
        if let Ok(meta) = std::fs::metadata(&self.files[n].path) {
            if let Ok(m) = meta.modified() {
                if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                    self.mtimes.insert(n, d.as_nanos());
                }
            }
        }
        if self.frames.len() > 1 && self.img_animate {
            self.schedule_animate();
        }
    }

    fn schedule_animate(&mut self) {
        if self.frames.len() > 1 && self.img_animate {
            let d = self
                .delays
                .get(self.img_sel)
                .copied()
                .unwrap_or(model::DEF_ANIM_DELAY_MS);
            self.next_frame_at = Some(Instant::now() + Duration::from_millis(d.max(1) as u64));
        }
    }

    fn prefetch_neighbors(&mut self, n: usize) {
        if self.mode != Mode::Image || self.visible.is_empty() {
            return;
        }
        let Some(pos) = self.visible.iter().position(|&i| i == n) else {
            return;
        };
        for d in [1i64, -1] {
            let mp = pos as i64 + d;
            if mp >= 0 && (mp as usize) < self.visible.len() {
                self.submit_prefetch(self.visible[mp as usize]);
            }
        }
    }

    fn submit_prefetch(&mut self, n: usize) {
        if self.prefetch.contains_key(&n) || n == self.fileidx || self.in_flight.contains(&n) {
            return;
        }
        if n >= self.files.len() {
            return;
        }
        let path = self.files[n].path.clone();
        let max_dim = self.cfg_behavior["max_load_dim"] as u32;
        let gen = self.files_gen;
        let tx = self.tx.clone();
        self.in_flight.insert(n);
        self.spawn_limited(move || {
            let res = images::load_frames(&path, Some(max_dim));
            let _ = tx.send(Msg::Prefetch { idx: n, gen, res });
        });
    }

    // ------------------------------------------------------------ navigation

    fn navigate(&mut self, n: usize) -> bool {
        if n >= self.files.len() {
            return false;
        }
        if n == self.fileidx && self.mode != Mode::Image {
            return false;
        }
        match self.mode {
            Mode::Image => self.load_image_async(n),
            _ => {
                self.alternate = self.fileidx;
                self.fileidx = n;
                self.gallery_scroll_to = Some(n);
                if self.mode == Mode::List {
                    self.list_pending = -2;
                }
                self.persist_index();
                true
            }
        }
    }

    fn nav_visible(&mut self, delta: i64) {
        if self.visible.is_empty() {
            return;
        }
        match self.visible.iter().position(|&i| i == self.fileidx) {
            Some(pos) => {
                let np = pos as i64 + delta;
                if np >= 0 && (np as usize) < self.visible.len() {
                    self.navigate(self.visible[np as usize]);
                }
            }
            None => {
                let pos = if delta >= 0 {
                    0
                } else {
                    self.visible.len() - 1
                };
                self.navigate(self.visible[pos]);
            }
        }
    }

    /// Port of tkiv.py _key_nav: in image mode the arrows pan the image
    /// (act_scroll), in list mode Up/Down move the selection, and in
    /// gallery mode Up/Down move by a full row of columns.
    fn nav_key(&mut self, dir: Dir) {
        match self.mode {
            Mode::Image => match dir {
                Dir::Left => self.pan_step(1.0, 0.0),
                Dir::Right => self.pan_step(-1.0, 0.0),
                Dir::Up => self.pan_step(0.0, 1.0),
                Dir::Down => self.pan_step(0.0, -1.0),
            },
            Mode::List => match dir {
                Dir::Up => self.nav_visible(-1),
                Dir::Down => self.nav_visible(1),
                _ => {}
            },
            Mode::Gallery => {
                let cols = self.gallery_cols.max(1) as i64;
                match dir {
                    Dir::Up => self.nav_visible(-cols),
                    Dir::Down => self.nav_visible(cols),
                    Dir::Left => self.nav_visible(-1),
                    Dir::Right => self.nav_visible(1),
                }
            }
        }
    }

    fn pan_step(&mut self, sx: f32, sy: f32) {
        // pan by 1/5 of window size (screen=false path of act_scroll)
        self.pending_pan = Some((sx, sy));
    }

    fn page_key(&mut self, dir: i64, win_h: f32, row_h: f32) {
        match self.mode {
            Mode::Image => self.nav_image(dir * 10),
            Mode::List => self.nav_visible(dir * 20),
            Mode::Gallery => {
                let rows = ((win_h / row_h).max(1.0)) as i64;
                self.nav_visible(dir * rows * self.gallery_cols.max(1) as i64);
            }
        }
    }

    fn nav_image(&mut self, d: i64) {
        if self.mode == Mode::Image {
            self.nav_visible(d);
        }
    }

    // ------------------------------------------------------------ hold-repeat

    /// Register a fixed navigation key for hold-to-repeat.  Navigation
    /// keys repeat at half the configured `key_repeat_ms` so that holding
    /// Up/Down scrolls quickly (tkiv.py repeats at key_repeat_ms; the
    /// faster default here is a deliberate UX improvement).
    /// `delay` is how long the key must be held before the repeat timer
    /// starts firing (`interval` thereafter), so a single press of an
    /// arrow/Page key advances exactly one step.
    fn hold_nav(&mut self, key: egui::Key, name: String, dir: Dir, interval: Duration, delay: Duration) {
        self.nav_key(dir);
        self.held.insert(
            name,
            HeldKey {
                key,
                what: HeldWhat::Nav(dir),
                interval,
                next_fire: Instant::now() + delay,
            },
        );
    }

    fn hold_page(
        &mut self,
        key: egui::Key,
        name: String,
        dir: i64,
        interval: Duration,
        win: Vec2,
        delay: Duration,
    ) {
        self.page_key(dir, win.y, self.row_h(win));
        self.held.insert(
            name,
            HeldKey {
                key,
                what: HeldWhat::Page(dir),
                interval,
                next_fire: Instant::now() + delay,
            },
        );
    }

    /// Re-fire held keys whose interval elapsed and are still physically
    /// down (keys_down is the safety net for missed release events, the
    /// role tkiv.py's rel_time bookkeeping plays).
    fn fire_held(&mut self, ctx: &egui::Context, win: Vec2) {
        if self.held.is_empty() {
            return;
        }
        let now = Instant::now();
        let keys_down: HashSet<egui::Key> = ctx.input(|i| i.keys_down.clone());
        self.held.retain(|_, h| keys_down.contains(&h.key));
        if self.held.is_empty() {
            return;
        }
        let mut due: Vec<HeldWhat> = Vec::new();
        for h in self.held.values_mut() {
            if now >= h.next_fire {
                due.push(h.what.clone());
                h.next_fire = now + h.interval;
            }
        }
        for what in &due {
            match what {
                HeldWhat::Act(a) => self.run_action(*a, ctx, win),
                HeldWhat::Zoom(d) => self.pending_key_zoom = Some(*d),
                HeldWhat::Nav(d) => self.nav_key(*d),
                HeldWhat::Page(d) => self.page_key(*d, win.y, self.row_h(win)),
            }
        }
        if let Some(next) = self.held.values().map(|h| h.next_fire).min() {
            ctx.request_repaint_after(next.saturating_duration_since(Instant::now()));
        }
    }

    // ------------------------------------------------------------ removal/marks

    fn remove_file(&mut self, n: usize, manual: bool) -> bool {
        if n >= self.files.len() {
            return false;
        }
        if self.files.len() == 1 {
            if !manual && !self.opts.quiet {
                eprintln!("{PROGNAME}: no more files to display, aborting");
            }
            self.quit(if manual { 0 } else { 1 });
        }
        self.files.remove(n);
        if self.fileidx > n || self.fileidx >= self.files.len() {
            self.fileidx = self.fileidx.saturating_sub(1);
        }
        if self.alternate > n || self.alternate >= self.files.len() {
            self.alternate = self.alternate.saturating_sub(1);
        }
        if self.markidx > n || self.markidx >= self.files.len() {
            self.markidx = self.markidx.saturating_sub(1);
        }
        self.files_gen += 1;
        self.prefetch.clear();
        self.prefetch_keys.clear();
        self.in_flight.clear();
        self.pending_load = None;
        self.thumb_in_flight.clear();
        self.thumbs = std::mem::take(&mut self.thumbs)
            .into_iter()
            .filter_map(|(i, t)| {
                if i == n {
                    None
                } else {
                    Some((if i > n { i - 1 } else { i }, t))
                }
            })
            .collect();
        self.rebuild_visible();
        self.persist_index();
        true
    }

    fn mark(&mut self, n: usize, on: bool) -> bool {
        if let Some(f) = self.files.get_mut(n) {
            let old = f.flags & FF_MARK != 0;
            if old != on {
                if on {
                    f.flags |= FF_MARK;
                } else {
                    f.flags &= !FF_MARK;
                }
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------ output

    fn emit_output(&self, use_labels: bool) {
        let mut marked: Vec<&FileEntry> = self.files.iter().filter(|f| f.marked()).collect();
        if marked.is_empty() {
            if let Some(f) = self.files.get(self.fileidx) {
                marked.push(f);
            }
        }
        if marked.is_empty() {
            return;
        }
        use std::io::Write;
        let sep = if self.opts.using_null { "\0" } else { "\n" };
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        for f in marked {
            let s = if use_labels { &f.label } else { &f.path };
            let _ = out.write_all(s.as_bytes());
            let _ = out.write_all(sep.as_bytes());
        }
        let _ = out.flush();
    }

    pub fn quit(&mut self, status: i32) -> ! {
        if self.quitting {
            std::process::exit(status);
        }
        self.quitting = true;
        if status == 0 {
            match self.opts.purpose {
                Purpose::Select => self.emit_output(self.opts.return_label),
                Purpose::View => {
                    if self.opts.stdout {
                        self.emit_output(false);
                    }
                }
            }
        }
        std::process::exit(status);
    }

    // ------------------------------------------------------------ clipboard

    fn copy_image_to_clipboard(&mut self) {
        if self.files.is_empty() || self.fileidx >= self.files.len() {
            return;
        }
        let img = if self.mode == Mode::Image && !self.frames.is_empty() {
            self.frames[self.img_sel].clone()
        } else {
            match images::load_frames(
                &self.files[self.fileidx].path,
                Some(self.cfg_behavior["max_load_dim"] as u32),
            ) {
                Ok((f, _)) if !f.is_empty() => f[0].clone(),
                Ok(_) => return,
                Err(e) => {
                    if !self.opts.quiet {
                        eprintln!("{PROGNAME}: clipboard copy failed: {e}");
                    }
                    return;
                }
            }
        };
        let mut buf = std::io::Cursor::new(Vec::new());
        if image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .is_err()
        {
            return;
        }
        let png = buf.into_inner();
        // Try each helper in turn.  A candidate must be skipped not only when
        // the binary is missing (spawn failure) but also when it starts fine
        // yet fails: wl-copy is often present on X11-only machines (pulled
        // in by wayland-leaning packages) and cannot open a Wayland display
        // there.  tkiv.py falls back on the helper's non-zero exit
        // (check=True / except: continue), but this code returned after any
        // successful spawn, so egiv silently did nothing on such a machine
        // while tkiv still copied.
        let mut spawned_any = false;
        for cmd in [
            vec!["wl-copy", "--type", "image/png"],
            vec!["xclip", "-sel", "clip", "-t", "image/png"],
        ] {
            let mut child = match std::process::Command::new(cmd[0])
                .args(&cmd[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            spawned_any = true;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(&png);
            }
            match child.wait() {
                Ok(st) if st.success() => return,
                // The helper ran but failed (e.g. wl-copy without a Wayland
                // compositor): keep trying the next candidate.
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        if !self.opts.quiet {
            if spawned_any {
                eprintln!(
                    "{PROGNAME}: clipboard copy failed: the clipboard helpers exited with failure"
                );
            } else {
                eprintln!("{PROGNAME}: clipboard copy failed: neither wl-copy nor xclip were found");
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

fn shellexpand(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(h) = std::env::var_os("HOME") {
            return format!("{}/{rest}", h.to_string_lossy());
        }
    }
    p.to_string()
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                collect_recursive(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

// placeholder field storage for pan requests
// (pending_pan consumed during image-mode render)

// ============================================================ actions

const REPEATABLE: [Action; 18] = [
    Action::Next,
    Action::Prev,
    Action::Nav10Forward,
    Action::Nav10Back,
    Action::PanLeft,
    Action::PanDown,
    Action::PanUp,
    Action::PanRight,
    Action::ZoomIn,
    Action::ZoomOut,
    Action::GammaDown,
    Action::GammaUp,
    Action::BrightnessDown,
    Action::BrightnessUp,
    Action::ContrastDown,
    Action::ContrastUp,
    Action::RotateLeft,
    Action::RotateRight,
];

impl App {
    fn run_action(&mut self, a: Action, ctx: &egui::Context, win: Vec2) {
        let r: Result<(), String> = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.dispatch(a, ctx, win);
        }))
        .map_err(|_| "action panicked".to_string());
        if let Err(e) = r {
            if !self.opts.quiet {
                eprintln!("{PROGNAME}: {e}");
            }
        }
    }

    fn dispatch(&mut self, a: Action, ctx: &egui::Context, win: Vec2) {
        match a {
            Action::Quit => self.quit(0),
            Action::ToggleBar => self.show_bar = !self.show_bar,
            Action::Remove => self.act_remove(ctx),
            Action::FitWidth => self.act_fit(ScaleMode::Width),
            Action::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Action::First => {
                if let Some(&i) = self.visible.first() {
                    self.navigate(i);
                }
            }
            Action::Last => {
                if let Some(&i) = self.visible.last() {
                    self.navigate(i);
                }
            }
            Action::CopyToClipboard => self.copy_image_to_clipboard(),
            Action::PanLeft => self.act_scroll(Dir::Left, win),
            Action::PanDown => self.act_scroll(Dir::Down, win),
            Action::PanUp => self.act_scroll(Dir::Up, win),
            Action::PanRight => self.act_scroll(Dir::Right, win),
            Action::ToggleAntialias => self.anti_alias = !self.anti_alias,
            Action::ToggleMark => {
                if !self.files.is_empty() && self.fileidx < self.files.len() {
                    let on = !self.files[self.fileidx].marked();
                    self.mark(self.fileidx, on);
                    self.markidx = self.fileidx;
                }
            }
            Action::Next => self.nav_visible(1),
            Action::Prev => self.nav_visible(-1),
            Action::Reload => self.act_reload(ctx),
            Action::Slideshow => {
                if self.ss_on {
                    self.ss_on = false;
                    self.next_slide_at = None;
                } else {
                    self.ss_on = true;
                    self.schedule_slideshow();
                }
            }
            Action::UnmarkAll => {
                for f in self.files.iter_mut() {
                    f.flags &= !FF_MARK;
                }
            }
            Action::FitDown => self.act_fit(ScaleMode::Down),
            Action::SortCycle => {
                let (m, r) = model::sort_cycle_next(self.sort_mode, self.sort_reverse);
                self.apply_sort(Some(m), Some(r), true);
            }
            Action::SortReverse => {
                if self.sort_mode == SortMode::None {
                    self.apply_sort(Some(SortMode::Name), Some(false), true);
                } else {
                    self.apply_sort(None, Some(!self.sort_reverse), true);
                }
            }
            Action::Center => {
                if self.mode == Mode::Image {
                    self.pan = Vec2::new(
                        (win.x - self.img_w as f32 * self.zoom as f32) / 2.0,
                        (win.y - self.img_h as f32 * self.zoom as f32) / 2.0,
                    );
                }
            }
            Action::Animate => {
                if self.frames.len() > 1 {
                    self.img_animate = !self.img_animate;
                    if self.img_animate {
                        self.next_frame_at = Some(Instant::now());
                    } else {
                        self.next_frame_at = None;
                    }
                }
            }
            Action::Zoom100 => {
                if self.mode == Mode::Gallery {
                    // Reset the gallery thumbnails to their automatic size.
                    if self.gallery_tile_override.take().is_some() {
                        self.thumbs.clear();
                        self.thumb_in_flight.clear();
                    }
                    self.gallery_scroll_to = Some(self.fileidx);
                } else if self.mode == Mode::Image {
                    self.scale_mode = ScaleMode::Zoom;
                    self.zoom_to(1.0, win);
                }
            }
            Action::ZoomIn => self.act_zoom(1, ctx, win),
            Action::ZoomOut => self.act_zoom(-1, ctx, win),
            Action::Nav10Forward => self.nav_image(10),
            Action::Nav10Back => self.nav_image(-10),
            Action::GammaDown => {
                self.gamma = (self.gamma - 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::GammaUp => {
                self.gamma = (self.gamma + 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::ContrastDown => {
                self.contrast = (self.contrast - 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::ContrastUp => {
                self.contrast = (self.contrast + 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::BrightnessDown => {
                self.brightness = (self.brightness - 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::BrightnessUp => {
                self.brightness = (self.brightness + 1).clamp(-model::CC_STEPS, model::CC_STEPS)
            }
            Action::RotateLeft => self.act_rotate(3),
            Action::RotateRight => self.act_rotate(1),
            Action::Rotate180 => self.act_rotate(2),
            Action::FlipH => self.act_flip(true),
            Action::FlipV => self.act_flip(false),
            Action::Fit => self.act_fit(ScaleMode::Fit),
            Action::Fill => self.act_fit(ScaleMode::Fill),
            Action::FitHeight => self.act_fit(ScaleMode::Height),
            Action::ToggleAlpha => self.alpha_layer = !self.alpha_layer,
            Action::ToggleSearchbar => {
                // Show/hide the bar only.  An active search stays applied
                // whether the bar is visible or hidden; only an empty search
                // shows every file.  Esc clears the search while the bar is
                // focused.
                self.no_searchbar = !self.no_searchbar;
                self.rebuild_visible();
            }
            Action::ModeCycleFwd => self.cycle_mode(1),
            Action::ModeCycleRev => self.cycle_mode(-1),
            Action::Return => self.key_return(),
            Action::Escape => {
                if !self.filter_text.is_empty() && !self.no_searchbar {
                    self.filter_text.clear();
                    self.rebuild_visible();
                } else {
                    let s = if self.opts.purpose == Purpose::Select {
                        1
                    } else {
                        0
                    };
                    self.quit(s);
                }
            }
            Action::DeleteKey => self.act_remove(ctx),
        }
        self.img_tex = None;
    }

    fn row_h(&self, win: Vec2) -> f32 {
        let _ = win;
        let (_, th, cap, pad) = self.tile_metrics(egui::Vec2::new(1.0, 1.0), true);
        th + cap + pad
    }

    fn act_fit(&mut self, m: ScaleMode) {
        self.scale_mode = m;
    }

    fn act_reload(&mut self, _ctx: &egui::Context) {
        if self.files.is_empty() || self.fileidx >= self.files.len() {
            return;
        }
        match self.mode {
            Mode::Image => {
                self.prefetch.remove(&self.fileidx);
                self.load_image_async(self.fileidx);
            }
            _ => {
                self.thumbs.remove(&self.fileidx);
            }
        }
    }

    fn act_remove(&mut self, ctx: &egui::Context) {
        if self.files.is_empty() {
            return;
        }
        let mut marked: Vec<usize> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.marked())
            .map(|(i, _)| i)
            .collect();
        if marked.is_empty() {
            marked.push(self.fileidx);
        }
        marked.sort_unstable_by(|a, b| b.cmp(a));
        for i in marked {
            if !self.remove_file(i, true) {
                return;
            }
        }
        if self.fileidx >= self.files.len() {
            self.fileidx = self.files.len().saturating_sub(1);
        }
        match self.mode {
            Mode::Image => {
                self.load_image_async(self.fileidx);
            }
            _ => {
                self.gallery_scroll_to = Some(self.fileidx);
                if self.mode == Mode::List {
                    self.list_pending = -2;
                }
            }
        }
        ctx.request_repaint();
    }

    fn act_scroll(&mut self, d: Dir, win: Vec2) {
        if self.mode != Mode::Image {
            return;
        }
        let x = win.x / 5.0;
        let y = win.y / 5.0;
        let (dx, dy) = match d {
            Dir::Left => (x, 0.0),
            Dir::Right => (-x, 0.0),
            Dir::Up => (0.0, y),
            Dir::Down => (0.0, -y),
        };
        self.pan += Vec2::new(dx, dy);
    }

    fn zoom_to(&mut self, z: f64, win: Vec2) {
        if (model::ZOOM_MIN..=model::ZOOM_MAX).contains(&z) && self.zoom > 0.0 {
            let cx = win.x as f64 / 2.0;
            let cy = win.y as f64 / 2.0;
            self.pan.x = (cx - (cx - self.pan.x as f64) * z / self.zoom) as f32;
            self.pan.y = (cy - (cy - self.pan.y as f64) * z / self.zoom) as f32;
            self.zoom = z;
            self.scale_mode = ScaleMode::Zoom;
        }
    }

    fn act_zoom(&mut self, d: i64, _ctx: &egui::Context, win: Vec2) {
        if self.mode == Mode::Gallery {
            let step = GALLERY_ZOOM_STEP * d as f32;
            let base = self
                .gallery_tile_override
                .unwrap_or_else(|| self.cur_tile_long_side(win));
            let mut ns = base + step;
            ns = ns.clamp(GALLERY_TILE_MIN, GALLERY_TILE_MAX);
            if Some(ns) == self.gallery_tile_override {
                return;
            }
            self.gallery_tile_override = Some(ns);
            // The existing thumbnails are kept on screen (drawn scaled into
            // the new tile size) while sharper ones are re-decoded in the
            // background; clearing them here made the gallery go blank
            // after every Ctrl+plus/minus.
            self.gallery_scroll_to = Some(self.fileidx);
            return;
        }
        if self.mode == Mode::List {
            return;
        }
        if d > 0 {
            for z in model::ZOOM_LEVELS {
                if z > self.zoom + 1e-9 {
                    self.zoom_to(z, win);
                    break;
                }
            }
        } else {
            for z in model::ZOOM_LEVELS.iter().rev() {
                if *z < self.zoom - 1e-9 {
                    self.zoom_to(*z, win);
                    break;
                }
            }
        }
    }

    fn cur_tile_long_side(&self, win: Vec2) -> f32 {
        let (tw, th, _, _) = self.tile_metrics(win, true);
        tw.max(th)
    }

    fn act_rotate(&mut self, quarter_turns: u32) {
        if self.mode != Mode::Image || self.frames.is_empty() {
            return;
        }
        for f in self.frames.iter_mut() {
            *f = match quarter_turns % 4 {
                1 => image::imageops::rotate90(f),
                2 => image::imageops::rotate180(f),
                3 => image::imageops::rotate270(f),
                _ => continue,
            };
        }
        if let Some(f) = self.frames.first() {
            self.img_w = f.width();
            self.img_h = f.height();
        }
    }

    fn act_flip(&mut self, horizontal: bool) {
        if self.mode != Mode::Image || self.frames.is_empty() {
            return;
        }
        for f in self.frames.iter_mut() {
            *f = if horizontal {
                image::imageops::flip_horizontal(f)
            } else {
                image::imageops::flip_vertical(f)
            };
        }
    }

    fn schedule_slideshow(&mut self) {
        if self.ss_on {
            let mut ms = self.ss_delay_ds.max(0) as u64 * 100;
            if self.frames.len() > 1 && self.img_animate {
                let total: u64 = self.delays.iter().map(|d| *d as u64).sum();
                ms = ms.max(total);
            }
            self.next_slide_at = Some(Instant::now() + Duration::from_millis(ms));
        }
    }

    fn key_return(&mut self) {
        if self.opts.purpose == Purpose::Select {
            self.quit(0);
        }
        match self.mode {
            Mode::List | Mode::Gallery => self.enter_mode(Mode::Image),
            Mode::Image => self.enter_mode(Mode::Gallery),
        }
    }

    fn cycle_mode(&mut self, dir: i64) {
        let order = [Mode::Image, Mode::Gallery, Mode::List];
        let idx = order.iter().position(|m| *m == self.mode).unwrap_or(0) as i64;
        let target = order[((idx + dir).rem_euclid(3)) as usize];
        self.enter_mode(target);
    }

    fn enter_mode(&mut self, m: Mode) {
        if m == self.mode {
            return;
        }
        self.mode = m;
        self.next_frame_at = None;
        if m != Mode::Image {
            self.ss_on = false;
            self.next_slide_at = None;
        }
        match m {
            Mode::Image => {
                if !self.files.is_empty() && self.fileidx < self.files.len() {
                    self.load_image_async(self.fileidx);
                }
            }
            Mode::Gallery => self.gallery_scroll_to = Some(self.fileidx),
            Mode::List => self.list_pending = -2,
        }
    }

    // ------------------------------------------------------------ gallery metrics

    fn gallery_aspect(&mut self, win_aspect_default: f32) -> f32 {
        if let Some(a) = self.gallery_aspect_cache {
            return a;
        }
        if let Some(a) = self.gallery_aspect_opt {
            let a = a.clamp(GALLERY_ASPECT_MIN, GALLERY_ASPECT_MAX);
            self.gallery_aspect_cache = Some(a);
            return a;
        }
        if !self.aspect_pending {
            self.aspect_pending = true;
            self.aspect_gen += 1;
            let gen = self.aspect_gen;
            let sample: Vec<String> = self
                .files
                .iter()
                .take(GALLERY_ASPECT_SAMPLE)
                .map(|f| f.path.clone())
                .collect();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let mut ratios = vec![];
                for p in sample {
                    let (w, h) = images::get_orig_size(&p);
                    if w > 0 && h > 0 {
                        ratios.push(w as f64 / h as f64);
                    }
                }
                let _ = tx.send(Msg::Aspect { gen, ratios });
            });
        }
        let _ = win_aspect_default;
        self.gallery_aspect_cache = Some(GALLERY_ASPECT_DEFAULT);
        GALLERY_ASPECT_DEFAULT
    }

    /// Returns (tile_w, tile_h, caption_h, pad). `probe` avoids borrowing
    /// issues by optionally skipping the aspect worker start.
    fn tile_metrics(&self, win: Vec2, probe: bool) -> (f32, f32, f32, f32) {
        let aspect = if probe {
            GALLERY_ASPECT_DEFAULT
        } else {
            self.gallery_aspect_cache
                .unwrap_or(self.gallery_aspect_opt.unwrap_or(GALLERY_ASPECT_DEFAULT))
        };
        let aspect = if let Some(a) = self.gallery_aspect_cache {
            a
        } else {
            aspect
        };
        let (base_w, base_h);
        if let Some(ov) = self.gallery_tile_override {
            if aspect >= 1.0 {
                base_w = ov;
                base_h = ov / aspect;
            } else {
                base_h = ov;
                base_w = ov * aspect;
            }
        } else {
            let rows = self
                .opts
                .gallery_rows
                .filter(|r| *r > 0.0)
                .unwrap_or(GALLERY_TARGET_ROWS as f64) as f32;
            let denom_v = 1.0 + GALLERY_CAPTION_RATIO + GALLERY_PAD_RATIO;
            let th_from_rows = (win.y / rows) / denom_v;
            let mut tw_from_rows = th_from_rows * aspect;
            if let Some(cols) = self.opts.gallery_cols.filter(|c| *c > 0) {
                let denom_h = 1.0 + GALLERY_PAD_RATIO;
                let tw_from_cols = (win.x / cols as f32) / denom_h;
                tw_from_rows = tw_from_rows.min(tw_from_cols);
            }
            base_w = tw_from_rows;
            base_h = base_w / aspect;
        }
        let q = GALLERY_SIZE_QUANTUM;
        let mut tile_w = ((base_w / q).round() * q).max(q);
        let mut tile_h = ((base_h / q).round() * q).max(q);
        let long = tile_w.max(tile_h);
        if long < GALLERY_TILE_MIN {
            let s = GALLERY_TILE_MIN / long;
            tile_w = ((tile_w * s / q).round() * q).max(q);
            tile_h = ((tile_h * s / q).round() * q).max(q);
        } else if long > GALLERY_TILE_MAX {
            let s = GALLERY_TILE_MAX / long;
            tile_w = ((tile_w * s / q).round() * q).max(q);
            tile_h = ((tile_h * s / q).round() * q).max(q);
        }
        let pad = (tile_h * GALLERY_PAD_RATIO).round().max(GALLERY_PAD_MIN);
        let cap = (tile_h * GALLERY_CAPTION_RATIO)
            .round()
            .max(GALLERY_CAPTION_MIN);
        (tile_w, tile_h, cap, pad)
    }

    fn submit_gallery_tile(&mut self, i: usize, max_w: u32, max_h: u32) {
        if i >= self.files.len()
            || self.thumb_in_flight.contains(&i)
            || self.thumb_in_flight.len() >= THUMB_MAX_IN_FLIGHT
        {
            return;
        }
        // A thumbnail is skipped only when one at a sufficient resolution
        // already exists; an older, smaller one stays on screen (scaled)
        // while a sharper decode for the new tile size runs in the
        // background and replaces it.
        if let Some(tex) = self.thumbs.get(&i) {
            if tex.size()[0] >= max_w as usize {
                return;
            }
        }
        self.thumb_in_flight.insert(i);
        let path = self.files[i].path.clone();
        let gen = self.files_gen;
        let tx = self.tx.clone();
        let selector = self.opts.purpose == Purpose::Select;
        std::thread::spawn(move || {
            let res = if selector {
                images::decode_thumb(&path, max_w, max_h)
            } else {
                images::load_gallery_thumb(&path, max_w, max_h)
                    .ok()
                    .map(|im| (im, images::get_orig_size(&path)))
            };
            let _ = tx.send(Msg::Thumb { idx: i, gen, res });
        });
    }
}
