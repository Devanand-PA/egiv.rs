//! CLI parsing matching tkiv.py's argparse setup — same flags, same
//! aliases, same defaults.  Hand-rolled so that every alias (including
//! the odd ones like -g/-t/--gallery/--thumbnail) is accepted as in the
//! Python version.

#[derive(Debug, Clone, Default)]
pub struct SharedOpts {
    pub thumb_mode: bool,        // -g -t --gallery --thumbnail
    pub list_mode: bool,         // -l --list-mode
    pub thumb_size: Option<u32>, // -T --gallery-tile-size --thumb-size
    pub gallery_rows: Option<f64>,
    pub gallery_cols: Option<u32>,
    pub gallery_aspect: Option<f64>,
    pub lazy: bool,
    pub recursive: bool,      // -r
    pub hidden: bool,         // -H
    pub time: bool,           // --time
    pub sort: String,         // --sort none|name|mtime|size (default none)
    pub sort_reverse: bool,   // -R --sort-reverse
    pub start_at: i64,        // -n --start-at --pass-idx (default 1)
    pub name: Option<String>, // --name --custom-title
    pub idx_write_path: Option<String>,
    pub pre_select: Option<String>,
    pub pre_select_file: Option<String>,
    pub no_searchbar: bool, // --no-searchbar --no-search --direct-keys
    pub quiet: bool,        // -q --quiet
    pub version: bool,      // -v --version
    pub help: bool,         // -h --help
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ViewerOpts {
    pub shared: SharedOpts,
    pub animate: bool,  // -a
    pub framerate: i64, // -A (accepted, no-op in Rust port)
    pub assume_files: bool,
    pub no_bar: bool, // -b
    pub bar: bool,    // --bar
    pub floating_window: bool,
    pub clean_cache: bool, // -c (accepted, no-op)
    pub embed: i64,        // -e WID (accepted, no-op)
    pub fullscreen: bool,  // -f
    pub gamma: i32,        // -G
    pub geometry: Option<String>,
    pub from_stdin: bool,            // -i
    pub legacy_name: Option<String>, // -N
    pub class_: Option<String>,      // --class (deprecated)
    pub stdout: bool,                // -o
    pub private_mode: bool,          // -p (accepted, no-op)
    pub ss_delay: f64,               // -S seconds
    pub scale_mode: char,            // -s
    pub zoom: i64,                   // -z percent
    pub zoom_100: bool,              // -Z
    pub using_null: bool,            // -0
    pub anti_alias: String,          // --anti-alias [yes|no] (nargs=?)
    pub alpha_layer: Option<String>, // --alpha-layer [yes|no]
    pub cache_allow: Option<String>, // accepted, no-op
    pub cache_deny: Option<String>,  // accepted, no-op
    pub update_cache: bool,          // accepted, no-op
}

#[derive(Debug, Clone)]
pub struct SelectorOpts {
    pub shared: SharedOpts,
    pub dmenu_mode: bool,
    pub list_file: Option<String>,
    pub list_entries: Option<String>,
    pub image_file: Option<String>,
    pub image_entries: Option<String>,
    pub return_label: bool,
    pub using_null: bool, // -0 via shared flags
}

impl Default for ViewerOpts {
    fn default() -> Self {
        ViewerOpts {
            shared: SharedOpts {
                sort: "none".into(),
                start_at: 1,
                ..Default::default()
            },
            animate: false,
            framerate: 0,
            assume_files: false,
            no_bar: false,
            bar: false,
            floating_window: false,
            clean_cache: false,
            embed: 0,
            fullscreen: false,
            gamma: 0,
            geometry: None,
            from_stdin: false,
            legacy_name: None,
            class_: None,
            stdout: false,
            private_mode: false,
            ss_delay: 0.0,
            scale_mode: 'd',
            zoom: 0,
            zoom_100: false,
            using_null: false,
            anti_alias: "yes".into(),
            alpha_layer: None,
            cache_allow: None,
            cache_deny: None,
            update_cache: false,
        }
    }
}

impl Default for SelectorOpts {
    fn default() -> Self {
        SelectorOpts {
            shared: SharedOpts {
                sort: "none".into(),
                start_at: 1,
                ..Default::default()
            },
            dmenu_mode: false,
            list_file: None,
            list_entries: None,
            image_file: None,
            image_entries: None,
            return_label: false,
            using_null: false,
        }
    }
}

pub(crate) enum Need {
    Str,
    Int,
    Float,
}

pub(crate) struct FlagSpec {
    action: &'static str,
    need: Option<Need>,
}

fn viewer_flags() -> Vec<(Vec<&'static str>, FlagSpec)> {
    use Need::*;
    let mut v: Vec<(Vec<&'static str>, FlagSpec)> = vec![
        (
            vec!["-a", "--animate"],
            FlagSpec {
                action: "animate",
                need: None,
            },
        ),
        (
            vec!["-A", "--framerate"],
            FlagSpec {
                action: "framerate",
                need: Some(Int),
            },
        ),
        (
            vec!["--assume-files"],
            FlagSpec {
                action: "assume_files",
                need: None,
            },
        ),
        (
            vec!["-b", "--no-bar"],
            FlagSpec {
                action: "no_bar",
                need: None,
            },
        ),
        (
            vec!["--bar"],
            FlagSpec {
                action: "bar",
                need: None,
            },
        ),
        (
            vec!["--floating-window"],
            FlagSpec {
                action: "floating_window",
                need: None,
            },
        ),
        (
            vec!["-c", "--clean-cache"],
            FlagSpec {
                action: "clean_cache",
                need: None,
            },
        ),
        (
            vec!["-e", "--embed"],
            FlagSpec {
                action: "embed",
                need: Some(Int),
            },
        ),
        (
            vec!["-f", "--fullscreen"],
            FlagSpec {
                action: "fullscreen",
                need: None,
            },
        ),
        (
            vec!["-G", "--gamma"],
            FlagSpec {
                action: "gamma",
                need: Some(Int),
            },
        ),
        (
            vec!["--geometry"],
            FlagSpec {
                action: "geometry",
                need: Some(Str),
            },
        ),
        (
            vec!["-i", "--stdin"],
            FlagSpec {
                action: "from_stdin",
                need: None,
            },
        ),
        (
            vec!["-N", "--legacy-name"],
            FlagSpec {
                action: "legacy_name",
                need: Some(Str),
            },
        ),
        (
            vec!["--class"],
            FlagSpec {
                action: "class_",
                need: Some(Str),
            },
        ),
        (
            vec!["-o", "--stdout"],
            FlagSpec {
                action: "stdout",
                need: None,
            },
        ),
        (
            vec!["-p", "--private"],
            FlagSpec {
                action: "private_mode",
                need: None,
            },
        ),
        (
            vec!["-S", "--ss-delay"],
            FlagSpec {
                action: "ss_delay",
                need: Some(Float),
            },
        ),
        (
            vec!["-s", "--scale-mode"],
            FlagSpec {
                action: "scale_mode",
                need: Some(Str),
            },
        ),
        (
            vec!["-z", "--zoom"],
            FlagSpec {
                action: "zoom",
                need: Some(Int),
            },
        ),
        (
            vec!["-Z", "--zoom-100"],
            FlagSpec {
                action: "zoom_100",
                need: None,
            },
        ),
        (
            vec!["-0", "--null"],
            FlagSpec {
                action: "using_null",
                need: None,
            },
        ),
        (
            vec!["--anti-alias"],
            FlagSpec {
                action: "anti_alias",
                need: Some(Str),
            },
        ), // optional value
        (
            vec!["--alpha-layer"],
            FlagSpec {
                action: "alpha_layer",
                need: Some(Str),
            },
        ), // optional value
        (
            vec!["--cache-allow"],
            FlagSpec {
                action: "cache_allow",
                need: Some(Str),
            },
        ),
        (
            vec!["--cache-deny"],
            FlagSpec {
                action: "cache_deny",
                need: Some(Str),
            },
        ),
        (
            vec!["--update-cache"],
            FlagSpec {
                action: "update_cache",
                need: None,
            },
        ),
    ];
    v.extend(shared_flags());
    v
}

fn shared_flags() -> Vec<(Vec<&'static str>, FlagSpec)> {
    use Need::*;
    vec![
        (
            vec!["-g", "-t", "--gallery", "--thumbnail"],
            FlagSpec {
                action: "thumb_mode",
                need: None,
            },
        ),
        (
            vec!["-l", "--list-mode"],
            FlagSpec {
                action: "list_mode",
                need: None,
            },
        ),
        (
            vec!["-T", "--gallery-tile-size", "--thumb-size"],
            FlagSpec {
                action: "thumb_size",
                need: Some(Int),
            },
        ),
        (
            vec!["--gallery-rows"],
            FlagSpec {
                action: "gallery_rows",
                need: Some(Float),
            },
        ),
        (
            vec!["--gallery-cols"],
            FlagSpec {
                action: "gallery_cols",
                need: Some(Int),
            },
        ),
        (
            vec!["--gallery-aspect"],
            FlagSpec {
                action: "gallery_aspect",
                need: Some(Float),
            },
        ),
        (
            vec!["--lazy"],
            FlagSpec {
                action: "lazy",
                need: None,
            },
        ),
        (
            vec!["-r", "--recursive"],
            FlagSpec {
                action: "recursive",
                need: None,
            },
        ),
        (
            vec!["-H", "--hidden"],
            FlagSpec {
                action: "hidden",
                need: None,
            },
        ),
        (
            vec!["--time"],
            FlagSpec {
                action: "time",
                need: None,
            },
        ),
        (
            vec!["--sort"],
            FlagSpec {
                action: "sort",
                need: Some(Str),
            },
        ),
        (
            vec!["-R", "--sort-reverse"],
            FlagSpec {
                action: "sort_reverse",
                need: None,
            },
        ),
        (
            vec!["-n", "--start-at", "--pass-idx"],
            FlagSpec {
                action: "start_at",
                need: Some(Int),
            },
        ),
        (
            vec!["--name", "--custom-title"],
            FlagSpec {
                action: "name",
                need: Some(Str),
            },
        ),
        (
            vec!["--idx-write-path"],
            FlagSpec {
                action: "idx_write_path",
                need: Some(Str),
            },
        ),
        (
            vec!["--pre-select"],
            FlagSpec {
                action: "pre_select",
                need: Some(Str),
            },
        ),
        (
            vec!["--pre-select-file"],
            FlagSpec {
                action: "pre_select_file",
                need: Some(Str),
            },
        ),
        (
            vec!["--no-searchbar", "--no-search", "--direct-keys"],
            FlagSpec {
                action: "no_searchbar",
                need: None,
            },
        ),
        (
            vec!["-q", "--quiet"],
            FlagSpec {
                action: "quiet",
                need: None,
            },
        ),
        (
            vec!["-v", "--version"],
            FlagSpec {
                action: "version",
                need: None,
            },
        ),
        (
            vec!["-h", "--help"],
            FlagSpec {
                action: "help",
                need: None,
            },
        ),
    ]
}

fn selector_flags() -> Vec<(Vec<&'static str>, FlagSpec)> {
    use Need::*;
    let mut v = vec![
        (
            vec!["--dmenu-mode"],
            FlagSpec {
                action: "dmenu_mode",
                need: None,
            },
        ),
        (
            vec!["--list-file"],
            FlagSpec {
                action: "list_file",
                need: Some(Str),
            },
        ),
        (
            vec!["--list-entries"],
            FlagSpec {
                action: "list_entries",
                need: Some(Str),
            },
        ),
        (
            vec!["--image-file"],
            FlagSpec {
                action: "image_file",
                need: Some(Str),
            },
        ),
        (
            vec!["--image-entries"],
            FlagSpec {
                action: "image_entries",
                need: Some(Str),
            },
        ),
        (
            vec!["--return-label"],
            FlagSpec {
                action: "return_label",
                need: None,
            },
        ),
        (
            vec!["-0", "--null"],
            FlagSpec {
                action: "using_null",
                need: None,
            },
        ),
    ];
    v.extend(shared_flags());
    v
}

pub struct ParseResult {
    pub bools: std::collections::HashMap<String, bool>,
    pub strings: std::collections::HashMap<String, String>,
    pub positionals: Vec<String>,
}

/// Generic small parser: exact match flags (no abbreviation, like
/// allow_abbrev=False), `--flag=value` accepted, `-x value` or combined
/// short flag `-xVALUE`?  argparse does not combine; we keep it simple:
/// exact tokens only.
pub(crate) fn parse_args(
    specs: &[(Vec<&'static str>, FlagSpec)],
    args: &[String],
    prog: &str,
) -> Result<ParseResult, String> {
    let mut lookup: std::collections::HashMap<&str, &FlagSpec> = std::collections::HashMap::new();
    for (names, spec) in specs {
        for n in names {
            lookup.insert(n, spec);
        }
    }
    let mut res = ParseResult {
        bools: Default::default(),
        strings: Default::default(),
        positionals: vec![],
    };
    let mut i = 0;
    let mut only_positional = false;
    while i < args.len() {
        let a = &args[i];
        if only_positional {
            res.positionals.push(a.clone());
            i += 1;
            continue;
        }
        if a == "--" {
            only_positional = true;
            i += 1;
            continue;
        }
        // --flag=value form
        let (tok, inline_val) = match a.split_once('=') {
            Some((t, v)) if t.starts_with("--") => (t, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        if tok.starts_with('-') && tok != "-" {
            let Some(spec) = lookup.get(tok) else {
                return Err(format!("{prog}: error: unrecognized arguments: {a}"));
            };
            match &spec.need {
                None => {
                    if inline_val.is_some() {
                        return Err(format!(
                            "{prog}: error: argument {tok}: ignored explicit argument"
                        ));
                    }
                    res.bools.insert(spec.action.to_string(), true);
                }
                Some(Need::Str) => {
                    let v = match inline_val {
                        Some(v) => v,
                        None => {
                            // --anti-alias / --alpha-layer have nargs='?':
                            // value optional.
                            let optional =
                                spec.action == "anti_alias" || spec.action == "alpha_layer";
                            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                                i += 1;
                                args[i].clone()
                            } else if optional {
                                if spec.action == "anti_alias" {
                                    "yes".to_string()
                                } else {
                                    "no".to_string()
                                }
                            } else {
                                return Err(format!(
                                    "{prog}: error: argument {tok}: expected one argument"
                                ));
                            }
                        }
                    };
                    res.strings.insert(spec.action.to_string(), v);
                }
                Some(Need::Int) => {
                    let v = match inline_val {
                        Some(v) => v,
                        None => {
                            if i + 1 >= args.len() {
                                return Err(format!(
                                    "{prog}: error: argument {tok}: expected one argument"
                                ));
                            }
                            i += 1;
                            args[i].clone()
                        }
                    };
                    // validate int now to mimic argparse
                    if v.parse::<i64>().is_err() {
                        return Err(format!(
                            "{prog}: error: argument {tok}: invalid int value: '{v}'"
                        ));
                    }
                    res.strings.insert(spec.action.to_string(), v);
                }
                Some(Need::Float) => {
                    let v = match inline_val {
                        Some(v) => v,
                        None => {
                            if i + 1 >= args.len() {
                                return Err(format!(
                                    "{prog}: error: argument {tok}: expected one argument"
                                ));
                            }
                            i += 1;
                            args[i].clone()
                        }
                    };
                    if v.parse::<f64>().is_err() {
                        return Err(format!(
                            "{prog}: error: argument {tok}: invalid float value: '{v}'"
                        ));
                    }
                    res.strings.insert(spec.action.to_string(), v);
                }
            }
        } else {
            res.positionals.push(a.clone());
        }
        i += 1;
    }
    Ok(res)
}

impl ViewerOpts {
    fn from_parse(p: ParseResult) -> Result<ViewerOpts, String> {
        let mut o = ViewerOpts::default();
        macro_rules! b {
            ($n:expr, $f:ident) => {
                o.$f = p.bools.get($n).copied().unwrap_or(false)
            };
        }
        macro_rules! sb {
            ($n:expr, $f:ident) => {
                o.shared.$f = p.bools.get($n).copied().unwrap_or(false)
            };
        }
        b!("animate", animate);
        b!("assume_files", assume_files);
        b!("no_bar", no_bar);
        b!("bar", bar);
        b!("floating_window", floating_window);
        b!("clean_cache", clean_cache);
        b!("fullscreen", fullscreen);
        b!("from_stdin", from_stdin);
        b!("stdout", stdout);
        b!("private_mode", private_mode);
        b!("zoom_100", zoom_100);
        b!("using_null", using_null);
        b!("update_cache", update_cache);
        sb!("thumb_mode", thumb_mode);
        sb!("list_mode", list_mode);
        sb!("lazy", lazy);
        sb!("recursive", recursive);
        sb!("hidden", hidden);
        sb!("time", time);
        sb!("sort_reverse", sort_reverse);
        sb!("no_searchbar", no_searchbar);
        sb!("quiet", quiet);
        sb!("version", version);
        sb!("help", help);

        macro_rules! s {
            ($n:expr, $f:ident) => {
                if let Some(v) = p.strings.get($n) {
                    o.$f = v.clone();
                }
            };
        }
        macro_rules! so {
            ($n:expr, $f:ident) => {
                o.$f = p.strings.get($n).cloned()
            };
        }
        macro_rules! i {
            ($n:expr, $f:ident) => {
                if let Some(v) = p.strings.get($n) {
                    o.$f = v.parse().map_err(|_| format!("bad int {v}"))?;
                }
            };
        }
        macro_rules! f {
            ($n:expr, $f:ident) => {
                if let Some(v) = p.strings.get($n) {
                    o.$f = v.parse().map_err(|_| format!("bad float {v}"))?;
                }
            };
        }
        i!("framerate", framerate);
        i!("embed", embed);
        i!("gamma", gamma);
        f!("ss_delay", ss_delay);
        i!("zoom", zoom);
        s!("anti_alias", anti_alias);
        o.alpha_layer = p.strings.get("alpha_layer").cloned();
        so!("geometry", geometry);
        so!("legacy_name", legacy_name);
        so!("class_", class_);
        so!("cache_allow", cache_allow);
        so!("cache_deny", cache_deny);
        if let Some(v) = p.strings.get("scale_mode") {
            o.scale_mode = v.chars().next().unwrap_or('d');
        }
        if let Some(v) = p.strings.get("sort") {
            match v.as_str() {
                "none" | "name" | "mtime" | "size" => o.shared.sort = v.clone(),
                _ => return Err(format!(
                    "tkiv img: error: argument --sort: invalid choice: '{v}' (choose from 'none', 'name', 'mtime', 'size')"
                )),
            }
        }
        if let Some(v) = p.strings.get("start_at") {
            o.shared.start_at = v.parse().unwrap_or(1);
        }
        o.shared.thumb_size = p.strings.get("thumb_size").and_then(|v| v.parse().ok());
        o.shared.gallery_rows = p.strings.get("gallery_rows").and_then(|v| v.parse().ok());
        o.shared.gallery_cols = p.strings.get("gallery_cols").and_then(|v| v.parse().ok());
        o.shared.gallery_aspect = p.strings.get("gallery_aspect").and_then(|v| v.parse().ok());
        o.shared.name = p.strings.get("name").cloned();
        o.shared.idx_write_path = p.strings.get("idx_write_path").cloned();
        o.shared.pre_select = p.strings.get("pre_select").cloned();
        o.shared.pre_select_file = p.strings.get("pre_select_file").cloned();
        o.shared.files = p.positionals;
        Ok(o)
    }
}

impl SelectorOpts {
    fn from_parse(p: ParseResult) -> Result<SelectorOpts, String> {
        let mut o = SelectorOpts::default();
        macro_rules! b {
            ($n:expr, $f:ident) => {
                o.$f = p.bools.get($n).copied().unwrap_or(false)
            };
        }
        macro_rules! sb {
            ($n:expr, $f:ident) => {
                o.shared.$f = p.bools.get($n).copied().unwrap_or(false)
            };
        }
        b!("dmenu_mode", dmenu_mode);
        b!("return_label", return_label);
        b!("using_null", using_null);
        sb!("thumb_mode", thumb_mode);
        sb!("list_mode", list_mode);
        sb!("lazy", lazy);
        sb!("recursive", recursive);
        sb!("hidden", hidden);
        sb!("time", time);
        sb!("sort_reverse", sort_reverse);
        sb!("no_searchbar", no_searchbar);
        sb!("quiet", quiet);
        sb!("version", version);
        sb!("help", help);

        o.list_file = p.strings.get("list_file").cloned();
        o.list_entries = p.strings.get("list_entries").cloned();
        o.image_file = p.strings.get("image_file").cloned();
        o.image_entries = p.strings.get("image_entries").cloned();
        if let Some(v) = p.strings.get("sort") {
            match v.as_str() {
                "none" | "name" | "mtime" | "size" => o.shared.sort = v.clone(),
                _ => return Err(format!(
                    "tkiv select: error: argument --sort: invalid choice: '{v}' (choose from 'none', 'name', 'mtime', 'size')"
                )),
            }
        }
        if let Some(v) = p.strings.get("start_at") {
            o.shared.start_at = v.parse().unwrap_or(1);
        }
        o.shared.thumb_size = p.strings.get("thumb_size").and_then(|v| v.parse().ok());
        o.shared.gallery_rows = p.strings.get("gallery_rows").and_then(|v| v.parse().ok());
        o.shared.gallery_cols = p.strings.get("gallery_cols").and_then(|v| v.parse().ok());
        o.shared.gallery_aspect = p.strings.get("gallery_aspect").and_then(|v| v.parse().ok());
        o.shared.name = p.strings.get("name").cloned();
        o.shared.idx_write_path = p.strings.get("idx_write_path").cloned();
        o.shared.pre_select = p.strings.get("pre_select").cloned();
        o.shared.pre_select_file = p.strings.get("pre_select_file").cloned();
        o.shared.files = p.positionals;
        Ok(o)
    }
}

pub fn parse_viewer_args(args: &[String]) -> Result<ViewerOpts, String> {
    ViewerOpts::from_parse(parse_args(&viewer_flags(), args, "tkiv img")?)
}

pub fn parse_selector_args(args: &[String]) -> Result<SelectorOpts, String> {
    SelectorOpts::from_parse(parse_args(&selector_flags(), args, "tkiv select")?)
}

/// Read pre-select labels exactly like tkiv.py's _read_pre_select.
pub fn read_pre_select(
    pre: &Option<String>,
    pre_file: &Option<String>,
) -> Result<Option<Vec<String>>, String> {
    if pre.is_some() && pre_file.is_some() {
        return Err("Error: both --pre-select and --pre-select-file provided.".into());
    }
    if let Some(f) = pre_file {
        let text = std::fs::read_to_string(f)
            .map_err(|e| format!("Error reading pre-select file: {e}"))?;
        return Ok(Some(
            text.lines()
                .map(|l| l.trim_end_matches('\n').to_string())
                .collect(),
        ));
    }
    if let Some(p) = pre {
        return Ok(Some(p.split('\n').map(|s| s.to_string()).collect()));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(args: &[&str]) -> ViewerOpts {
        parse_viewer_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }
    fn ps(args: &[&str]) -> SelectorOpts {
        parse_selector_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn viewer_aliases() {
        let o = pv(&["-g", "a.png"]);
        assert!(o.shared.thumb_mode);
        let o = pv(&["--thumbnail", "a.png"]);
        assert!(o.shared.thumb_mode);
        let o = pv(&["-t", "--list-mode", "a.png"]);
        assert!(o.shared.thumb_mode && o.shared.list_mode);
    }

    #[test]
    fn viewer_values() {
        let o = pv(&[
            "-n",
            "5",
            "-T",
            "320",
            "--sort",
            "mtime",
            "-R",
            "-z",
            "150",
            "-s",
            "f",
            "-G",
            "-10",
            "-S",
            "2.5",
            "--gallery-rows",
            "4.5",
            "--gallery-cols",
            "6",
            "--gallery-aspect",
            "1.7",
            "x.png",
        ]);
        assert_eq!(o.shared.start_at, 5);
        assert_eq!(o.shared.thumb_size, Some(320));
        assert_eq!(o.shared.sort, "mtime");
        assert!(o.shared.sort_reverse);
        assert_eq!(o.zoom, 150);
        assert_eq!(o.scale_mode, 'f');
        assert_eq!(o.gamma, -10);
        assert!((o.ss_delay - 2.5).abs() < 1e-9);
        assert!((o.shared.gallery_rows.unwrap() - 4.5).abs() < 1e-9);
        assert_eq!(o.shared.gallery_cols, Some(6));
        assert!((o.shared.gallery_aspect.unwrap() - 1.7).abs() < 1e-9);
    }

    #[test]
    fn viewer_noop_flags_accepted() {
        let o = pv(&[
            "-A",
            "30",
            "-c",
            "-e",
            "12345",
            "-p",
            "--cache-allow",
            "*.png",
            "--cache-deny",
            "*.gif",
            "--update-cache",
            "--private",
            "a.png",
        ]);
        assert_eq!(o.framerate, 30);
        assert!(o.clean_cache);
        assert_eq!(o.embed, 12345);
        assert!(o.private_mode && o.update_cache);
        assert_eq!(o.cache_allow.as_deref(), Some("*.png"));
    }

    #[test]
    fn viewer_optional_value_flags() {
        // argparse nargs='?' greedily consumes the next non-option token.
        let o = pv(&["--anti-alias"]);
        assert_eq!(o.anti_alias, "yes");
        let o = pv(&["--anti-alias=no", "a.png"]);
        assert_eq!(o.anti_alias, "no");
        let o = pv(&["--alpha-layer", "--", "a.png"]);
        assert_eq!(o.alpha_layer.as_deref(), Some("no"));
        let o = pv(&["--alpha-layer", "yes", "a.png"]);
        assert_eq!(o.alpha_layer.as_deref(), Some("yes"));
        // greedy consume like argparse: 'a.png' becomes the option value
        let o = pv(&["--anti-alias", "a.png"]);
        assert_eq!(o.anti_alias, "a.png");
        assert!(o.shared.files.is_empty());
    }

    #[test]
    fn viewer_unknown_flag_errors() {
        assert!(parse_viewer_args(&["--nope".to_string()]).is_err());
        assert!(parse_viewer_args(&["--sort".to_string(), "bogus".to_string()]).is_err());
        assert!(parse_viewer_args(&["-n".to_string(), "abc".to_string()]).is_err());
    }

    #[test]
    fn selector_dmenu_flags() {
        let o = ps(&[
            "--dmenu-mode",
            "--list-entries",
            "a\nb",
            "--image-entries",
            "/a.png\n/b.png",
            "--return-label",
        ]);
        assert!(o.dmenu_mode && o.return_label);
        assert_eq!(o.list_entries.as_deref(), Some("a\nb"));
    }

    #[test]
    fn selector_positional_default_kept() {
        let o = ps(&[]);
        assert!(o.shared.files.is_empty()); // default '.' applied at call site
    }

    #[test]
    fn pre_select_conflict() {
        let err = read_pre_select(&Some("a".into()), &Some("f".into())).unwrap_err();
        assert_eq!(
            err,
            "Error: both --pre-select and --pre-select-file provided."
        );
        let v = read_pre_select(&Some("a\nb".into()), &None)
            .unwrap()
            .unwrap();
        assert_eq!(v, vec!["a", "b"]);
    }
}
