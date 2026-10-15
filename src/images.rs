//! Image decoding: full frames (with animation delays), gallery
//! thumbnails and the selector's webp disk cache.

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, RgbaImage};
use sha1::{Digest, Sha1};
use std::path::PathBuf;

#[cfg(feature = "vips")]
use std::sync::OnceLock;

use crate::model::DEF_ANIM_DELAY_MS;

pub type Frames = (Vec<RgbaImage>, Vec<u32>); // frames + per-frame delay in ms

// libvips gives us the same fast loading path and broad codec coverage as
// tkiv.py's pyvips branch (including PCX/XPM/JPEG2000/HEIC/HEIF when the
// host libvips supports them).  It is optional: behind the `vips` cargo
// feature at build time, and if libvips cannot initialize at runtime we
// keep the pure-Rust image crate fallback.
#[cfg(feature = "vips")]
static VIPS_READY: OnceLock<bool> = OnceLock::new();

#[cfg(feature = "vips")]
fn vips_enabled() -> bool {
    *VIPS_READY.get_or_init(|| {
        match libvips::VipsApp::new("tkiv", false) {
            Ok(app) => {
                // VipsApp::drop calls vips_shutdown; the library must live for
                // all worker threads, so intentionally leak this tiny handle.
                std::mem::forget(app);
                true
            }
            Err(_) => false,
        }
    })
}

#[cfg(feature = "vips")]
fn vips_to_rgba(mut v: libvips::VipsImage) -> Result<RgbaImage, String> {
    if v.get_format().ok().map(|f| f as i32) != Some(0) {
        v = libvips::ops::cast(&v, libvips::ops::BandFormat::Uchar).map_err(|e| e.to_string())?;
    }
    let (w, h, bands) = (v.get_width(), v.get_height(), v.get_bands());
    if w <= 0 || h <= 0 || bands <= 0 {
        return Err("invalid vips image".into());
    }
    let raw = libvips::VipsImage::image_copy_memory(v)
        .map_err(|e| e.to_string())?
        .image_write_to_memory();
    let expected = w as usize * h as usize * bands as usize;
    if raw.len() < expected {
        return Err("short vips image buffer".into());
    }
    let mut out = image::ImageBuffer::from_pixel(w as u32, h as u32, image::Rgba([0, 0, 0, 255]));
    let b = bands as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * b;
            let px = match b {
                1 => image::Rgba([raw[i], raw[i], raw[i], 255]),
                2 => image::Rgba([raw[i], raw[i], raw[i], raw[i + 1]]),
                3 => image::Rgba([raw[i], raw[i + 1], raw[i + 2], 255]),
                _ => image::Rgba([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]),
            };
            out.put_pixel(x as u32, y as u32, px);
        }
    }
    Ok(out)
}

#[cfg(feature = "vips")]
fn vips_frame(path: &str, max_dim: Option<u32>) -> Result<RgbaImage, String> {
    let mut v =
        libvips::VipsImage::new_from_file_access(path, libvips::ops::Access::Sequential, false)
            .or_else(|_| libvips::VipsImage::new_from_file(path))
            .map_err(|e| e.to_string())?;
    if let Some(md) = max_dim.filter(|m| *m > 0) {
        let scale = (v.get_width() as f64 / md as f64).max(v.get_height() as f64 / md as f64);
        if scale > 1.0 {
            v = libvips::ops::resize(&v, 1.0 / scale).map_err(|e| e.to_string())?;
        }
    }
    vips_to_rgba(v)
}

#[cfg(feature = "vips")]
fn vips_thumbnail(path: &str, max_w: u32, max_h: u32) -> Result<RgbaImage, String> {
    let mut v = libvips::ops::thumbnail(path, max_w.max(1) as i32).map_err(|e| e.to_string())?;
    if v.get_height() > max_h as i32 {
        let s = max_h.max(1) as f64 / v.get_height() as f64;
        v = libvips::ops::resize(&v, s).map_err(|e| e.to_string())?;
    }
    vips_to_rgba(v)
}

