//! Key-spec parsing compatible with tkiv.py.
//!
//! Human-friendly specs like "Ctrl+Shift+W" are translated to canonical
//! Tk-style sequences like "<Control-Shift-w>".  In direct-key mode the
//! Control/Alt/Meta modifiers are stripped (and Shift+letter becomes an
//! uppercase letter).

use std::collections::HashMap;
use std::sync::OnceLock;

fn mod_aliases() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| {
        HashMap::from([
            ("ctrl", "Control"),
            ("control", "Control"),
            ("shift", "Shift"),
            ("alt", "Alt"),
            ("meta", "Meta"),
            ("cmd", "Meta"),
            ("super", "Meta"),
            ("command", "Meta"),
        ])
    })
}

fn key_aliases() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = HashMap::from([
            ("space", "space"),
            ("plus", "plus"),
            ("+", "plus"),
            ("minus", "minus"),
            ("-", "minus"),
            ("equal", "equal"),
            ("=", "equal"),
            ("bracketleft", "bracketleft"),
            ("[", "bracketleft"),
            ("bracketright", "bracketright"),
            ("]", "bracketright"),
            ("braceleft", "braceleft"),
            ("{", "braceleft"),
            ("braceright", "braceright"),
            ("}", "braceright"),
            ("parenleft", "parenleft"),
            ("(", "parenleft"),
            ("parenright", "parenright"),
            (")", "parenright"),
            ("less", "less"),
            ("<", "less"),
            ("greater", "greater"),
            (">", "greater"),
            ("question", "question"),
            ("?", "question"),
            ("bar", "bar"),
            ("|", "bar"),
            ("underscore", "underscore"),
            ("_", "underscore"),
            ("return", "Return"),
            ("enter", "Return"),
            ("escape", "Escape"),
            ("esc", "Escape"),
            ("tab", "Tab"),
            ("backspace", "BackSpace"),
            ("bs", "BackSpace"),
            ("delete", "Delete"),
            ("del", "Delete"),
            ("home", "Home"),
            ("end", "End"),
            ("pageup", "Prior"),
            ("pgup", "Prior"),
            ("prior", "Prior"),
            ("pagedown", "Next"),
            ("pgdn", "Next"),
            ("next", "Next"),
            ("up", "Up"),
            ("down", "Down"),
            ("left", "Left"),
            ("right", "Right"),
            ("insert", "Insert"),
            ("ins", "Insert"),
            ("pause", "Pause"),
            ("print", "Print"),
            ("kp_add", "KP_Add"),
            ("kp_subtract", "KP_Subtract"),
            ("kp_enter", "KP_Enter"),
            ("kp_multiply", "KP_Multiply"),
            ("kp_divide", "KP_Divide"),
            ("kp_decimal", "KP_Decimal"),
            ("kp_0", "KP_0"),
            ("kp_1", "KP_1"),
            ("kp_2", "KP_2"),
            ("kp_3", "KP_3"),
            ("kp_4", "KP_4"),
            ("kp_5", "KP_5"),
            ("kp_6", "KP_6"),
            ("kp_7", "KP_7"),
            ("kp_8", "KP_8"),
            ("kp_9", "KP_9"),
        ]);
        for i in 1..=12 {
            m.insert(
                Box::leak(format!("f{i}").into_boxed_str()) as &'static str,
                Box::leak(format!("F{i}").into_boxed_str()) as &'static str,
            );
        }
        m
    })
}

