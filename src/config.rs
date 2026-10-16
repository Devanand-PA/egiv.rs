//! INI-like config file parsing with tkiv.py's exact all-or-nothing
//! semantics: ANY syntax/validation error discards the whole config.

use std::collections::HashMap;
use std::path::PathBuf;

pub const PROGNAME: &str = "egiv";
pub const VERSION: &str = "0.4.1";

pub const SLIDESHOW_DELAY: i64 = 5; // seconds
pub const MAX_LOAD_DIM: i64 = 4096;
pub const KEY_REPEAT_MS: i64 = 30;
/// A key must be held this long before its repeat timer starts firing, so
/// a single press advances exactly one step.
pub const KEY_HOLD_DELAY_MS: i64 = 300;

/// Theme colours (Nord-ish), same defaults as tkiv.py.
pub fn default_theme() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("bg_primary", "#2e3440"),
        ("bg_secondary", "#3b4252"),
        ("bg_input", "#4c566a"),
        ("fg_text", "#d8dee9"),
        ("fg_bright", "#eceff4"),
        ("accent", "#88c0d0"),
        ("accent_fg", "#2e3440"),
        ("selected_bg", "#5e81ac"),
        ("selected_fg", "#eceff4"),
        ("hover_border", "#4c566a"),
    ])
}

/// Default action -> bindings mapping (order preserved).
pub fn default_keys() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("quit", vec!["Ctrl+Q"]),
        ("toggle_bar", vec!["Ctrl+B"]),
        ("remove", vec!["Ctrl+D"]),
        ("fit_width", vec!["Ctrl+E"]),
        ("fullscreen", vec!["F11"]),
        ("first", vec!["Ctrl+G"]),
        ("copy_to_clipboard", vec!["Ctrl+C"]),
        ("pan_left", vec!["Ctrl+H"]),
        ("toggle_antialias", vec!["Ctrl+I"]),
        ("pan_down", vec!["Ctrl+J"]),
        ("pan_up", vec!["Ctrl+K"]),
        ("pan_right", vec!["Ctrl+L"]),
        ("toggle_mark", vec!["Ctrl+M"]),
        ("next", vec!["Ctrl+N"]),
        ("prev", vec!["Ctrl+P"]),
        ("reload", vec!["Ctrl+R"]),
        ("slideshow", vec!["Ctrl+S"]),
        ("unmark_all", vec!["Ctrl+U"]),
        ("fit_down", vec!["Ctrl+W"]),
        ("sort_cycle", vec!["Ctrl+Y"]),
        ("center", vec!["Ctrl+Z"]),
        ("animate", vec!["Ctrl+Space"]),
        ("zoom_100", vec!["Ctrl+0"]),
        ("zoom_in", vec!["Ctrl+Plus"]),
        ("zoom_out", vec!["Ctrl+Minus"]),
        ("nav_10_forward", vec!["Ctrl+BracketRight"]),
        ("nav_10_back", vec!["Ctrl+BracketLeft"]),
        ("gamma_down", vec!["Ctrl+BraceLeft"]),
        ("gamma_up", vec!["Ctrl+BraceRight"]),
        ("contrast_down", vec!["Ctrl+ParenLeft"]),
        ("contrast_up", vec!["Ctrl+ParenRight"]),
        ("brightness_down", vec!["Ctrl+1"]),
        ("brightness_up", vec!["Ctrl+2"]),
        ("rotate_left", vec!["Ctrl+Less"]),
        ("rotate_right", vec!["Ctrl+Greater"]),
        ("rotate_180", vec!["Ctrl+Question"]),
        ("flip_h", vec!["Ctrl+Bar"]),
        ("flip_v", vec!["Ctrl+Underscore"]),
        ("fit", vec!["Ctrl+Shift+W"]),
        ("fill", vec!["Ctrl+Shift+F"]),
        ("fit_height", vec!["Ctrl+Shift+E"]),
        ("toggle_alpha", vec!["Ctrl+Shift+I"]),
        ("sort_reverse", vec!["Ctrl+Shift+Y"]),
        ("toggle_searchbar", vec!["Ctrl+F"]),
    ]
}