/// Load all frames of an image.  Animated GIFs keep their per-frame
/// delays (clamped to >=1ms, default 75ms like tkiv.py).  When max_dim
/// is given, a single-page image whose largest dimension exceeds it is
/// scaled down (matches the pyvips path in tkiv.py).
pub fn load_frames(path: &str, max_dim: Option<u32>) -> Result<Frames, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".gif") {
        if let Ok(res) = load_gif_frames(path, max_dim) {
            return Ok(res);
        }
    } else if lower.ends_with(".webp") {
        if let Ok(res) = load_webp_frames(path, max_dim) {
            return Ok(res);
        }
    } else if lower.ends_with(".png") {
        if let Ok(res) = load_apng_frames(path, max_dim) {
            return Ok(res);
        }
    }
    #[cfg(feature = "vips")]
    if vips_enabled() {
        if let Ok(img) = vips_frame(path, max_dim) {
            return Ok((vec![img], vec![DEF_ANIM_DELAY_MS]));
        }
    }
    let img = image::open(path).map_err(|e| e.to_string())?;
    let mut rgba = img.to_rgba8();
    if let Some(md) = max_dim {
        if md > 0 {
            let (w, h) = (rgba.width(), rgba.height());
            let scale = (w as f64 / md as f64).max(h as f64 / md as f64);
            if scale > 1.0 {
                let nw = ((w as f64 / scale).round() as u32).max(1);
                let nh = ((h as f64 / scale).round() as u32).max(1);
                rgba =
                    image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Lanczos3);
            }
        }
    }
    Ok((vec![rgba], vec![DEF_ANIM_DELAY_MS]))
}

fn finish_animation_frames(
    mut imgs: Vec<RgbaImage>,
    mut delays: Vec<u32>,
    max_dim: Option<u32>,
) -> Result<Frames, String> {
    if imgs.is_empty() {
        return Err("no frames".to_string());
    }
    if let Some(md) = max_dim.filter(|m| *m > 0) {
        let (w, h) = (imgs[0].width(), imgs[0].height());
        let scale = (w as f64 / md as f64).max(h as f64 / md as f64);
        if scale > 1.0 {
            let nw = ((w as f64 / scale).round() as u32).max(1);
            let nh = ((h as f64 / scale).round() as u32).max(1);
            imgs = imgs
                .iter()
                .map(|im| {
                    image::imageops::resize(im, nw, nh, image::imageops::FilterType::Lanczos3)
                })
                .collect();
        }
    }
    for d in &mut delays {
        if *d == 0 {
            *d = DEF_ANIM_DELAY_MS;
        }
        *d = (*d).max(1);
    }
    Ok((imgs, delays))
}

fn collect_frames<'a>(it: image::Frames<'a>, max_dim: Option<u32>) -> Result<Frames, String> {
    let mut imgs = Vec::new();
    let mut delays = Vec::new();
    for f in it {
        let f = f.map_err(|e| e.to_string())?;
        let (num, den) = f.delay().numer_denom_ms();
        delays.push(num.checked_div(den).unwrap_or(0));
        imgs.push(f.into_buffer());
    }
    finish_animation_frames(imgs, delays, max_dim)
}

fn load_gif_frames(path: &str, max_dim: Option<u32>) -> Result<Frames, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let dec = GifDecoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    collect_frames(dec.into_frames(), max_dim)
}

fn load_webp_frames(path: &str, max_dim: Option<u32>) -> Result<Frames, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let dec = WebPDecoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    if !dec.has_animation() {
        return Err("not animated webp".into());
    }
    collect_frames(dec.into_frames(), max_dim)
}

fn load_apng_frames(path: &str, max_dim: Option<u32>) -> Result<Frames, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let png = PngDecoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let dec = png.apng().map_err(|e| e.to_string())?;
    collect_frames(dec.into_frames(), max_dim)
}

/// Gallery thumbnail: decode + fit within (max_w, max_h), RGBA.
pub fn load_gallery_thumb(path: &str, max_w: u32, max_h: u32) -> Result<RgbaImage, String> {
    #[cfg(feature = "vips")]
    if vips_enabled() {
        if let Ok(img) = vips_thumbnail(path, max_w, max_h) {
            return Ok(img);
        }
    }
    let img = image::open(path).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    Ok(image::imageops::thumbnail(
        &rgba,
        max_w.max(1),
        max_h.max(1),
    ))
}

fn fallback_thumb(path: &str, max_w: u32, max_h: u32) -> Option<RgbaImage> {
    let img = image::open(path).ok()?;
    Some(image::imageops::thumbnail(
        &img.to_rgba8(),
        max_w.max(1),
        max_h.max(1),
    ))
}

