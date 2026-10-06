//! The icon an executable shows in Explorer, for the Name column.
//!
//! Task Manager puts each program's own icon beside its name, and a list of
//! six hundred bare names is markedly slower to scan: people find Chrome by
//! its colours long before they read the word. The sampler does not carry
//! icons — extracting one costs a file open, a resource walk and a GDI round
//! trip — so they are asked for by the screen, for the rows it shows, and
//! cached here per executable path: fifty `chrome.exe` renderers cost one
//! extraction, and a path is never extracted twice in a session.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use base64::Engine as _;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, ReleaseDC,
};
use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
};

/// Edge of the extracted bitmap, in pixels.
///
/// Twice the 16 px the row draws, so a 200 % display gets a sharp icon
/// rather than a smeared upscale; the webview scales it down for 100 %.
pub const ICON_SIZE: i32 = 32;

/// Extractions per path, including `None` for a file with no readable icon,
/// so a protected or vanished executable is asked once.
fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::with_capacity(256)))
}

/// The executable's icon as a `data:image/png;base64,…` URL, cached by path.
///
/// A data URL rather than bytes: Tauri serialises `Vec<u8>` as a JSON array
/// of numbers, four times the size and parsed on the webview's main thread,
/// and the URL drops straight into an `<img src>` the CSP already allows.
#[must_use]
pub fn icon_data_url(path: &str) -> Option<String> {
    let key = path.to_ascii_lowercase();
    if let Some(known) = cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return known.clone();
    }
    // Extracted outside the lock: a slow network-share executable must not
    // hold up every other row's lookup.
    let url = extract_rgba(Path::new(path))
        .and_then(|(width, height, rgba)| encode_png(width, height, &rgba))
        .map(|png| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        });
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, url.clone());
    url
}

/// The file's own first icon at [`ICON_SIZE`], else the shell's icon for it
/// The file's own first icon at [`ICON_SIZE`], else the shell's icon for it
/// (the generic program icon for an executable that ships none — which is
/// what Explorer and Task Manager show too).
fn extract_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = path.to_str()?.encode_utf16().chain(Some(0)).collect();
    let mut icon: HICON = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and alive; one HICON slot is provided
    // and the id out-param may be null.
    let extracted = unsafe {
        PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            ICON_SIZE,
            ICON_SIZE,
            &raw mut icon,
            std::ptr::null_mut(),
            1,
            0,
        )
    };
    if extracted == 0 || extracted == u32::MAX || icon.is_null() {
        // SAFETY: SHFILEINFOW is plain data; all-zero is a valid empty value.
        let mut info: SHFILEINFOW = unsafe { std::mem::zeroed() };
        let size = u32::try_from(size_of::<SHFILEINFOW>()).ok()?;
        // SAFETY: `wide` is live; `info` is sized as declared.
        let ok = unsafe {
            SHGetFileInfoW(
                wide.as_ptr(),
                0,
                &raw mut info,
                size,
                SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if ok == 0 || info.hIcon.is_null() {
            return None;
        }
        icon = info.hIcon;
    }
    let rgba = icon_to_rgba(icon);
    // SAFETY: the icon was created for us by one of the calls above.
    unsafe { DestroyIcon(icon) };
    rgba
}

/// Converts an icon to straight RGBA, top row first, with its dimensions.
fn icon_to_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    // SAFETY: ICONINFO is plain data, filled by GetIconInfo.
    let mut info: ICONINFO = unsafe { std::mem::zeroed() };
    // SAFETY: `icon` is a live icon handle.
    if unsafe { GetIconInfo(icon, &raw mut info) } == 0 {
        return None;
    }
    let result = if info.hbmColor.is_null() {
        None
    } else {
        bitmap_bgra(info.hbmColor).map(|(width, height, mut bgra)| {
            // Icons from before alpha channels (XP-era and many installers)
            // carry an all-zero alpha byte and say transparency through the
            // AND mask instead. Read that way, they would be invisible.
            if bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
                let mask = bitmap_bgra(info.hbmMask)
                    .filter(|(w, h, _)| *w == width && *h >= height)
                    .map(|(_, _, m)| m);
                for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let transparent = mask
                        .as_ref()
                        .and_then(|m| m.get(i * 4))
                        .is_some_and(|b| *b != 0);
                    px[3] = if transparent { 0 } else { 255 };
                }
            }
            for px in bgra.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
            (width.unsigned_abs(), height.unsigned_abs(), bgra)
        })
    };
    // SAFETY: GetIconInfo hands us copies of both bitmaps; we own and free them.
    unsafe {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
    }
    result
}

/// Reads a bitmap as 32-bit BGRA, top-down, with its dimensions.
fn bitmap_bgra(bitmap: HBITMAP) -> Option<(i32, i32, Vec<u8>)> {
    // SAFETY: BITMAP is plain data, filled by GetObjectW.
    let mut header: BITMAP = unsafe { std::mem::zeroed() };
    let size = i32::try_from(size_of::<BITMAP>()).ok()?;
    // SAFETY: `header` is `size` bytes.
    if unsafe { GetObjectW(bitmap, size, (&raw mut header).cast()) } == 0 {
        return None;
    }
    let (width, height) = (header.bmWidth, header.bmHeight);
    if width <= 0 || height <= 0 {
        return None;
    }
    // SAFETY: BITMAPINFO is plain data; the header is filled in below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
        biWidth: width,
        // Negative: top-down rows, so no flip is needed afterwards.
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    let mut pixels = vec![0_u8; usize::try_from(width).ok()? * usize::try_from(height).ok()? * 4];
    // SAFETY: a screen DC, released below.
    let dc = unsafe { GetDC(std::ptr::null_mut()) };
    // SAFETY: `pixels` holds width × height 32-bit pixels, as `bmi` declares.
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height.unsigned_abs(),
            pixels.as_mut_ptr().cast(),
            &raw mut bmi,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: obtained from GetDC above.
    unsafe { ReleaseDC(std::ptr::null_mut(), dc) };
    (lines > 0).then_some((width, height, pixels))
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(4096);
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
    }
    out.flush().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(url: &str) -> (u32, u32, Vec<u8>) {
        let b64 = url
            .strip_prefix("data:image/png;base64,")
            .expect("a png data url");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("valid base64");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("png header");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("png frame");
        (info.width, info.height, buf)
    }

    #[test]
    fn explorer_has_a_visible_icon_of_the_requested_size() {
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let url = icon_data_url(&format!(r"{windir}\explorer.exe")).expect("explorer has an icon");
        let (w, h, rgba) = decode(&url);
        assert_eq!((w, h), (32, 32));
        // Not a blank square: some pixels are opaque and some are not, which
        // is what an icon with a shape looks like (an all-zero alpha read
        // would be all transparent; a lost alpha would be all opaque).
        let opaque = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 200)
            .count();
        assert!(opaque > 64, "only {opaque} opaque pixels");
        assert!(opaque < 1024, "every pixel opaque: the alpha was lost");
    }

    #[test]
    fn a_missing_file_has_no_icon_and_is_remembered_as_such() {
        let path = r"C:\definitely\not\here\nothing.exe";
        assert_eq!(icon_data_url(path), None);
        assert_eq!(
            cache()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&path.to_ascii_lowercase()),
            Some(&None)
        );
    }
}