pub fn default_behavior() -> HashMap<&'static str, i64> {
    HashMap::from([
        ("slideshow_delay", SLIDESHOW_DELAY),
        ("max_load_dim", MAX_LOAD_DIM),
        ("key_repeat_ms", KEY_REPEAT_MS),
        ("key_hold_delay_ms", KEY_HOLD_DELAY_MS),
    ])
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// action name -> list of binding specs (config overrides defaults)
    pub keys: HashMap<String, Vec<String>>,
    pub theme: HashMap<String, String>,
    pub behavior: HashMap<String, i64>,
}

pub fn validate_color(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

impl Config {
    /// Merge over defaults, exactly like tkiv.py does.
    pub fn merged_keys(&self) -> Vec<(String, Vec<String>)> {
        default_keys()
            .into_iter()
            .map(|(a, v)| {
                (
                    a.to_string(),
                    self.keys
                        .get(a)
                        .cloned()
                        .unwrap_or_else(|| v.iter().map(|s| s.to_string()).collect()),
                )
            })
            .collect()
    }
    pub fn merged_behavior(&self) -> HashMap<String, i64> {
        let mut b: HashMap<String, i64> = default_behavior()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        for (k, v) in &self.behavior {
            b.insert(k.clone(), *v);
        }
        b
    }
    pub fn merged_theme(&self) -> HashMap<String, String> {
        let mut t: HashMap<String, String> = default_theme()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (k, v) in &self.theme {
            t.insert(k.clone(), v.clone());
        }
        t
    }
}

/// Parse config text.  Err on any problem (caller discards everything).
pub fn parse_config_text(text: &str) -> Result<Config, String> {
    let known_actions: Vec<&str> = default_keys().into_iter().map(|(a, _)| a).collect();
    let theme_keys: Vec<&str> = default_theme().keys().copied().collect();
    let behavior_keys: Vec<&str> = default_behavior().keys().copied().collect();

    let mut cfg = Config::default();
    let mut section: Option<String> = None;
    for (lineno, raw) in text.lines().enumerate() {
        let lineno = lineno + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_lowercase();
            if !["keys", "theme", "behavior"].contains(&name.as_str()) {
                return Err(format!("line {lineno}: unknown section [{name}]"));
            }
            section = Some(name);
            continue;
        }
        let sec = match &section {
            None => return Err(format!("line {lineno}: key outside of any section")),
            Some(s) => s.clone(),
        };
        let Some(eq) = line.find('=') else {
            return Err(format!("line {lineno}: expected 'key = value'"));
        };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        if key.is_empty() {
            return Err(format!("line {lineno}: empty key"));
        }
        match sec.as_str() {
            "keys" => {
                if !known_actions.contains(&key) {
                    return Err(format!("line {lineno}: unknown action '{key}'"));
                }
                if crate::keys::parse_key_spec(val).is_none() {
                    return Err(format!("line {lineno}: invalid key sequence '{val}'"));
                }
                cfg.keys
                    .entry(key.to_string())
                    .or_default()
                    .push(val.to_string());
            }
            "theme" => {
                if !theme_keys.contains(&key) {
                    return Err(format!("line {lineno}: unknown theme key '{key}'"));
                }
                if !validate_color(val) {
                    return Err(format!("line {lineno}: invalid color '{val}'"));
                }
                cfg.theme.insert(key.to_string(), val.to_string());
            }
            _ => {
                // behavior
                if !behavior_keys.contains(&key) {
                    return Err(format!("line {lineno}: unknown behavior '{key}'"));
                }
                match key {
                    "slideshow_delay" | "max_load_dim" | "key_repeat_ms" => {
                        let iv: i64 = val
                            .parse()
                            .map_err(|_| format!("line {lineno}: {key} must be an integer"))?;
                        if iv <= 0 {
                            return Err(format!("line {lineno}: {key} must be positive"));
                        }
                        cfg.behavior.insert(key.to_string(), iv);
                    }
                    _ => return Err(format!("line {lineno}: unhandled behavior '{key}'")),
                }
            }
        }
    }
    Ok(cfg)
}

