//! UI/event/render implementation for `App`.  Kept in a child module so it
//! can use `App`'s private state without widening the public API.

use super::*;
use egui::{ColorImage, Margin, RichText, TopBottomPanel, Ui};

/// Height of one row in list mode (monospace 13 + padding).
const LIST_ROW_H: f32 = 18.0;

fn texture_options(smooth: bool) -> TextureOptions {
    if smooth {
        TextureOptions::LINEAR
    } else {
        TextureOptions::NEAREST
    }
}

fn upload(
    ctx: &egui::Context,
    name: impl Into<String>,
    img: &image::RgbaImage,
    smooth: bool,
) -> TextureHandle {
    let size = [img.width() as usize, img.height() as usize];
    let ci = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture(name, ci, texture_options(smooth))
}

fn key_matches(b: &Binding, key: Key, mods: egui::Modifiers) -> bool {
    key_matches_shift(b, key, mods, false)
}

/// Shift handling: Tk records shifted punctuation as the resulting keysym
/// (braceleft, parenleft, less, ...), while egui reports the physical key
/// plus Modifiers::SHIFT.  Do not require an exact shift match for those.
/// With `strict_shift` the binding's shift flag must equal the event's -
/// pass one of find_binding, which keeps chords like Ctrl+F (searchbar)
/// distinct from Ctrl+Shift+F (fill) even though the lenient rule would let
/// the uppercase-letter binding match both.
fn key_matches_shift(
    b: &Binding,
    key: Key,
    mods: egui::Modifiers,
    strict_shift: bool,
) -> bool {
    const SHIFTED_SYMBOLS: [&str; 11] = [
        "plus",
        "braceleft",
        "braceright",
        "parenleft",
        "parenright",
        "less",
        "greater",
        "question",
        "bar",
        "underscore",
        "equal",
    ];
    let shift_lenient = SHIFTED_SYMBOLS.contains(&b.key.as_str())
        || (b.key.chars().count() == 1
            && b.key.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
    let shift_ok = if !strict_shift && shift_lenient {
        true
    } else {
        b.shift == mods.shift
    };
    key_from_event_text(&b.key) == Some(key)
        && b.ctrl == mods.ctrl
        && shift_ok
        && b.alt == mods.alt
        && b.meta == mods.mac_cmd
}

/// Find the binding matching this key press: exact shift parity across the
/// whole map first, lenient shift rules only as a fallback.  Matching order
/// then no longer decides between chords that share a physical key, e.g.
/// fill (Ctrl+Shift+F, "F", lenient) vs toggle_searchbar (Ctrl+F, "f",
/// exact): with single-pass first-match-wins the earlier `fill` entry also
/// swallowed plain Ctrl+F and the searchbar could never open.
fn find_binding(
    map: &[(Binding, Action, bool)],
    key: Key,
    mods: egui::Modifiers,
) -> Option<Action> {
    for strict in [true, false] {
        for (b, action, _) in map {
            if key_matches_shift(b, key, mods, strict) {
                return Some(*action);
            }
        }
    }
    None
}

/// The zoom action a Ctrl+plus/equals/minus/0 chord maps to, if any.
///
/// Ctrl+Shift+0 is excluded even though it reports the same physical key
/// (Key::Num0): on US layouts Shift+0 types ')' and Ctrl+) is bound to
/// contrast_up, so the shifted chord must fall through to the keymap.  The
/// other shifted variants stay zoom chords - Ctrl+Shift+= and Ctrl+Shift+-
/// have no conflicting binding, and egui reports them through the same
/// logical keys (Plus/Minus).
fn zoom_chord_action(key: Key, mods: egui::Modifiers) -> Option<Action> {
    if !(mods.ctrl || mods.mac_cmd) {
        return None;
    }
    match key {
        Key::Plus | Key::Equals => Some(Action::ZoomIn),
        Key::Minus => Some(Action::ZoomOut),
        Key::Num0 if !mods.shift => Some(Action::Zoom100),
        _ => None,
    }
}

/// Port of tkiv.py `_prepare_crop` colour adjustments.  Each enhancement is
/// applied ONLY when its step value is non-zero, exactly like the Python
/// code — the neutral value of one adjustment must not corrupt the others:
///   brightness: v *= max(0.01, steps_to_range(b, BRIGHTNESS_MAX, 1.0))
///               (PIL ImageEnhance.Brightness factor; offset 1.0 so the
///                neutral step is "no change" and the range is 0..2x.
///                tkiv.py passes offset 0.0 here, which makes +1 go black
///                and only recover at +32 — fixed per user request to
///                behave like the other adjustments.)
///               (PIL ImageEnhance.Brightness factor)
///   contrast:   v = mean + (v - mean) * max(0.01, steps_to_range(c, CONTRAST_MAX, 1.0))
///               (PIL ImageEnhance.Contrast blends with a flat image whose
///                grey level is the rounded mean luminance, ITU-R 601-2)
///   gamma:      v = 255 * (v / 255) ** (1 / max(0.01, steps_to_range(g, GAMMA_MAX, 1.0)))
fn adjusted_image(
    src: &image::RgbaImage,
    brightness: i32,
    contrast: i32,
    gamma: i32,
) -> image::RgbaImage {
    if brightness == 0 && contrast == 0 && gamma == 0 {
        return src.clone();
    }
    // PIL's ContrastEnhance pivots on the mean luminance of the image
    // (converted to "L" with 0.299 R + 0.587 G + 0.114 B), rounded like
    // int(mean + 0.5); the same grey value is used for all three channels.
    let gray_mean = if contrast != 0 {
        let n = src.width() as f64 * src.height() as f64;
        if n > 0.0 {
            let mut acc = 0.0f64;
            for p in src.pixels() {
                acc += 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
            }
            (acc / n + 0.5).floor()
        } else {
            128.0
        }
    } else {
        0.0
    };
    let b = if brightness != 0 {
        // Offset 1.0: neutral = no change, +32 steps = 2x brighter,
        // -32 steps = black — the intuitive behaviour matching contrast
        // and gamma.  (tkiv.py's offset 0.0 made +1 go nearly black.)
        Some(model::steps_to_range(brightness, model::BRIGHTNESS_MAX, 1.0).max(0.01))
    } else {
        None
    };
    let c = if contrast != 0 {
        Some(model::steps_to_range(contrast, model::CONTRAST_MAX, 1.0).max(0.01))
    } else {
        None
    };
    let inv = if gamma != 0 {
        Some(1.0 / model::steps_to_range(gamma, model::GAMMA_MAX, 1.0).max(0.01))
    } else {
        None
    };
    let mut out = src.clone();
    for p in out.pixels_mut() {
        let a = p[3];
        for ch in 0..3 {
            let mut v = p[ch] as f64;
            if let Some(b) = b {
                v *= b;
            }
            if let Some(c) = c {
                v = gray_mean + (v - gray_mean) * c;
            }
            if let Some(inv) = inv {
                v = 255.0 * ((v / 255.0).clamp(0.0, 1.0)).powf(inv);
            }
            p[ch] = (v + 0.5).clamp(0.0, 255.0) as u8;
        }
        p[3] = a;
    }
    out
}

impl App {
    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Loaded {
                    token,
                    idx,
                    gen,
                    res,
                } => {
                    self.in_flight.remove(&idx);
                    if token != self.load_token
                        || gen != self.files_gen
                        || idx >= self.files.len()
                        || self.quitting
                    {
                        self.pending_load = None;
                        continue;
                    }
                    self.pending_load = None;
                    match res {
                        Ok((frames, delays)) if !frames.is_empty() => {
                            self.install_image(idx, frames, delays);
                            self.persist_index();
                            self.prefetch_neighbors(idx);
                            ctx.request_repaint();
                        }
                        res => {
                            if let Err(e) = res {
                                if !self.opts.quiet {
                                    eprintln!("{PROGNAME}: {}: {e}", self.files[idx].path);
                                }
                            }
                            if self.opts.assume_files {
                                self.frames.clear();
                                self.img_w = 0;
                                self.img_h = 0;
                                ctx.request_repaint();
                            } else if self.remove_file(idx, false) {
                                if self.fileidx >= self.files.len() {
                                    self.fileidx = self.files.len() - 1;
                                }
                                self.load_image_async(self.fileidx);
                            }
                        }
                    }
                }
                Msg::Prefetch { idx, gen, res } => {
                    self.in_flight.remove(&idx);
                    if gen != self.files_gen || self.quitting {
                        continue;
                    }
                    let wanted = self.pending_load == Some((idx, self.load_token));
                    match res {
                        Ok((frames, delays)) if wanted && self.mode == Mode::Image => {
                            self.pending_load = None;
                            self.install_image(idx, frames, delays);
                            self.persist_index();
                            self.prefetch_neighbors(idx);
                            ctx.request_repaint();
                        }
                        Ok(frames) => {
                            if let std::collections::hash_map::Entry::Vacant(v) =
                                self.prefetch.entry(idx)
                            {
                                v.insert(frames);
                                self.prefetch_keys.push(idx);
                                while self.prefetch.len() > PREFETCH_MAX {
                                    if let Some(k) = self.prefetch_keys.first().copied() {
                                        self.prefetch_keys.remove(0);
                                        self.prefetch.remove(&k);
                                    }
                                }
                            }
                        }
                        Err(e) if wanted => {
                            self.pending_load = None;
                            if !self.opts.quiet {
                                eprintln!("{PROGNAME}: {}: {e}", self.files[idx].path);
                            }
                            if !self.opts.assume_files && self.remove_file(idx, false) {
                                if self.fileidx >= self.files.len() {
                                    self.fileidx = self.files.len() - 1;
                                }
                                self.load_image_async(self.fileidx);
                            }
                        }
                        Err(_) => {}
                    }
                }
                Msg::Thumb { idx, gen, res } => {
                    self.thumb_in_flight.remove(&idx);
                    if gen != self.files_gen || idx >= self.files.len() || self.quitting {
                        continue;
                    }
                    if let Some((img, size)) = res {
                        let tex = upload(ctx, format!("thumb:{idx}"), &img, true);
                        self.thumbs.insert(idx, tex);
                        if size.0 > 0 {
                            self.origins.insert(self.files[idx].path.clone(), size);
                        }
                        ctx.request_repaint();
                    }
                }
                Msg::LazyBatch(paths) => self.commit_lazy(paths, ctx),
                Msg::LazyDone => {
                    self.lazy_scan_active = false;
                    if self.sort_mode != SortMode::None {
                        self.apply_sort(None, None, true);
                    }
                    if self.mode == Mode::List {
                        self.list_pending = -2;
                    }
                    ctx.request_repaint();
                }
                Msg::Aspect { gen, mut ratios } => {
                    if gen != self.aspect_gen {
                        continue;
                    }
                    self.aspect_pending = false;
                    if ratios.is_empty() {
                        continue;
                    }
                    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let a = (ratios[ratios.len() / 2] as f32)
                        .clamp(GALLERY_ASPECT_MIN, GALLERY_ASPECT_MAX);
                    if Some(a) != self.gallery_aspect_cache {
                        self.gallery_aspect_cache = Some(a);
                        self.thumbs.clear();
                        self.thumb_in_flight.clear();
                        self.gallery_scroll_to = Some(self.fileidx);
                    }
                    ctx.request_repaint();
                }
                Msg::ListPreview { idx, res } => {
                    if idx as i64 != self.list_pending || self.mode != Mode::List {
                        continue;
                    }
                    match res {
                        Ok((frames, _)) if !frames.is_empty() => {
                            self.list_preview = Some(upload(ctx, "list-preview", &frames[0], true));
                        }
                        _ => self.list_preview = None,
                    }
                    ctx.request_repaint();
                }
            }
        }
    }

    fn handle_timers(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(t) = self.next_frame_at {
            if now >= t && self.frames.len() > 1 && self.img_animate {
                self.img_sel = (self.img_sel + 1) % self.frames.len();
                if let Some(f) = self.frames.get(self.img_sel) {
                    self.img_w = f.width();
                    self.img_h = f.height();
                }
                self.img_tex = None;
                self.schedule_animate();
                ctx.request_repaint();
            }
        }
        if let Some(t) = self.next_frame_at {
            ctx.request_repaint_after(t.saturating_duration_since(now));
        }
        if let Some(t) = self.next_slide_at {
            if now >= t && self.ss_on && !self.visible.is_empty() {
                let pos = self
                    .visible
                    .iter()
                    .position(|&i| i == self.fileidx)
                    .unwrap_or(0);
                self.navigate(self.visible[(pos + 1) % self.visible.len()]);
                self.schedule_slideshow();
            } else {
                ctx.request_repaint_after(t.saturating_duration_since(now));
            }
        }
        if self.mode == Mode::Image
            && now.duration_since(self.last_autoreload) >= Duration::from_millis(500)
        {
            self.last_autoreload = now;
            if !self.frames.is_empty() && self.fileidx < self.files.len() {
                if let Ok(meta) = std::fs::metadata(&self.files[self.fileidx].path) {
                    if let Ok(m) = meta.modified() {
                        if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                            let cur = d.as_nanos();
                            if self
                                .mtimes
                                .get(&self.fileidx)
                                .is_some_and(|old| *old != cur)
                            {
                                self.mtimes.insert(self.fileidx, cur);
                                self.prefetch.remove(&self.fileidx);
                                self.load_image_async(self.fileidx);
                            }
                        }
                    }
                }
            }
        }
    }

    fn apply_filter_change(&mut self, ctx: &egui::Context) {
        self.rebuild_visible();
        if !self.visible.is_empty() && !self.visible.contains(&self.fileidx) {
            self.fileidx = self.visible[0];
            self.persist_index();
            match self.mode {
                Mode::Image => {
                    self.load_image_async(self.fileidx);
                }
                Mode::Gallery => self.gallery_scroll_to = Some(self.fileidx),
                Mode::List => self.list_pending = -2,
            }
        }
        ctx.request_repaint();
    }

    fn handle_keys(&mut self, ctx: &egui::Context, win: Vec2) {
        let events = ctx.input(|i| i.events.clone());
        let repeat_ms = self
            .cfg_behavior
            .get("key_repeat_ms")
            .copied()
            .unwrap_or(60)
            .max(1) as u64;
        let repeat_every = Duration::from_millis(repeat_ms);
        // Fixed navigation keys repeat twice as fast as the configurable
        // actions so holding Up/Down scrolls quickly.
        let nav_repeat = Duration::from_millis((repeat_ms / 2).max(1));
        // Repeats only start after the key has been held for this long, so
        // a single press advances exactly one step (a 30ms repeat otherwise
        // fired several times within a normal tap).
        let hold_delay = Duration::from_millis(
            self.cfg_behavior
                .get("key_hold_delay_ms")
                .copied()
                .unwrap_or(crate::config::KEY_HOLD_DELAY_MS)
                .max(1) as u64,
        );
        // Held Ctrl+plus/minus zoom continuously at a gentler pace than the
        // navigation keys, so the ramp through the zoom levels is visible.
        let zoom_every = Duration::from_millis((repeat_ms * 4).max(120));
        for ev in events {
            let egui::Event::Key {
                key,
                pressed,
                modifiers,
                repeat,
                ..
            } = ev
            else {
                continue;
            };
            let key_name = format!(
                "{key:?}:{}{}{}{}{}",
                modifiers.ctrl as u8,
                modifiers.shift as u8,
                modifiers.alt as u8,
                modifiers.mac_cmd as u8,
                self.no_searchbar as u8
            );
            if !pressed {
                self.held.remove(&key_name);
                continue;
            }
            // Ctrl+plus/equals/minus/0 zoom the IMAGES, never the UI, in
            // every mode: image mode steps the picture through the same
            // levels as the mouse wheel, gallery mode resizes the thumbnails
            // (act_zoom's gallery branch), and list mode ignores it.  egui
            // would scale the whole UI at the end of the pass by consuming
            // these very shortcuts, so they are consumed here first - on the
            // initial press AND on every synthetic auto-repeat - which
            // shadows that completely.
            //
            // Exception: Ctrl+Shift+0 types ')' on US layouts (contrast_up
            // by default), so it is NOT zoom-to-default.  egui's shortcut
            // matching ignores the extra shift (Modifiers::matches_logically),
            // so its built-in Ctrl+0 UI-zoom would still fire on that press
            // - hence the shortcut is consumed either way, but only the
            // unshifted press acts as a zoom; the shifted one falls through
            // to the keymap below.
            let zoom_action = zoom_chord_action(key, modifiers);
            if zoom_action.is_some()
                || ((modifiers.ctrl || modifiers.mac_cmd)
                    && matches!(key, Key::Plus | Key::Equals | Key::Minus | Key::Num0))
            {
                // Consume the egui UI-zoom shortcut matching this key press
                // (modifiers match logically: shift/alt extras are ignored,
                // exactly like egui's own end-pass handling).
                ctx.input_mut(|i| {
                    i.consume_shortcut(&egui::KeyboardShortcut::new(
                        egui::Modifiers::COMMAND,
                        key,
                    ));
                });
                if let Some(action) = zoom_action {
                    // One step per press, like one wheel notch; holding the
                    // chord then keeps zooming through the same pending path
                    // (first repeat after the hold delay, then every
                    // zoom_every).  Ctrl+0 does not repeat.
                    if !repeat {
                        if self.mode == Mode::Image {
                            // Applied inside render_image through the very same
                            // act_zoom call the mouse wheel uses, with the same
                            // viewport metrics and the same point in the frame.
                            let step = match action {
                                Action::ZoomIn => Some(1),
                                Action::ZoomOut => Some(-1),
                                _ => None,
                            };
                            self.pending_key_zoom = step.or(Some(0));
                            if let Some(d) = step {
                                self.held.insert(
                                    key_name.clone(),
                                    HeldKey {
                                        key,
                                        what: HeldWhat::Zoom(d),
                                        interval: zoom_every,
                                        next_fire: Instant::now() + hold_delay,
                                    },
                                );
                            }
                        } else {
                            self.run_action(action, ctx, win);
                        }
                    }
                    continue;
                }
                // Ctrl+Shift+0 = ')': fall through to the keymap so the
                // shifted chord reaches contrast_up (or its rebound action).
            }
            // Synthetic auto-repeat events are ignored: repetition is
            // driven by our own hold timers (tkiv.py's after() chain),
            // which start right after the first press instead of waiting
            // for the OS repeat delay.
            if repeat {
                continue;
            }
            let mods = modifiers;
            let map = if self.no_searchbar {
                &self.keymap_direct
            } else {
                &self.keymap
            };
            if let Some(a) = find_binding(map, key, mods) {
                self.run_action(a, ctx, win);
                if REPEATABLE.contains(&a) {
                    self.held.insert(
                        key_name.clone(),
                        HeldKey {
                            key,
                            what: HeldWhat::Act(a),
                            interval: repeat_every,
                            next_fire: Instant::now() + hold_delay,
                        },
                    );
                }
                continue;
            }
            // Fixed navigation / mode keys.  Navigation keys register a
            // hold timer; mode keys are one-shot (auto-repeat swallowed).
            match key {
                Key::ArrowLeft => self.hold_nav(key, key_name, Dir::Left, nav_repeat, hold_delay),
                Key::ArrowRight => self.hold_nav(key, key_name, Dir::Right, nav_repeat, hold_delay),
                Key::ArrowUp => self.hold_nav(key, key_name, Dir::Up, nav_repeat, hold_delay),
                Key::ArrowDown => self.hold_nav(key, key_name, Dir::Down, nav_repeat, hold_delay),
                Key::PageUp => self.hold_page(key, key_name, -1, nav_repeat, win, hold_delay),
                Key::PageDown => self.hold_page(key, key_name, 1, nav_repeat, win, hold_delay),
                Key::Home => self.run_action(Action::First, ctx, win),
                Key::End => self.run_action(Action::Last, ctx, win),
                Key::Tab if mods.shift => self.run_action(Action::ModeCycleRev, ctx, win),
                Key::Tab => self.run_action(Action::ModeCycleFwd, ctx, win),
                Key::Enter if !self.no_searchbar && mods.ctrl => {
                    self.run_action(Action::ToggleMark, ctx, win)
                }
                Key::Enter => self.run_action(Action::Return, ctx, win),
                Key::Escape => self.run_action(Action::Escape, ctx, win),
                Key::Delete => self.run_action(Action::DeleteKey, ctx, win),
                _ => {}
            }
        }
        self.fire_held(ctx, win);
    }

    fn render_image(&mut self, ui: &mut Ui) {
        if let Some((sx, sy)) = self.pending_pan.take() {
            let rect = ui.available_size();
            self.pan += Vec2::new(sx * rect.x / 5.0, sy * rect.y / 5.0);
        }
        if self.frames.is_empty() || self.img_sel >= self.frames.len() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(if self.opts.assume_files {
                        self.files
                            .get(self.fileidx)
                            .map(|f| f.label.as_str())
                            .unwrap_or("No image")
                    } else {
                        "No image"
                    })
                    .color(self.theme.fg_text),
                );
            });
            return;
        }
        let avail = ui.available_size();
        if self.scale_mode != ScaleMode::Zoom && self.img_w > 0 && self.img_h > 0 {
            let zw = avail.x as f64 / self.img_w as f64;
            let zh = avail.y as f64 / self.img_h as f64;
            let mut z = match self.scale_mode {
                ScaleMode::Fill => zw.max(zh),
                ScaleMode::Width => zw,
                ScaleMode::Height => zh,
                _ => zw.min(zh),
            };
            if self.scale_mode == ScaleMode::Down {
                z = z.min(1.0);
            }
            self.zoom = z.min(model::ZOOM_MAX);
        }
        let dw = (self.img_w as f32 * self.zoom as f32).max(1.0);
        let dh = (self.img_h as f32 * self.zoom as f32).max(1.0);
        if dw < avail.x {
            self.pan.x = (avail.x - dw) / 2.0;
        } else {
            self.pan.x = self.pan.x.min(0.0).max(avail.x - dw);
        }
        if dh < avail.y {
            self.pan.y = (avail.y - dh) / 2.0;
        } else {
            self.pan.y = self.pan.y.min(0.0).max(avail.y - dh);
        }

        let key = format!(
            "{}:{}:{}:{}:{}:{}",
            self.files
                .get(self.fileidx)
                .map(|f| f.path.as_str())
                .unwrap_or(""),
            self.img_sel,
            self.gamma,
            self.brightness,
            self.contrast,
            self.frames[self.img_sel].as_raw().as_ptr() as usize
        );
        if self.img_tex_key.as_deref() != Some(key.as_str()) {
            let img = adjusted_image(
                &self.frames[self.img_sel],
                self.brightness,
                self.contrast,
                self.gamma,
            );
            self.img_tex = Some(upload(ui.ctx(), key.clone(), &img, self.anti_alias));
            self.img_tex_key = Some(key);
        }
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let origin = rect.min + self.pan;
        let dst = egui::Rect::from_min_size(origin, Vec2::new(dw, dh));
        if let Some(tex) = &self.img_tex {
            painter.image(
                tex.id(),
                dst,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            self.pan += resp.drag_delta();
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let third = rect.width() / 3.0;
                let x = p.x - rect.min.x;
                if x < third {
                    self.nav_visible(-1);
                } else if x > rect.width() - third {
                    self.nav_visible(1);
                }
            }
        }
        // Keyboard zoom (Ctrl+plus/minus/0) goes through the very same
        // act_zoom call as the mouse wheel, at the same point in the frame.
        if let Some(d) = self.pending_key_zoom.take() {
            if d == 0 {
                self.scale_mode = ScaleMode::Zoom;
                self.zoom_to(1.0, avail);
            } else {
                self.act_zoom(d, ui.ctx(), avail);
            }
        }
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            self.act_zoom(if wheel > 0.0 { 1 } else { -1 }, ui.ctx(), avail);
        }
        resp.context_menu(|ui| {
            if ui.button("Copy").clicked() {
                ui.close_menu();
                self.copy_image_to_clipboard();
            }
        });
    }

    fn render_gallery(&mut self, ui: &mut Ui) {
        let win = ui.available_size();
        let _ = self.gallery_aspect(0.0);
        if self.visible.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No matches").color(self.theme.fg_text))
            });
            return;
        }
        let (tw, th, cap, pad) = self.tile_metrics(win, false);
        let cols = (((win.x - pad) / (tw + pad)).floor() as usize).max(1);
        self.gallery_cols = cols;
        let row_h = th + cap + pad;
        let rows = self.visible.len().div_ceil(cols);
        // Make the selected row visible, mirroring tkiv.py
        // _apply_gallery_scroll: only scroll when the row lies outside the
        // viewport, and only as far as needed.  egui's scroll_to_rect must
        // run inside the ScrollArea, so instead the target offset is
        // computed here (against the previous frame's offset) and applied
        // through ScrollArea::vertical_scroll_offset.
        let mut scroll_to: Option<f32> = None;
        if let Some(idx) = self.gallery_scroll_to {
            if let Some(pos) = self.visible.iter().position(|&i| i == idx) {
                let row = pos / cols.max(1);
                let row_top = row as f32 * row_h;
                let row_bottom = row_top + row_h;
                let vh = win.y.max(1.0);
                let y_top = self.gallery_last_offset_y;
                let y_bot = y_top + vh;
                let new_top = if row_top < y_top {
                    row_top
                } else if row_bottom > y_bot {
                    row_bottom - vh
                } else {
                    y_top
                };
                if (new_top - y_top).abs() > 0.5 {
                    scroll_to = Some(new_top.max(0.0));
                }
            }
        }
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(off) = scroll_to {
            scroll = scroll.vertical_scroll_offset(off);
        }
        let out = scroll
            .show_rows(ui, row_h, rows, |ui, row_range| {
                for row in row_range {
                    ui.horizontal(|ui| {
                        for col in 0..cols {
                            let pos = row * cols + col;
                            if pos >= self.visible.len() {
                                break;
                            }
                            let idx = self.visible[pos];
                            let selected = idx == self.fileidx;
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(tw, th + cap), Sense::click());
                            let stroke = if selected {
                                egui::Stroke::new(3.0_f32, self.theme.accent)
                            } else {
                                egui::Stroke::new(2.0_f32, self.theme.hover_border)
                            };
                            ui.painter()
                                .rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
                            if let Some(tex) = self.thumbs.get(&idx) {
                                let sz = tex.size_vec2();
                                // Cover the tile: fill it completely in at
                                // least one dimension (cropping the
                                // overflow) instead of letterboxing the
                                // image inside the frame.  Small images are
                                // upscaled to fill as well.
                                let scale = ((tw - 5.0) / sz.x.max(1.0))
                                    .max((th - 5.0) / sz.y.max(1.0));
                                let isz = sz * scale.max(0.01);
                                let img_area = egui::Rect::from_min_size(
                                    rect.min,
                                    Vec2::new(tw, th),
                                );
                                let r = egui::Rect::from_center_size(
                                    img_area.center(),
                                    isz,
                                );
                                // Clip to the image area so the overflowing
                                // dimension cannot bleed over neighbouring
                                // tiles or the caption below.
                                ui.painter_at(img_area).image(
                                    tex.id(),
                                    r,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            } else {
                                self.submit_gallery_tile(
                                    idx,
                                    (tw.max(1.0) - 5.0) as u32,
                                    (th.max(1.0) - 5.0) as u32,
                                );
                            }
                            if self.files[idx].marked() {
                                let mark = egui::Rect::from_min_max(
                                    rect.max - Vec2::new(12.0, cap + 12.0),
                                    rect.max - Vec2::new(2.0, cap + 2.0),
                                );
                                ui.painter().rect_filled(mark, 0.0, self.theme.accent);
                            }
                            let label = model::fit_caption(
                                &self.files[idx].label,
                                (tw - 4.0) as f64,
                                (cap - 2.0) as f64,
                                7.0,
                                14.0,
                            );
                            ui.painter().text(
                                egui::pos2(rect.center().x, rect.max.y - cap / 2.0),
                                egui::Align2::CENTER_CENTER,
                                label,
                                egui::FontId::monospace(11.0),
                                self.theme.fg_text,
                            );
                            if resp.clicked() {
                                self.fileidx = idx;
                                self.persist_index();
                                if self.opts.purpose == Purpose::Select
                                    && resp.ctx.input(|i| i.modifiers.ctrl)
                                {
                                    let on = !self.files[idx].marked();
                                    self.mark(idx, on);
                                }
                            }
                            resp.context_menu(|ui| {
                                if ui.button("Copy").clicked() {
                                    ui.close_menu();
                                    self.fileidx = idx;
                                    self.copy_image_to_clipboard();
                                }
                            });
                            ui.add_space(pad);
                        }
                    });
                }
            });
        // Remember where the viewport ended up so the make-visible
        // computation above can use it on the next frame.
        self.gallery_last_offset_y = out.state.offset.y;
        self.gallery_scroll_to = None;
    }

    fn render_list(&mut self, ui: &mut Ui) {
        if self.visible.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No matches").color(self.theme.fg_text))
            });
            return;
        }
        if !self.visible.contains(&self.fileidx) {
            self.fileidx = self.visible[0];
            self.persist_index();
        }
        if self.list_pending == -2 {
            self.submit_list_preview();
            if self.fileidx < self.files.len() {
                // Selection changed (navigation, sort, filter, removal):
                // bring it into view like tkiv.py's listbox.see(pos).
                self.list_scroll_to = Some(self.fileidx);
            }
        }
        let avail = ui.available_size();
        // Width of the file list: fit the widest visible label instead of
        // a fixed 300px column, so the preview gets most of the window.
        let font = egui::FontId::monospace(13.0);
        let cache_key = (self.files_gen, self.files.len(), self.filter_text.clone());
        if self.list_label_cache.0 != cache_key.0
            || self.list_label_cache.1 != cache_key.1
            || self.list_label_cache.2 != cache_key.2
        {
            let mut w = 0.0f32;
            for &i in &self.visible {
                if i >= self.files.len() {
                    continue;
                }
                let g = ui
                    .painter()
                    .layout_no_wrap(self.files[i].label.clone(), font.clone(), Color32::WHITE);
                w = w.max(g.size().x);
            }
            self.list_label_cache = (cache_key.0, cache_key.1, cache_key.2, w);
        }
        let list_w = (self.list_label_cache.3 + 20.0 + 10.0)
            .clamp(140.0, (avail.x * 0.4).max(140.0));
        ui.horizontal(|ui| {
            ui.allocate_ui(Vec2::new(list_w, avail.y), |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(list_w);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                            // Rows highlight only when actually selected: a
                            // parked mouse pointer over the list otherwise
                            // paints egui's hover fill as a phantom extra
                            // 'selection' on whatever row it happens to sit
                            // on (e.g. the first entry right after switching
                            // here from gallery mode).
                            {
                                let v = ui.visuals_mut();
                                let inactive = v.widgets.inactive.fg_stroke;
                                v.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
                                v.widgets.hovered.bg_fill = Color32::TRANSPARENT;
                                v.widgets.hovered.fg_stroke = inactive;
                            }
                            for pos in 0..self.visible.len() {
                                let idx = self.visible[pos];
                                let selected = idx == self.fileidx;
                                let marked = self.files[idx].marked();
                                let label = self.files[idx].label.clone();
                                // Painted by hand instead of
                                // egui::selectable_label: that widget also
                                // fills its background when hovered,
                                // highlighted or keyboard-focused, which
                                // showed up as a phantom 'ghost' selection
                                // on a row the pointer merely sat over.
                                let (row_rect, resp) = ui
                                    .allocate_exact_size(
                                        Vec2::new(list_w, LIST_ROW_H),
                                        Sense::click(),
                                    );
                                if selected {
                                    ui.painter().rect_filled(
                                        row_rect,
                                        3.0,
                                        self.theme.selected_bg,
                                    );
                                }
                                ui.painter().text(
                                    row_rect.left_center(),
                                    egui::Align2::LEFT_CENTER,
                                    &label,
                                    font.clone(),
                                    if selected || marked {
                                        self.theme.selected_fg
                                    } else {
                                        self.theme.fg_text
                                    },
                                );
                                if self.list_scroll_to == Some(idx) {
                                    // Minimal scroll, matching listbox.see().
                                    ui.scroll_to_rect(row_rect, None);
                                }
                                if resp.clicked() {
                                    self.fileidx = idx;
                                    self.persist_index();
                                    self.list_pending = -2;
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Copy").clicked() {
                                        ui.close_menu();
                                        self.fileidx = idx;
                                        self.copy_image_to_clipboard();
                                    }
                                });
                            }
                        });
                    });
            });
            // Preview pane: fill all remaining space.  tkiv.py used
            // thumbnail() (downscale only); here small images are
            // upscaled as well so the image uses the available room.
            ui.vertical_centered(|ui| {
                if let Some(tex) = &self.list_preview {
                    let avail = ui.available_size();
                    let sz = tex.size_vec2();
                    let scale = (avail.x / sz.x.max(1.0))
                        .min((avail.y - 24.0).max(40.0) / sz.y.max(1.0));
                    ui.add(egui::Image::new((tex.id(), sz * scale.max(0.01))));
                } else {
                    ui.label(
                        RichText::new(format!("Loading {}…", self.files[self.fileidx].label))
                            .color(self.theme.fg_text),
                    );
                }
                ui.label(
                    RichText::new(&self.files[self.fileidx].label)
                        .strong()
                        .color(self.theme.fg_bright),
                );
            });
        });
        self.list_scroll_to = None;
    }

    fn submit_list_preview(&mut self) {
        if self.fileidx >= self.files.len() {
            return;
        }
        self.list_pending = self.fileidx as i64;
        let idx = self.fileidx;
        let path = self.files[idx].path.clone();
        let tx = self.tx.clone();
        let max_dim = self.cfg_behavior["max_load_dim"] as u32;
        std::thread::spawn(move || {
            let res = images::load_frames(&path, Some(max_dim.min(2048)));
            let _ = tx.send(Msg::ListPreview { idx, res });
        });
    }

    fn status_line(&mut self) -> (String, String) {
        if self.files.is_empty() {
            return (String::new(), String::new());
        }
        self.fileidx %= self.files.len();
        let f = &self.files[self.fileidx];
        let all = self.files.len();
        let vis = self.visible.len();
        let fw = all.to_string().len();
        let mark = if f.marked() { "* " } else { "" };
        let vis_pos = self
            .visible
            .iter()
            .position(|&i| i == self.fileidx)
            .map(|p| p + 1)
            .unwrap_or(0);
        let sl = model::sort_label(self.sort_mode, self.sort_reverse);
        let suffix = if sl.is_empty() {
            String::new()
        } else {
            format!("  [{sl}]")
        };
        let first = if self.mode != Mode::Image {
            if !self.filter_text.is_empty() && !self.no_searchbar {
                format!("{mark}{vis_pos:0fw$}/{vis} ({all}){suffix}  {}", f.label)
            } else {
                format!("{mark}{:0fw$}/{all}{suffix}  {}", self.fileidx + 1, f.label)
            }
        } else {
            let mut parts: Vec<String> = vec![];
            if self.ss_on {
                let d = self.ss_delay_ds as f64 / 10.0;
                parts.push(if d.fract() == 0.0 {
                    format!("{}s", d as i64)
                } else {
                    format!("{d:.1}s")
                });
            }
            if self.gamma != 0 {
                parts.push(format!("G{:+}", self.gamma));
            }
            if self.brightness != 0 {
                parts.push(format!("B{:+}", self.brightness));
            }
            if self.contrast != 0 {
                parts.push(format!("C{:+}", self.contrast));
            }
            parts.push(format!("{}%", (self.zoom * 100.0) as i64));
            if self.frames.len() > 1 {
                parts.push(format!("{}/{}", self.img_sel + 1, self.frames.len()));
            }
            if !self.filter_text.is_empty() && !self.no_searchbar {
                parts.push(format!("{vis_pos}/{vis} ({all})"));
            } else {
                parts.push(format!("{:0fw$}/{all}", self.fileidx + 1));
            }
            if !sl.is_empty() {
                parts.push(sl);
            }
            format!("{mark}{}  {}", parts.join("  "), f.label)
        };
        let sz = std::fs::metadata(&f.path)
            .ok()
            .map(|m| m.len())
            .unwrap_or(0) as f64;
        let size = if sz < 1024.0 {
            format!("{} B", (sz * 100.0) as i64 as f64 / 100.0)
        } else if sz < 1024.0 * 1024.0 {
            format!("{} KB", (sz * 100.0 / 1024.0) as i64 as f64 / 100.0)
        } else {
            format!(
                "{} MB",
                (sz * 100.0 / 1024.0 / 1024.0) as i64 as f64 / 100.0
            )
        };
        let (rw, rh) = self.origins.get(&f.path).copied().unwrap_or_else(|| {
            let s = images::get_orig_size(&f.path);
            if s.0 > 0 {
                self.origins.insert(f.path.clone(), s);
            }
            s
        });
        let res = if rw > 0 {
            format!("{rw}x{rh}")
        } else {
            "?x?".to_string()
        };
        (first, format!(" Resolution : {res} | File Size :{size}"))
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.pending_show {
            // Floating windows were created hidden so the dialog window
            // type could be set before mapping; show them now with the
            // tkiv.py geometry (screen-100 x screen-100 +50+50).
            self.pending_show = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            // Grab input focus immediately, like dmenu: the selector is
            // on top and meant to be typed into without a click first.
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if let Some((w, h, x, y)) = self.pending_geometry.take() {
                // The pending geometry is in PHYSICAL pixels (measured from
                // the X screen), while the viewport commands are in egui
                // POINTS — egui-winit multiplies them by pixels_per_point,
                // so on scaled displays the raw values made the window far
                // larger than the screen.  Convert here, and reserve room
                // for the WM decorations (Tk's `geometry` sets the OUTER
                // size, InnerSize is the client area).
                let ppp = ctx.pixels_per_point();
                let deco_w = 16.0 / ppp;
                let deco_h = 48.0 / ppp;
                // (w, h) already carry the tkiv.py geometry (display size
                // minus 100px, centered by the +50/+50 offset); only the WM
                // decorations are subtracted here so the OUTER window lands
                // on the requested size.
                let inner = Vec2::new(
                    (w / ppp - deco_w).max(300.0),
                    (h / ppp - deco_h).max(200.0),
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(inner));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    x / ppp,
                    y / ppp,
                )));
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            let s = if self.opts.purpose == Purpose::Select {
                1
            } else {
                0
            };
            self.quit(s);
        }
        self.drain_messages(ctx);
        self.handle_timers(ctx);
        let win = ctx.available_rect().size();
        self.handle_keys(ctx, win);

        if self.show_bar {
            TopBottomPanel::bottom("status")
                .frame(
                    egui::Frame::NONE
                        .fill(self.theme.bg_primary)
                        .inner_margin(Margin::symmetric(8, 2)),
                )
                .show(ctx, |ui| {
                    let (a, b) = self.status_line();
                    ui.vertical(|ui| {
                        ui.label(RichText::new(b).monospace().color(self.theme.fg_text));
                        ui.label(RichText::new(a).monospace().color(self.theme.fg_text));
                    });
                });
        }
        if !self.no_searchbar {
            TopBottomPanel::bottom("search")
                .frame(
                    egui::Frame::NONE
                        .fill(self.theme.bg_input)
                        .inner_margin(Margin::symmetric(4, 3)),
                )
                .show(ctx, |ui| {
                    let te = egui::TextEdit::singleline(&mut self.filter_text)
                        .font(egui::TextStyle::Heading)
                        .text_color(self.theme.fg_text)
                        .frame(false)
                        .desired_width(f32::INFINITY);
                    let resp = ui.add(te);
                    resp.request_focus();
                    if resp.changed() {
                        self.apply_filter_change(ctx);
                    }
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.bg_primary))
            .show(ctx, |ui| match self.mode {
                Mode::Image => self.render_image(ui),
                Mode::Gallery => self.render_gallery(ui),
                Mode::List => self.render_list(ui),
            });
        if self.lazy_scan_active || !self.thumb_in_flight.is_empty() || !self.in_flight.is_empty() {
            ctx.request_repaint_after(QUEUE_POLL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(key: &str, ctrl: bool, shift: bool) -> Binding {
        Binding {
            ctrl,
            shift,
            alt: false,
            meta: false,
            key: key.to_string(),
        }
    }

    #[test]
    fn shifted_symbol_bindings_match_physical_keys() {
        let mut mods = egui::Modifiers::CTRL;
        mods.shift = true;
        // '{' is Shift+[ physically; egui reports the LOGICAL key
        // ("{" -> OpenCurlyBracket), so the braceleft binding must match
        // that — this is what makes Ctrl+{ (gamma) work.
        assert!(key_matches(
            &binding("braceleft", true, false),
            Key::OpenCurlyBracket,
            mods
        ));
        assert!(key_matches(&binding("plus", true, false), Key::Plus, mods));
        assert!(key_matches(
            &binding("question", true, false),
            Key::Questionmark,
            mods
        ));
        assert!(key_matches(&binding("bar", true, false), Key::Pipe, mods));
        // '(' has no logical egui name, so egui falls back to the physical
        // key (9), which is what the parenleft binding maps to.
        assert!(key_matches(
            &binding("parenleft", true, false),
            Key::Num9,
            mods
        ));
    }

    #[test]
    fn bracketleft_matches_plain_bracket() {
        // Ctrl+[ (no shift) is nav_10_back and must NOT fire gamma: the
        // bracketleft binding requires shift parity, the braceleft binding
        // does not match the plain-bracket event.
        let mods = egui::Modifiers::CTRL;
        assert!(key_matches(
            &binding("bracketleft", true, false),
            Key::OpenBracket,
            mods
        ));
        assert!(!key_matches(
            &binding("braceleft", true, false),
            Key::OpenBracket,
            mods
        ));
    }

    #[test]
    fn direct_uppercase_bindings_match_shifted_letters() {
        let mut mods = egui::Modifiers::default();
        mods.shift = true;
        assert!(key_matches(&binding("W", false, false), Key::W, mods));
        assert!(key_matches(&binding("S", false, false), Key::S, mods));
    }

    #[test]
    fn ctrl_shift_0_is_paren_not_zoom() {
        // ')' is Shift+0 physically; Ctrl+Shift+0 must fall through to the
        // keymap as contrast_up, so it must NOT map to Zoom100 (regression:
        // the zoom-chord handler used to hijack every Ctrl+Num0, shifted or
        // not, so Ctrl+) could never fire).
        let mut shifted = egui::Modifiers::CTRL;
        shifted.shift = true;
        assert_eq!(zoom_chord_action(Key::Num0, shifted), None);
        // Plain Ctrl+0 still zooms to default.
        assert_eq!(
            zoom_chord_action(Key::Num0, egui::Modifiers::CTRL),
            Some(Action::Zoom100)
        );
        // The other zoom chords are unaffected by shift.
        assert_eq!(zoom_chord_action(Key::Plus, shifted), Some(Action::ZoomIn));
        assert_eq!(
            zoom_chord_action(Key::Equals, shifted),
            Some(Action::ZoomIn)
        );
        assert_eq!(
            zoom_chord_action(Key::Minus, shifted),
            Some(Action::ZoomOut)
        );
        // Without Ctrl/Cmd there is no zoom chord at all.
        let mut no_ctrl = egui::Modifiers::SHIFT;
        no_ctrl.alt = true;
        assert_eq!(zoom_chord_action(Key::Num0, no_ctrl), None);
    }

    #[test]
    fn ctrl_f_opens_searchbar_not_fill() {
        // fill (Ctrl+Shift+F) sits BEFORE toggle_searchbar (Ctrl+F) in the
        // keymap, and its uppercase-letter binding matches leniently, so
        // first-match-wins used to swallow plain Ctrl+F (image just "fills",
        // invisibly) and the searchbar could never open.  find_binding must
        // prefer the exact-parity searchbar binding regardless of order.
        let map = vec![
            (binding("F", true, true), Action::Fill, true),
            (binding("f", true, false), Action::ToggleSearchbar, true),
        ];
        let ctrl = egui::Modifiers::CTRL;
        assert_eq!(
            find_binding(&map, Key::F, ctrl),
            Some(Action::ToggleSearchbar)
        );
        let mut ctrl_shift = egui::Modifiers::CTRL;
        ctrl_shift.shift = true;
        assert_eq!(find_binding(&map, Key::F, ctrl_shift), Some(Action::Fill));
    }

    #[test]
    fn lenient_matching_still_fallback_for_shifted_symbols() {
        // The strict pass must not break shifted-symbol chords: the
        // braceleft binding carries no shift flag, while Ctrl+{ arrives as
        // Key::OpenCurlyBracket with shift set (logical curly key).
        let map = vec![(binding("braceleft", true, false), Action::GammaDown, true)];
        let mut ctrl_shift = egui::Modifiers::CTRL;
        ctrl_shift.shift = true;
        assert_eq!(
            find_binding(&map, Key::OpenCurlyBracket, ctrl_shift),
            Some(Action::GammaDown)
        );
        // ...but a strictly-matching binding wins over a lenient one even
        // when it appears later: Ctrl+[ (no shift) stays nav_10_back even
        // though a braceleft binding leniently tolerates the event.
        let map = vec![
            (binding("braceleft", true, false), Action::GammaDown, true),
            (binding("bracketleft", true, false), Action::Nav10Back, true),
        ];
        assert_eq!(
            find_binding(&map, Key::OpenBracket, egui::Modifiers::CTRL),
            Some(Action::Nav10Back)
        );
    }

    // ------------------------------------------------ adjusted_image tests

    fn gray_img(v: u8) -> image::RgbaImage {
        image::RgbaImage::from_pixel(4, 4, image::Rgba([v, v, v, 255]))
    }

    #[test]
    fn adjustments_noop_at_zero() {
        let src = gray_img(100);
        let out = adjusted_image(&src, 0, 0, 0);
        assert_eq!(out.as_raw(), src.as_raw());
    }

    #[test]
    fn gamma_alone_does_not_blacken() {
        // Regression: brightness==0 used to multiply by 0.01.  With only
        // gamma set the pixel must follow the gamma LUT (128 -> ~149 at
        // gamma step +1), matching tkiv.py's lut computation.
        let src = gray_img(128);
        let out = adjusted_image(&src, 0, 0, 1);
        let g = model::steps_to_range(1, model::GAMMA_MAX, 1.0);
        let expected = (255.0f64 * (128.0f64 / 255.0).powf(1.0 / g) + 0.5) as u8;
        assert_eq!(out.get_pixel(0, 0)[0], expected);
        assert!(out.get_pixel(0, 0)[0] > 100, "must not go near-black");
    }

    #[test]
    fn contrast_alone_keeps_uniform_image() {
        // PIL blends around the image mean, so a uniform image is a fixed
        // point of the contrast enhancement at any factor.
        let src = gray_img(100);
        let out = adjusted_image(&src, 0, 1, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 100);
        let out = adjusted_image(&src, 0, -32, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 100);
    }

    #[test]
    fn contrast_spreads_around_gray_mean() {
        // 25% white pixels + 75% black: mean luminance = 64 (rounded).
        let mut src = image::RgbaImage::new(4, 4);
        for (x, _y, p) in src.enumerate_pixels_mut() {
            let v = if x == 0 { 255u8 } else { 0u8 };
            *p = image::Rgba([v, v, v, 255]);
        }
        let out = adjusted_image(&src, 0, 32, 0);
        let c = model::steps_to_range(32, model::CONTRAST_MAX, 1.0); // 4.0
        let white = (64.0 + (255.0 - 64.0) * c + 0.5) as u8;
        let black = (64.0 + (0.0 - 64.0) * c + 0.5).clamp(0.0, 255.0) as u8;
        assert_eq!(out.get_pixel(0, 0)[0], white);
        assert_eq!(out.get_pixel(1, 0)[0], black);
    }

    #[test]
    fn brightness_scales_monotonically() {
        // Intuitive brightness (offset 1.0): neutral = no change,
        // +32 steps = 2x, -32 steps = black.  (tkiv.py's offset 0.0 made
        // +1 black and only recovered at +32 — deliberately fixed.)
        let src = gray_img(100);
        // +1 step: 100 * (1 + 1/32) = 103.125
        let out = adjusted_image(&src, 1, 0, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 103);
        // +16 steps: 100 * 1.5 = 150
        let out = adjusted_image(&src, 16, 0, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 150);
        // +32 steps: 100 * 2.0 = 200
        let out = adjusted_image(&src, 32, 0, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 200);
        // -16 steps: 100 * 0.5 = 50
        let out = adjusted_image(&src, -16, 0, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 50);
        // -32 steps: black
        let out = adjusted_image(&src, -32, 0, 0);
        assert_eq!(out.get_pixel(0, 0)[0], 1);
    }

    #[test]
    fn alpha_is_preserved() {
        let src = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 200, 200, 77]));
        let out = adjusted_image(&src, 0, 0, 1);
        assert_eq!(out.get_pixel(0, 0)[3], 77);
    }
}