/// (width, height) without decoding pixels.
pub fn get_orig_size(path: &str) -> (u32, u32) {
    #[cfg(feature = "vips")]
    if vips_enabled() {
        if let Ok(v) =
            libvips::VipsImage::new_from_file_access(path, libvips::ops::Access::Sequential, false)
                .or_else(|_| libvips::VipsImage::new_from_file(path))
        {
            let w = v.get_width().max(0) as u32;
            let h = v.get_height().max(0) as u32;
            if w > 0 && h > 0 {
                return (w, h);
            }
        }
    }
    image::image_dimensions(path).unwrap_or((0, 0))
}

// ---------------------------------------------------------------- disk cache

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("~"));
            home.join(".cache")
        });
    base.join("tkiv_thumbs")
}

fn disk_cache_path(path: &str, w: u32, h: u32) -> Option<PathBuf> {
    let st = std::fs::metadata(path).ok()?;
    let mtime_ns = st
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha1::new();
    hasher.update(format!("{path}|{mtime_ns}|{}|{w}x{h}", st.len()).as_bytes());
    let key = hex(&hasher.finalize());
    Some(cache_dir().join(&key[..2]).join(format!("{key}.webp")))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

/// Selector thumbnail decode with webp disk cache.
/// (tkiv.py writes lossy webp q=82; the Rust `image` crate encoder is
/// lossless-only, so we write lossless webp — same cache layout and
/// invalidation semantics, larger cache entries.)
pub fn decode_thumb(path: &str, max_w: u32, max_h: u32) -> Option<(RgbaImage, (u32, u32))> {
    let orig_size = get_orig_size(path);
    let cache_file = disk_cache_path(path, max_w, max_h);

    if let Some(cf) = &cache_file {
        if cf.exists() {
            if let Ok(img) = image::open(cf) {
                return Some((img.to_rgba8(), orig_size));
            }
            let _ = std::fs::remove_file(cf);
        }
    }

    #[cfg(feature = "vips")]
    let thumb = if vips_enabled() {
        match vips_thumbnail(path, max_w, max_h) {
            Ok(t) => t,
            Err(_) => fallback_thumb(path, max_w, max_h)?,
        }
    } else {
        fallback_thumb(path, max_w, max_h)?
    };

    #[cfg(not(feature = "vips"))]
    let thumb = fallback_thumb(path, max_w, max_h)?;

    if let Some(cf) = &cache_file {
        if let Some(parent) = cf.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = cf.with_file_name(format!(
            "{}.tmp",
            cf.file_name().unwrap_or_default().to_string_lossy()
        ));
        let ok = std::fs::File::create(&tmp)
            .map_err(|e| e.to_string())
            .and_then(|mut f| {
                thumb
                    .write_to(&mut f, image::ImageFormat::WebP)
                    .map_err(|e| e.to_string())
            })
            .is_ok();
        if ok {
            let _ = std::fs::rename(&tmp, cf);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Some((thumb, orig_size))
}

/// Simple full-size decode for clipboard copy etc.
pub fn decode_full(path: &str, max_dim: u32) -> Result<DynamicImage, String> {
    Ok(image::open(path).map_err(|e| e.to_string())?.resize(
        max_dim,
        max_dim,
        image::imageops::FilterType::Lanczos3,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_png(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tkiv-rust-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.png"));
        let mut img = RgbaImage::new(32, 16);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 7) as u8, (y * 15) as u8, 128, 200]);
        }
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn loads_png_and_dimensions() {
        let p = test_png("loads");
        let s = p.to_string_lossy().into_owned();
        assert_eq!(get_orig_size(&s), (32, 16));
        let (frames, delays) = load_frames(&s, None).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].dimensions(), (32, 16));
        assert_eq!(frames[0].get_pixel(0, 0).0[3], 200);
        assert_eq!(delays, vec![DEF_ANIM_DELAY_MS]);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn respects_decode_size_limit() {
        let p = test_png("limits");
        let s = p.to_string_lossy().into_owned();
        let (frames, _) = load_frames(&s, Some(8)).unwrap();
        assert!(frames[0].width() <= 8 && frames[0].height() <= 8);
        let thumb = load_gallery_thumb(&s, 10, 5).unwrap();
        assert!(thumb.width() <= 10 && thumb.height() <= 5);
        let _ = std::fs::remove_file(p);
    }
}