/// ~/.config/egiv/config  (or $XDG_CONFIG_HOME/egiv/config)
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("~"));
            home.join(".config")
        });
    base.join("egiv").join("config")
}

/// Config location from the tkiv.py days, still read when the new one is
/// absent so existing setups keep their bindings and theme.
fn legacy_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let p = base.join("tkiv.py").join("config");
    p.exists().then_some(p)
}

/// Load config once.  On any error the whole config is discarded.
pub fn load_config() -> Config {
    let mut path = config_path();
    if !path.exists() {
        // Fall back to the tkiv.py-era location so existing setups keep
        // their bindings and theme after the rename.
        if let Some(legacy) = legacy_config_path() {
            path = legacy;
        }
    }
    if !path.exists() {
        return Config::default();
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{PROGNAME}: cannot read config {}: {e}; using defaults",
                path.display()
            );
            return Config::default();
        }
    };
    match parse_config_text(&text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{PROGNAME}: config error: {e}; using defaults");
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let text = r#"
# comment
[keys]
next = Ctrl+N
next = Space
quit = Ctrl+W

[theme]
accent = #aabbcc

[behavior]
slideshow_delay = 3
key_repeat_ms = 45
"#;
        let cfg = parse_config_text(text).unwrap();
        assert_eq!(
            cfg.keys["next"],
            vec!["Ctrl+N".to_string(), "Space".to_string()]
        );
        assert_eq!(cfg.keys["quit"], vec!["Ctrl+W".to_string()]);
        assert_eq!(cfg.theme["accent"], "#aabbcc");
        assert_eq!(cfg.behavior["slideshow_delay"], 3);
        assert_eq!(cfg.behavior["key_repeat_ms"], 45);
    }

    #[test]
    fn any_error_rejects_everything() {
        // unknown section
        assert!(parse_config_text("[bogus]\nx = y").is_err());
        // key outside section
        assert!(parse_config_text("next = Ctrl+N").is_err());
        // missing '='
        assert!(parse_config_text("[keys]\nnext").is_err());
        // empty key
        assert!(parse_config_text("[keys]\n= Ctrl+N").is_err());
        // unknown action
        assert!(parse_config_text("[keys]\nnope = Ctrl+N").is_err());
        // invalid key spec
        assert!(parse_config_text("[keys]\nnext = Ctrl+").is_err());
        // unknown theme key
        assert!(parse_config_text("[theme]\nnope = #aabbcc").is_err());
        // invalid color
        assert!(parse_config_text("[theme]\naccent = aabbcc").is_err());
        assert!(parse_config_text("[theme]\naccent = #abc").is_err());
        // unknown behavior
        assert!(parse_config_text("[behavior]\nnope = 1").is_err());
        // non-int behavior
        assert!(parse_config_text("[behavior]\nslideshow_delay = x").is_err());
        // non-positive behavior
        assert!(parse_config_text("[behavior]\nslideshow_delay = 0").is_err());
        assert!(parse_config_text("[behavior]\nmax_load_dim = -5").is_err());
    }

    #[test]
    fn section_names_case_insensitive() {
        let cfg = parse_config_text("[THEME]\naccent = #AABBCC").unwrap();
        assert_eq!(cfg.theme["accent"], "#AABBCC");
    }

    #[test]
    fn merged_respects_overrides() {
        let cfg =
            parse_config_text("[keys]\nnext = Ctrl+J\n[behavior]\nslideshow_delay=9").unwrap();
        let keys = cfg.merged_keys();
        let next = keys.iter().find(|(a, _)| a == "next").unwrap();
        assert_eq!(next.1, vec!["Ctrl+J".to_string()]);
        let prev = keys.iter().find(|(a, _)| a == "prev").unwrap();
        assert_eq!(prev.1, vec!["Ctrl+P".to_string()]);
        let b = cfg.merged_behavior();
        assert_eq!(b["slideshow_delay"], 9);
        assert_eq!(b["max_load_dim"], MAX_LOAD_DIM);
    }

    #[test]
    fn color_validation() {
        assert!(validate_color("#00ffAA"));
        assert!(!validate_color("00ffAA"));
        assert!(!validate_color("#00ffAAB"));
        assert!(!validate_color("#00ffAG"));
    }
}
