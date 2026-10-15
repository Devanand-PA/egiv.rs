//! Floating ("dialog") window support, porting tkiv.py's
//!
//! ```python
//! if (self.purpose == 'select') or self.enable_floating_window:
//!     self.root.attributes('-topmost', True)
//!     try: self.root.attributes('-type', 'dialog')
//!     except Exception: pass
//!     self.root.geometry(f'{sw - 100}x{sh - 100}+50+50')
//!     self.root.minsize(600, 400)
//! ```
//!
//! Tk's `-type dialog` sets `_NET_WM_WINDOW_TYPE_DIALOG` on X11, which is
//! what makes tiling window managers (i3, sway, hyprland, …) leave the
//! window floating.  eframe/winit offers no builder hook for it, so the
//! property is written directly through Xlib while the window is still
//! unmapped (the viewport is created hidden and shown on the first frame).
//!
//! On Wayland there is no window-type protocol, matching Tk where the
//! attribute is silently ignored — this then becomes a no-op.

use eframe::CreationContext;

#[cfg(unix)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Returns the screen dimensions (w, h) when the X11 dialog property was
/// set successfully; `None` on Wayland / when libX11 is unavailable / when
/// the window must not float anyway.
#[cfg(unix)]
pub fn setup_dialog(cc: &CreationContext<'_>) -> Option<(f32, f32)> {
    let handle = cc.window_handle().ok()?.as_raw();
    let window = match handle {
        RawWindowHandle::Xcb(h) => h.window.get() as u64,
        RawWindowHandle::Xlib(h) => h.window as u64,
        _ => return None,
    };
    unsafe { set_dialog_type(window) }
}

/// No X11 window-type hints outside unix — `_NET_WM_WINDOW_TYPE` doesn't
/// exist on Windows/macOS, matching Tk where the attribute is ignored.
#[cfg(not(unix))]
pub fn setup_dialog(_cc: &CreationContext<'_>) -> Option<(f32, f32)> {
    None
}

/// Writes `_NET_WM_WINDOW_TYPE = _NET_WM_WINDOW_TYPE_DIALOG` on `window`
/// and returns the default screen size.  A separate (short-lived) Xlib
/// display connection is used; property changes are server-side state so
/// they are visible to the window manager immediately.
#[cfg(unix)]
unsafe fn set_dialog_type(window: u64) -> Option<(f32, f32)> {
    let xlib = x11_dl::xlib::Xlib::open().ok()?;
    let dpy = (xlib.XOpenDisplay)(std::ptr::null());
    if dpy.is_null() {
        return None;
    }
    let wm_type = (xlib.XInternAtom)(
        dpy,
        b"_NET_WM_WINDOW_TYPE\0".as_ptr().cast(),
        x11_dl::xlib::False,
    );
    let dialog = (xlib.XInternAtom)(
        dpy,
        b"_NET_WM_WINDOW_TYPE_DIALOG\0".as_ptr().cast(),
        x11_dl::xlib::False,
    );
    let atom = (xlib.XInternAtom)(dpy, b"ATOM\0".as_ptr().cast(), x11_dl::xlib::False);
    if wm_type != 0 && dialog != 0 && atom != 0 {
        // XChangeProperty with format=32 expects `long` elements, so the
        // data must be an array of u64 on LP64 platforms.
        let data: [u64; 1] = [dialog];
        (xlib.XChangeProperty)(
            dpy,
            window,
            wm_type,
            atom,
            32,
            x11_dl::xlib::PropModeReplace,
            data.as_ptr().cast(),
            1,
        );
    }
    let screen = (xlib.XDefaultScreenOfDisplay)(dpy);
    let size = if screen.is_null() {
        None
    } else {
        Some((
            (xlib.XWidthOfScreen)(screen) as f32,
            (xlib.XHeightOfScreen)(screen) as f32,
        ))
    };
    (xlib.XCloseDisplay)(dpy);
    size
}

/// Tk-parity floating geometry: `screen-100 x screen-100 +50+50`.
pub fn floating_geometry(screen: (f32, f32)) -> (f32, f32, f32, f32) {
    (
        (screen.0 - 100.0).max(200.0),
        (screen.1 - 100.0).max(200.0),
        50.0,
        50.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_geometry_matches_tkiv() {
        let g = floating_geometry((1920.0, 1080.0));
        assert_eq!(g, (1820.0, 980.0, 50.0, 50.0));
        // Tiny screens stay usable.
        let g = floating_geometry((150.0, 150.0));
        assert_eq!(g, (200.0, 200.0, 50.0, 50.0));
    }
}
