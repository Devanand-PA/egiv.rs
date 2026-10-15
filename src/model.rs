//! File collection, filtering, sorting, captions and dmenu pairing.

use std::path::{Path, PathBuf};

pub const FF_MARK: u8 = 1;
pub const FF_WARN: u8 = 2;

pub const CC_STEPS: i32 = 32;
pub const GAMMA_MAX: f64 = 10.0;
pub const BRIGHTNESS_MAX: f64 = 2.0;
pub const CONTRAST_MAX: f64 = 4.0;

pub const ZOOM_LEVELS: [f64; 9] = [0.125, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0];
pub const ZOOM_MIN: f64 = ZOOM_LEVELS[0];
pub const ZOOM_MAX: f64 = ZOOM_LEVELS[8];

pub const DEF_ANIM_DELAY_MS: u32 = 75;

/// The recognised image extensions (same set as tkiv.py).
pub const IMAGE_EXTS: [&str; 23] = [
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".tif", ".webp", ".ppm", ".pgm", ".pbm",
    ".pnm", ".ico", ".jpe", ".jfif", ".pcx", ".tga", ".xpm", ".jp2", ".j2k", ".avif", ".heic",
    ".heif",
];

pub fn file_is_image<P: AsRef<Path>>(path: P) -> bool {
    let ext = path
        .as_ref()
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    IMAGE_EXTS.contains(&format!(".{ext}").as_str())
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub flags: u8,
    pub label: String,
}

impl FileEntry {
    pub fn new(path: String) -> Self {
        let label = Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        FileEntry {
            name: path.clone(),
            path,
            flags: 0,
            label,
        }
    }
    pub fn with_label(path: String, label: String) -> Self {
        FileEntry {
            name: path.clone(),
            path,
            flags: 0,
            label,
        }
    }
    pub fn marked(&self) -> bool {
        self.flags & FF_MARK != 0
    }
}

/// Recursively collect image files in a directory.
/// Mirrors tkiv.py collect_dir: per-directory names are sorted,
/// directories are recursed after the files of the parent, and when
/// sort_time is set each directory's own output is sorted by mtime,
/// newest first.
pub fn collect_dir(
    d: &Path,
    recursive: bool,
    include_hidden: bool,
    sort_time: bool,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(d) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return out,
    };
    entries.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .cmp(b.file_name().unwrap_or_default())
    });

    let mut subdirs: Vec<PathBuf> = Vec::new();
    for full in entries {
        let name = full.file_name().unwrap_or_default().to_string_lossy();
        if !include_hidden && name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&full) else {
            continue;
        };
        if meta.is_dir() {
            if recursive {
                subdirs.push(full);
            }
        } else if meta.is_file() && file_is_image(&full) {
            out.push(full);
        }
    }
    for s in subdirs {
        out.extend(collect_dir(&s, recursive, include_hidden, sort_time));
    }
    if sort_time {
        out.sort_by_key(|p| std::cmp::Reverse(mtime_nanos(p)));
    }
    out
}

pub fn mtime_nanos(p: &Path) -> i128 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- filtering

/// Rebuild visible indices from the filter text (all whitespace-separated
/// terms must appear in label or path, case-insensitive).
pub fn filter_visible(files: &[FileEntry], filter_text: &str, no_searchbar: bool) -> Vec<usize> {
    if no_searchbar {
        return (0..files.len()).collect();
    }
    let query = filter_text.to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        (0..files.len()).collect()
    } else {
        files
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                let lab = f.label.to_lowercase();
                let pth = f.path.to_lowercase();
                terms.iter().all(|t| lab.contains(t) || pth.contains(t))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

// ---------------------------------------------------------------- sorting

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    None,
    Name,
    Mtime,
    Size,
}

impl SortMode {
    pub fn parse(s: &str) -> SortMode {
        match s.to_lowercase().as_str() {
            "name" => SortMode::Name,
            "mtime" => SortMode::Mtime,
            "size" => SortMode::Size,
            _ => SortMode::None,
        }
    }
}

