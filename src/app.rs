//! The egui application: image / gallery / list modes, filtering,
//! marking, sorting, transforms, slideshow, animation, async decode,
//! prefetching, lazy scan, and selector dmenu output semantics.

use std::time::Duration;

use egui::{Color32, Key};

use crate::cli::{SelectorOpts, SharedOpts, ViewerOpts};
use crate::config::{default_theme, Config};
use crate::images::Frames;
use crate::model::SortMode;

pub const QUEUE_POLL: Duration = Duration::from_millis(20);
pub const FILTER_DEBOUNCE: Duration = Duration::from_millis(60);
pub const PREFETCH_MAX: usize = 20;
pub const THUMB_MAX_IN_FLIGHT: usize = 20;
pub const LAZY_BATCH_SIZE: usize = 128;
pub const GALLERY_TILE_MIN: f32 = 96.0;
pub const GALLERY_TILE_MAX: f32 = 512.0;
pub const GALLERY_TARGET_ROWS: f32 = 3.5;
pub const GALLERY_CAPTION_RATIO: f32 = 0.16;
pub const GALLERY_CAPTION_MIN: f32 = 22.0;
pub const GALLERY_PAD_RATIO: f32 = 0.045;
pub const GALLERY_PAD_MIN: f32 = 6.0;
pub const GALLERY_SIZE_QUANTUM: f32 = 8.0;
pub const GALLERY_ZOOM_STEP: f32 = 32.0;
pub const GALLERY_ASPECT_DEFAULT: f32 = 16.0 / 9.0;
pub const GALLERY_ASPECT_MIN: f32 = 0.4;
pub const GALLERY_ASPECT_MAX: f32 = 3.0;
pub const GALLERY_ASPECT_SAMPLE: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Image,
    Gallery,
    List,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleMode {
    Down,
    Fit,
    Fill,
    Width,
    Height,
    Zoom,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    View,
    Select,
}

// ---------------------------------------------------------------- messages

pub(crate) enum Msg {
    Loaded {
        token: u64,
        idx: usize,
        gen: u64,
        res: Result<Frames, String>,
    },
    Prefetch {
        idx: usize,
        gen: u64,
        res: Result<Frames, String>,
    },
    Thumb {
        idx: usize,
        gen: u64,
        res: Option<(image::RgbaImage, (u32, u32))>,
    },
    LazyBatch(Vec<String>),
    LazyDone,
    Aspect {
        gen: u64,
        ratios: Vec<f64>,
    },
    ListPreview {
        idx: usize,
        res: Result<Frames, String>,
    },
}

// ---------------------------------------------------------------- actions

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    Quit,
    ToggleBar,
    Remove,
    FitWidth,
    Fullscreen,
    First,
    CopyToClipboard,
    PanLeft,
    ToggleAntialias,
    PanDown,
    PanUp,
    PanRight,
    ToggleMark,
    Next,
    Prev,
    Reload,
    Slideshow,
    UnmarkAll,
    FitDown,
    SortCycle,
    Center,
    Animate,
    Zoom100,
    ZoomIn,
    ZoomOut,
    Nav10Forward,
    Nav10Back,
    GammaDown,
    GammaUp,
    BrightnessDown,
    BrightnessUp,
    ContrastDown,
    ContrastUp,
    RotateLeft,
    RotateRight,
    Rotate180,
    FlipH,
    FlipV,
    Fit,
    Fill,
    FitHeight,
    ToggleAlpha,
    SortReverse,
    ToggleSearchbar,
    Last,
    ModeCycleFwd,
    ModeCycleRev,
    Return,
    Escape,
    DeleteKey,
}