/// Split 'Ctrl+Shift+W' into ['Ctrl', 'Shift', 'W'].
/// A '+' key in its own right (e.g. from 'Ctrl++') is kept as '+'.
pub fn split_key_spec(spec: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in spec.chars() {
        if ch == '+' {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            } else {
                parts.push("+".to_string());
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Translate a human-friendly spec like 'Ctrl+Shift+W' into a Tk
/// sequence like '<Control-Shift-w>'.  Returns None on error.
pub fn parse_key_spec(spec: &str) -> Option<String> {
    let parts = split_key_spec(spec);
    if parts.is_empty() {
        return None;
    }
    let mods_alias = mod_aliases();
    let keys_alias = key_aliases();
    let mut mods: Vec<&'static str> = Vec::new();
    let mut key: Option<String> = None;
    for p in &parts {
        let low = p.to_lowercase();
        if let Some(&m) = mods_alias.get(low.as_str()) {
            if !mods.contains(&m) {
                mods.push(m);
            }
        } else {
            if key.is_some() {
                return None; // two non-modifier keys
            }
            key = Some(p.clone());
        }
    }
    let key = key?;
    let klow = key.to_lowercase();
    let key_name: String = if let Some(&k) = keys_alias.get(klow.as_str()) {
        k.to_string()
    } else if key.chars().count() == 1 {
        let c = key.chars().next().unwrap();
        if c.is_alphabetic() {
            if mods.contains(&"Shift") {
                key.to_uppercase()
            } else {
                key.to_lowercase()
            }
        } else {
            key.clone()
        }
    } else {
        return None;
    };
    let mut s = String::from("<");
    for m in &mods {
        s.push_str(m);
        s.push('-');
    }
    s.push_str(&key_name);
    s.push('>');
    Some(s)
}

/// Turn '<Control-Shift-w>' into '<Control-Shift-KeyRelease-w>'.
pub fn keyrelease_seq(tk_seq: &str) -> String {
    let inner = &tk_seq[1..tk_seq.len() - 1];
    let parts: Vec<&str> = inner.split('-').collect();
    let n = parts.len();
    let mut out = String::from("<");
    for p in &parts[..n - 1] {
        out.push_str(p);
        out.push('-');
    }
    out.push_str("KeyRelease-");
    out.push_str(parts[n - 1]);
    out.push('>');
    out
}

/// Produce the direct-key sequence by dropping Control/Alt/Meta and
/// uppercasing letters that had Shift.
pub fn to_direct_key(tk_seq: &str) -> Option<String> {
    if !(tk_seq.starts_with('<') && tk_seq.ends_with('>')) {
        return None;
    }
    let inner = &tk_seq[1..tk_seq.len() - 1];
    if inner.is_empty() {
        return None;
    }
    let parts: Vec<&str> = inner.split('-').collect();
    let key = parts[parts.len() - 1];
    let mods = &parts[..parts.len() - 1];
    let keep_shift = mods.contains(&"Shift");
    if key.chars().count() == 1 && key.chars().next().unwrap().is_alphabetic() && keep_shift {
        return Some(format!("<{}>", key.to_uppercase()));
    }
    if key.chars().count() == 1 && key.chars().next().unwrap().is_alphabetic() {
        return Some(format!("<{}>", key.to_lowercase()));
    }
    if keep_shift {
        return Some(format!("<Shift-{key}>"));
    }
    Some(format!("<{key}>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple() {
        assert_eq!(split_key_spec("Ctrl+Shift+W"), vec!["Ctrl", "Shift", "W"]);
        assert_eq!(split_key_spec("q"), vec!["q"]);
    }

    #[test]
    fn split_plus_key() {
        // 'Ctrl++' must produce a '+' key in its own right.
        assert_eq!(split_key_spec("Ctrl++"), vec!["Ctrl", "+"]);
    }

    #[test]
    fn parse_basic() {
        assert_eq!(parse_key_spec("Ctrl+Q").as_deref(), Some("<Control-q>"));
        assert_eq!(
            parse_key_spec("Ctrl+Shift+W").as_deref(),
            Some("<Control-Shift-W>")
        );
        assert_eq!(parse_key_spec("Space").as_deref(), Some("<space>"));
        assert_eq!(
            parse_key_spec("Ctrl+Space").as_deref(),
            Some("<Control-space>")
        );
        assert_eq!(parse_key_spec("pageup").as_deref(), Some("<Prior>"));
        assert_eq!(parse_key_spec("F5").as_deref(), Some("<F5>"));
        assert_eq!(parse_key_spec("escape").as_deref(), Some("<Escape>"));
    }

    #[test]
    fn parse_symbol_keys() {
        assert_eq!(
            parse_key_spec("Ctrl+Plus").as_deref(),
            Some("<Control-plus>")
        );
        assert_eq!(parse_key_spec("Ctrl++").as_deref(), Some("<Control-plus>"));
        assert_eq!(
            parse_key_spec("Ctrl+[").as_deref(),
            Some("<Control-bracketleft>")
        );
        assert_eq!(
            parse_key_spec("Ctrl+BraceLeft").as_deref(),
            Some("<Control-braceleft>")
        );
        assert_eq!(
            parse_key_spec("Ctrl+?").as_deref(),
            Some("<Control-question>")
        );
        assert_eq!(parse_key_spec("Ctrl+|").as_deref(), Some("<Control-bar>"));
        assert_eq!(
            parse_key_spec("Ctrl+_").as_deref(),
            Some("<Control-underscore>")
        );
        assert_eq!(parse_key_spec("Ctrl+0").as_deref(), Some("<Control-0>"));
        assert_eq!(parse_key_spec("Ctrl+<").as_deref(), Some("<Control-less>"));
    }

    #[test]
    fn parse_errors() {
        assert!(parse_key_spec("").is_none());
        assert!(parse_key_spec("Ctrl+").is_none());
        assert!(parse_key_spec("Ctrl").is_none());
        assert!(parse_key_spec("Ctrl+AW").is_none()); // two keys
        assert!(parse_key_spec("NotAKeyNameHere").is_none());
    }

    #[test]
    fn shift_uppercases_letters() {
        assert_eq!(parse_key_spec("Shift+y").as_deref(), Some("<Shift-Y>"));
        assert_eq!(
            parse_key_spec("Ctrl+Shift+y").as_deref(),
            Some("<Control-Shift-Y>")
        );
    }

    #[test]
    fn keyrelease() {
        assert_eq!(
            keyrelease_seq("<Control-Shift-w>"),
            "<Control-Shift-KeyRelease-w>"
        );
        assert_eq!(keyrelease_seq("<q>"), "<KeyRelease-q>");
    }

    #[test]
    fn direct_key() {
        assert_eq!(to_direct_key("<Control-q>").as_deref(), Some("<q>"));
        assert_eq!(to_direct_key("<Control-Shift-W>").as_deref(), Some("<W>"));
        assert_eq!(
            to_direct_key("<Control-Shift-braceleft>").as_deref(),
            Some("<Shift-braceleft>")
        );
        assert_eq!(to_direct_key("<Control-space>").as_deref(), Some("<space>"));
        assert_eq!(to_direct_key("bogus").as_deref(), None);
        assert_eq!(to_direct_key("<>").as_deref(), None);
    }
}