/// The six states of the Ctrl+Y sort cycle.
pub fn sort_cycle_next(mode: SortMode, reverse: bool) -> (SortMode, bool) {
    let cycle = [
        (SortMode::Name, false),
        (SortMode::Name, true),
        (SortMode::Mtime, false),
        (SortMode::Mtime, true),
        (SortMode::Size, false),
        (SortMode::Size, true),
    ];
    let idx = cycle
        .iter()
        .position(|&c| c == (mode, reverse))
        .map(|i| i as i64)
        .unwrap_or(-1);
    cycle[((idx + 1) % 6) as usize]
}

/// Sort entries in place.  For mtime/size the default direction is
/// "most relevant first" (newest/largest), so `reverse` is inverted.
/// Returns the new index of `cur_path` (or clamped `fallback`).
pub fn apply_sort(
    files: &mut Vec<FileEntry>,
    mode: SortMode,
    reverse: bool,
    cur_path: Option<&str>,
    fallback: usize,
) -> usize {
    if mode == SortMode::None || files.is_empty() {
        return fallback;
    }
    let mut indexed: Vec<FileEntry> = std::mem::take(files);
    match mode {
        SortMode::Name => {
            indexed.sort_by(|a, b| {
                let ord = a.label.to_lowercase().cmp(&b.label.to_lowercase());
                if reverse {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        SortMode::Mtime => {
            indexed.sort_by(|a, b| {
                let ka = mtime_nanos(Path::new(&a.path));
                let kb = mtime_nanos(Path::new(&b.path));
                // default (reverse=false): newest first => descending
                let ord = kb.cmp(&ka);
                if reverse {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        SortMode::Size => {
            indexed.sort_by(|a, b| {
                let ka = file_size(Path::new(&a.path));
                let kb = file_size(Path::new(&b.path));
                // default: largest first => descending
                let ord = kb.cmp(&ka);
                if reverse {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
        SortMode::None => {}
    }
    *files = indexed;
    if let Some(cp) = cur_path {
        files
            .iter()
            .position(|f| f.path == cp)
            .unwrap_or_else(|| fallback.min(files.len().saturating_sub(1)))
    } else {
        fallback.min(files.len().saturating_sub(1))
    }
}

pub fn file_size(p: &Path) -> i64 {
    std::fs::metadata(p).map(|m| m.len() as i64).unwrap_or(0)
}

pub fn sort_label(mode: SortMode, reverse: bool) -> String {
    if mode == SortMode::None {
        return String::new();
    }
    let name = match mode {
        SortMode::Name => "name",
        SortMode::Mtime => "date",
        SortMode::Size => "size",
        SortMode::None => unreachable!(),
    };
    let ascending = if mode == SortMode::Name {
        !reverse
    } else {
        reverse
    };
    format!(
        "{}{}",
        name,
        if ascending { '\u{2191}' } else { '\u{2193}' }
    )
}

// ---------------------------------------------------------------- adjustments

pub fn steps_to_range(d: i32, mx: f64, offset: f64) -> f64 {
    offset + (d as f64) * ((if d <= 0 { 1.0 } else { mx - 1.0 }) / CC_STEPS as f64)
}

// ---------------------------------------------------------------- captions

/// Port of tkiv.py _fit_caption: wrap a label to `max_w`/`max_h` given a
/// character width and line height, ellipsising overflow.
pub fn fit_caption(label: &str, max_w: f64, max_h: f64, char_w: f64, line_h: f64) -> String {
    if char_w <= 0.0 || line_h <= 0.0 || max_w <= 1.0 || max_h <= 1.0 {
        return label.to_string();
    }
    let cpl = (max_w / char_w) as usize; // int(max_w // char_w) for positive
    let cpl = cpl.max(1);
    let max_lines = ((max_h / line_h) as usize).max(1);
    let ell = '\u{2026}';

    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= cpl {
        return label.to_string();
    }

    let mut tokens: Vec<String> = Vec::new();
    for w in label.split_whitespace() {
        let mut w: Vec<char> = w.chars().collect();
        while w.len() > cpl {
            tokens.push(w[..cpl].iter().collect());
            w = w[cpl..].to_vec();
        }
        if !w.is_empty() {
            tokens.push(w.iter().collect());
        }
    }

    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut consumed = 0usize;
    let mut truncated = false;
    for w in &tokens {
        let wlen = w.chars().count();
        let curlen = cur.chars().count();
        if cur.is_empty() {
            cur = w.clone();
            consumed += 1;
        } else if curlen + 1 + wlen <= cpl {
            cur.push(' ');
            cur.push_str(w);
            consumed += 1;
        } else {
            lines.push(std::mem::take(&mut cur));
            if lines.len() >= max_lines {
                truncated = true;
                break;
            }
            cur = w.clone();
            consumed += 1;
        }
    }
    if !truncated {
        if !cur.is_empty() && lines.len() < max_lines {
            lines.push(std::mem::take(&mut cur));
        } else if !cur.is_empty() {
            truncated = true;
        }
    }
    if !truncated && consumed >= tokens.len() {
        return lines.join("\n");
    }
    if let Some(last) = lines.last_mut() {
        let lchars: Vec<char> = last.chars().collect();
        if lchars.len() >= cpl {
            *last = format!("{}{}", lchars[..cpl - 1].iter().collect::<String>(), ell);
        } else {
            *last = format!("{}{}", lchars.iter().collect::<String>(), ell);
        }
    } else if !cur.is_empty() {
        let cchars: Vec<char> = cur.chars().collect();
        if cchars.len() >= cpl {
            lines.push(format!(
                "{}{}",
                cchars[..cpl - 1].iter().collect::<String>(),
                ell
            ));
        } else {
            lines.push(format!("{}{}", cur, ell));
        }
    } else {
        return ell.to_string();
    }
    lines.join("\n")
}

// ---------------------------------------------------------------- dmenu

/// Pair list labels with image paths; the Python exits(1) with a message
/// when counts differ — here we return Err with the same message text.
pub fn dmenu_entries(
    list_entries: &[String],
    image_entries: &[String],
) -> Result<Vec<FileEntry>, String> {
    if list_entries.len() != image_entries.len() {
        return Err(format!(
            "Error: number of list entries ({}) and image entries ({}) do not match",
            list_entries.len(),
            image_entries.len()
        ));
    }
    Ok(list_entries
        .iter()
        .zip(image_entries.iter())
        .map(|(l, p)| FileEntry::with_label(p.clone(), l.clone()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_exts() {
        assert!(file_is_image("a.JPG"));
        assert!(file_is_image("a.png"));
        assert!(file_is_image("a.webp"));
        assert!(file_is_image("a.avif"));
        assert!(file_is_image("a.heic"));
        assert!(file_is_image("a.heif")); // .heif lowercases to .heif
        assert!(!file_is_image("a.txt"));
        assert!(!file_is_image("noext"));
    }

    #[test]
    fn filter_semantics() {
        let files = vec![
            FileEntry::new("/tmp/cats/fluffy.png".into()),
            FileEntry::new("/tmp/dogs/rex.png".into()),
            FileEntry::new("/tmp/cats/mittens.png".into()),
        ];
        assert_eq!(filter_visible(&files, "", false), vec![0, 1, 2]);
        assert_eq!(filter_visible(&files, "rex", false), vec![1]);
        assert_eq!(filter_visible(&files, "cats", false), vec![0, 2]);
        assert_eq!(filter_visible(&files, "cats mit", false), vec![2]);
        assert_eq!(filter_visible(&files, "nope", false), Vec::<usize>::new());
        // no_searchbar ignores the query
        assert_eq!(filter_visible(&files, "rex", true), vec![0, 1, 2]);
    }

    #[test]
    fn sort_cycle_six_states() {
        let mut m = (SortMode::None, false);
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Name, false));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Name, true));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Mtime, false));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Mtime, true));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Size, false));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Size, true));
        m = sort_cycle_next(m.0, m.1);
        assert_eq!(m, (SortMode::Name, false)); // wraps
    }

    #[test]
    fn sort_name_directions() {
        let mut files = vec![
            FileEntry::new("/x/b.png".into()),
            FileEntry::new("/x/A.png".into()),
            FileEntry::new("/x/c.png".into()),
        ];
        let idx = apply_sort(&mut files, SortMode::Name, false, Some("/x/A.png"), 0);
        let labels: Vec<&str> = files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, vec!["A.png", "b.png", "c.png"]);
        assert_eq!(idx, 0);
        apply_sort(&mut files, SortMode::Name, true, None, 0);
        let labels: Vec<&str> = files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, vec!["c.png", "b.png", "A.png"]);
    }

    #[test]
    fn sort_labels() {
        assert_eq!(sort_label(SortMode::None, false), "");
        assert_eq!(sort_label(SortMode::Name, false), "name\u{2191}");
        assert_eq!(sort_label(SortMode::Name, true), "name\u{2193}");
        assert_eq!(sort_label(SortMode::Mtime, false), "date\u{2193}");
        assert_eq!(sort_label(SortMode::Mtime, true), "date\u{2191}");
        assert_eq!(sort_label(SortMode::Size, true), "size\u{2191}");
    }

    #[test]
    fn steps_to_range_math() {
        assert_eq!(steps_to_range(0, GAMMA_MAX, 1.0), 1.0);
        assert!((steps_to_range(-CC_STEPS, GAMMA_MAX, 1.0) - 0.0).abs() < 1e-9);
        assert!((steps_to_range(CC_STEPS, GAMMA_MAX, 1.0) - GAMMA_MAX).abs() < 1e-9);
        assert_eq!(steps_to_range(0, BRIGHTNESS_MAX, 0.0), 0.0);
        assert!(
            (steps_to_range(CC_STEPS, BRIGHTNESS_MAX, 0.0) - (BRIGHTNESS_MAX - 1.0)).abs() < 1e-9
        );
    }

    #[test]
    fn caption_short_label_passthrough() {
        assert_eq!(fit_caption("short", 200.0, 40.0, 8.0, 16.0), "short");
        assert_eq!(fit_caption("any", 0.0, 40.0, 8.0, 16.0), "any");
    }

    #[test]
    fn caption_wraps_and_ellipsizes() {
        // cpl = 100/10 = 10 chars per line; max_lines = 32/16 = 2
        let out = fit_caption("aaa bbb ccc ddd eee fff", 100.0, 32.0, 10.0, 16.0);
        let lines: Vec<&str> = out.split('\n').collect();
        assert!(lines.len() <= 2);
        assert!(out.contains('\u{2026}'));
        let full = fit_caption("aaa bbb ccc ddd", 100.0, 32.0, 10.0, 16.0);
        assert_eq!(full, "aaa bbb\nccc ddd");
    }

    #[test]
    fn dmenu_pairing() {
        let ls = vec!["one".to_string(), "two".to_string()];
        let ps = vec!["/a.png".to_string(), "/b.png".to_string()];
        let e = dmenu_entries(&ls, &ps).unwrap();
        assert_eq!(e[0].label, "one");
        assert_eq!(e[0].path, "/a.png");
        assert_eq!(e[1].label, "two");
        let err = dmenu_entries(&ls[..1], &ps).unwrap_err();
        assert_eq!(
            err,
            "Error: number of list entries (1) and image entries (2) do not match"
        );
    }
}