pub(crate) const ACTION_NAMES: [(&str, Action); 44] = [
    ("quit", Action::Quit),
    ("toggle_bar", Action::ToggleBar),
    ("remove", Action::Remove),
    ("fit_width", Action::FitWidth),
    ("fullscreen", Action::Fullscreen),
    ("first", Action::First),
    ("copy_to_clipboard", Action::CopyToClipboard),
    ("pan_left", Action::PanLeft),
    ("toggle_antialias", Action::ToggleAntialias),
    ("pan_down", Action::PanDown),
    ("pan_up", Action::PanUp),
    ("pan_right", Action::PanRight),
    ("toggle_mark", Action::ToggleMark),
    ("next", Action::Next),
    ("prev", Action::Prev),
    ("reload", Action::Reload),
    ("slideshow", Action::Slideshow),
    ("unmark_all", Action::UnmarkAll),
    ("fit_down", Action::FitDown),
    ("sort_cycle", Action::SortCycle),
    ("center", Action::Center),
    ("animate", Action::Animate),
    ("zoom_100", Action::Zoom100),
    ("zoom_in", Action::ZoomIn),
    ("zoom_out", Action::ZoomOut),
    ("nav_10_forward", Action::Nav10Forward),
    ("nav_10_back", Action::Nav10Back),
    ("gamma_down", Action::GammaDown),
    ("gamma_up", Action::GammaUp),
    ("brightness_down", Action::BrightnessDown),
    ("brightness_up", Action::BrightnessUp),
    ("contrast_down", Action::ContrastDown),
    ("contrast_up", Action::ContrastUp),
    ("rotate_left", Action::RotateLeft),
    ("rotate_right", Action::RotateRight),
    ("rotate_180", Action::Rotate180),
    ("flip_h", Action::FlipH),
    ("flip_v", Action::FlipV),
    ("fit", Action::Fit),
    ("fill", Action::Fill),
    ("fit_height", Action::FitHeight),
    ("toggle_alpha", Action::ToggleAlpha),
    ("sort_reverse", Action::SortReverse),
    ("toggle_searchbar", Action::ToggleSearchbar),
];

// ---------------------------------------------------------------- key binding

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Binding {
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) meta: bool,
    /// Lowercased Tk keysym, e.g. "q", "plus", "Return".  Letters already
    /// carry case per _parse_key_spec semantics (Shift+letter → uppercase).
    pub(crate) key: String,
}

impl Binding {
    pub(crate) fn from_tk_seq(seq: &str) -> Option<Binding> {
        if !(seq.starts_with('<') && seq.ends_with('>')) {
            return None;
        }
        let inner = &seq[1..seq.len() - 1];
        if inner.is_empty() {
            return None;
        }
        let mut b = Binding {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key: String::new(),
        };
        for part in inner.split('-') {
            match part {
                "Control" => b.ctrl = true,
                "Shift" => b.shift = true,
                "Alt" => b.alt = true,
                "Meta" | "Cmd" => b.meta = true,
                other => b.key = other.to_string(),
            }
        }
        if b.key.is_empty() {
            None
        } else {
            Some(b)
        }
    }
}

pub(crate) fn key_from_event_text(k: &str) -> Option<Key> {
    Some(match k {
        "space" => Key::Space,
        "Return" => Key::Enter,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        "BackSpace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "Prior" => Key::PageUp,
        "Next" => Key::PageDown,
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "Insert" => Key::Insert,
        "plus" | "equal" => Key::Plus,
        "minus" | "underscore" => Key::Minus,
        "bracketleft" => Key::OpenBracket,
        "bracketright" => Key::CloseBracket,
        // egui delivers shifted punctuation through the LOGICAL key path
        // (winit keysym -> egui::Key::from_name), so '{' arrives as
        // Key::OpenCurlyBracket, '|' as Key::Pipe and '?' as
        // Key::Questionmark — NOT as the underlying physical key.
        // Mapping these correctly is what makes Ctrl+{ / Ctrl+} (gamma),
        // Ctrl+| (flip) and Ctrl+? (rotate 180) work at all.
        "braceleft" => Key::OpenCurlyBracket,
        "braceright" => Key::CloseCurlyBracket,
        "parenleft" => Key::Num9,
        "parenright" => Key::Num0,
        "less" => Key::Comma,
        "greater" => Key::Period,
        "question" => Key::Questionmark,
        "bar" => Key::Pipe,
        "KP_Enter" => Key::Enter,
        "KP_Add" => Key::Plus,
        "KP_Subtract" => Key::Minus,
        s if s.len() == 1 && s.chars().next().unwrap().is_ascii_digit() => match s {
            "0" => Key::Num0,
            "1" => Key::Num1,
            "2" => Key::Num2,
            "3" => Key::Num3,
            "4" => Key::Num4,
            "5" => Key::Num5,
            "6" => Key::Num6,
            "7" => Key::Num7,
            "8" => Key::Num8,
            _ => Key::Num9,
        },
        s if s.len() == 1 && s.is_ascii() && s.chars().next().unwrap().is_alphabetic() => {
            let c = s.chars().next().unwrap().to_ascii_uppercase();
            match c {
                'A' => Key::A,
                'B' => Key::B,
                'C' => Key::C,
                'D' => Key::D,
                'E' => Key::E,
                'F' => Key::F,
                'G' => Key::G,
                'H' => Key::H,
                'I' => Key::I,
                'J' => Key::J,
                'K' => Key::K,
                'L' => Key::L,
                'M' => Key::M,
                'N' => Key::N,
                'O' => Key::O,
                'P' => Key::P,
                'Q' => Key::Q,
                'R' => Key::R,
                'S' => Key::S,
                'T' => Key::T,
                'U' => Key::U,
                'V' => Key::V,
                'W' => Key::W,
                'X' => Key::X,
                'Y' => Key::Y,
                _ => Key::Z,
            }
        }
        s if s.starts_with('F') && s[1..].parse::<u32>().is_ok() => {
            let n: usize = s[1..].parse().ok()?;
            const FKEYS: [Key; 12] = [
                Key::F1,
                Key::F2,
                Key::F3,
                Key::F4,
                Key::F5,
                Key::F6,
                Key::F7,
                Key::F8,
                Key::F9,
                Key::F10,
                Key::F11,
                Key::F12,
            ];
            if (1..=12).contains(&n) {
                FKEYS[n - 1]
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------- theme

pub struct Theme {
    pub bg_primary: Color32,
    pub bg_secondary: Color32,
    pub bg_input: Color32,
    pub fg_text: Color32,
    pub fg_bright: Color32,
    pub accent: Color32,
    pub selected_bg: Color32,
    pub selected_fg: Color32,
    pub hover_border: Color32,
}

fn parse_hex(s: &str) -> Option<Color32> {
    let r = u8::from_str_radix(s.get(1..3)?, 16).ok()?;
    let g = u8::from_str_radix(s.get(3..5)?, 16).ok()?;
    let b = u8::from_str_radix(s.get(5..7)?, 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

impl Theme {
    pub fn from_config(cfg: &Config) -> Theme {
        let t = cfg.merged_theme();
        let get = |k: &str| {
            t.get(k)
                .and_then(|s| parse_hex(s))
                .unwrap_or_else(|| parse_hex(default_theme()[k]).unwrap())
        };
        Theme {
            bg_primary: get("bg_primary"),
            bg_secondary: get("bg_secondary"),
            bg_input: get("bg_input"),
            fg_text: get("fg_text"),
            fg_bright: get("fg_bright"),
            accent: get("accent"),
            selected_bg: get("selected_bg"),
            selected_fg: get("selected_fg"),
            hover_border: get("hover_border"),
        }
    }
}

// ---------------------------------------------------------------- options bundle

/// Normalised runtime options shared by both entry points.
pub struct Opts {
    pub purpose: Purpose,
    pub thumb_mode: bool,
    pub list_mode: bool,
    pub thumb_size: Option<u32>,
    pub gallery_rows: Option<f64>,
    pub gallery_cols: Option<u32>,
    pub gallery_aspect: Option<f64>,
    pub lazy: bool,
    pub recursive: bool,
    pub hidden: bool,
    pub time: bool,
    pub sort: SortMode,
    pub sort_reverse: bool,
    pub start_at: i64,
    pub name: Option<String>,
    pub idx_write_path: Option<String>,
    pub no_searchbar: bool,
    pub quiet: bool,
    // viewer
    pub animate: bool,
    pub assume_files: bool,
    pub show_bar: bool,
    pub floating_window: bool,
    pub fullscreen: bool,
    pub gamma: i32,
    pub geometry: Option<String>,
    pub ss_delay_decisec: i64, // slideshow delay in deciseconds
    pub ss_on: bool,
    pub scale_mode: ScaleMode,
    pub zoom_start: f64,
    pub using_null: bool,
    pub anti_alias: bool,
    pub alpha_layer: bool,
    pub stdout: bool,
    pub return_label: bool,
}

pub fn opts_from_viewer(o: &ViewerOpts) -> Opts {
    let name = o
        .shared
        .name
        .clone()
        .or_else(|| o.legacy_name.clone())
        .or_else(|| o.class_.clone());
    let (scale_mode, zoom_start) = if o.zoom_100 {
        (ScaleMode::Zoom, 1.0)
    } else if o.zoom > 0 {
        (ScaleMode::Zoom, o.zoom as f64 / 100.0)
    } else {
        (
            match o.scale_mode {
                'd' => ScaleMode::Down,
                'f' => ScaleMode::Fit,
                'F' => ScaleMode::Fill,
                'w' => ScaleMode::Width,
                'h' => ScaleMode::Height,
                'z' => ScaleMode::Zoom,
                _ => ScaleMode::Down,
            },
            1.0,
        )
    };
    Opts {
        purpose: Purpose::View,
        thumb_mode: o.shared.thumb_mode,
        list_mode: o.shared.list_mode,
        thumb_size: o.shared.thumb_size,
        gallery_rows: o.shared.gallery_rows,
        gallery_cols: o.shared.gallery_cols,
        gallery_aspect: o.shared.gallery_aspect,
        lazy: o.shared.lazy,
        recursive: o.shared.recursive,
        hidden: o.shared.hidden,
        time: o.shared.time,
        sort: SortMode::parse(&o.shared.sort),
        sort_reverse: o.shared.sort_reverse,
        start_at: o.shared.start_at,
        name,
        idx_write_path: o.shared.idx_write_path.clone(),
        no_searchbar: o.shared.no_searchbar,
        quiet: o.shared.quiet,
        animate: o.animate,
        assume_files: o.assume_files,
        show_bar: !o.no_bar || o.bar,
        floating_window: o.floating_window,
        fullscreen: o.fullscreen,
        gamma: o.gamma,
        geometry: o.geometry.clone(),
        ss_delay_decisec: 0, // filled with behavior later
        ss_on: o.ss_delay > 0.0,
        scale_mode,
        zoom_start,
        using_null: o.using_null,
        anti_alias: o.anti_alias != "no",
        alpha_layer: o.alpha_layer.as_deref() == Some("yes"),
        stdout: o.stdout,
        return_label: false,
    }
}

pub fn opts_from_selector(o: &SelectorOpts) -> Opts {
    Opts {
        purpose: Purpose::Select,
        thumb_mode: o.shared.thumb_mode,
        list_mode: o.shared.list_mode,
        thumb_size: o.shared.thumb_size,
        gallery_rows: o.shared.gallery_rows,
        gallery_cols: o.shared.gallery_cols,
        gallery_aspect: o.shared.gallery_aspect,
        lazy: o.shared.lazy,
        recursive: o.shared.recursive,
        hidden: o.shared.hidden,
        time: o.shared.time,
        sort: SortMode::parse(&o.shared.sort),
        sort_reverse: o.shared.sort_reverse,
        start_at: o.shared.start_at,
        name: o.shared.name.clone(),
        idx_write_path: o.shared.idx_write_path.clone(),
        no_searchbar: o.shared.no_searchbar,
        quiet: o.shared.quiet,
        animate: false,
        assume_files: o.dmenu_mode, // tkiv.py sets assume_files in dmenu mode
        show_bar: true,
        floating_window: false,
        fullscreen: false,
        gamma: 0,
        geometry: None,
        ss_delay_decisec: 0,
        ss_on: false,
        scale_mode: ScaleMode::Down,
        zoom_start: 1.0,
        using_null: o.using_null,
        anti_alias: true,
        alpha_layer: false,
        stdout: false,
        return_label: o.return_label,
    }
}

pub fn sh(o: &SharedOpts) -> &SharedOpts {
    o
}
