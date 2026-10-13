use anyhow::{Context, ensure};
use std::{
    collections::HashMap,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use windows_sys::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, HGDIOBJ, SelectObject,
    },
    UI::{
        Shell::ExtractIconExW,
        WindowsAndMessaging::{DI_NOMIRROR, DI_NORMAL, DestroyIcon, DrawIconEx, HICON},
    },
};

const SIZE: usize = 32;
const BYTE_COUNT: usize = SIZE * SIZE * 4;
const CACHE_LIMIT: usize = 512;

struct Entry {
    stamp: Option<(u64, SystemTime)>,
    png: Option<Arc<Vec<u8>>>,
    used: Instant,
    expires: Instant,
}

/// File icons only; no extension association fallback or retained Windows handles.
#[derive(Default)]
pub struct ExecutableIcons {
    entries: HashMap<PathBuf, Entry>,
}

impl ExecutableIcons {
    /// Call from a blocking worker, since both metadata and extraction touch disk.
    pub fn get(&mut self, path: &Path) -> Option<Arc<Vec<u8>>> {
        let now = Instant::now();
        let stamp = std::fs::metadata(path)
            .and_then(|metadata| Ok((metadata.len(), metadata.modified()?)))
            .ok();
        if let Some(entry) = self.entries.get_mut(path)
            && entry.stamp == stamp
            && now < entry.expires
        {
            entry.used = now;
            return entry.png.clone();
        }
        let png = match extract(path) {
            Ok(png) => png.map(Arc::new),
            Err(error) => {
                tracing::debug!(path = %path.display(), %error, "程序图标读取失败");
                None
            }
        };
        if self.entries.len() >= CACHE_LIMIT
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone())
        {
            self.entries.remove(&oldest);
        }
        let ttl = if png.is_some() { 300 } else { 30 };
        self.entries.insert(
            path.to_owned(),
            Entry {
                stamp,
                png: png.clone(),
                used: now,
                expires: now + Duration::from_secs(ttl),
            },
        );
        png
    }
}

struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DestroyIcon(self.0);
            }
        }
    }
}

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
}
impl Surface {
    fn new() -> anyhow::Result<Self> {
        let mut surface = Self {
            dc: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
            bits: null_mut(),
        };
        surface.dc = unsafe { CreateCompatibleDC(null_mut()) };
        ensure!(!surface.dc.is_null(), "CreateCompatibleDC failed");
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE as i32,
                biHeight: -(SIZE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        surface.bitmap = unsafe {
            CreateDIBSection(surface.dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0)
        };
        ensure!(
            !surface.bitmap.is_null() && !bits.is_null(),
            "CreateDIBSection failed"
        );
        surface.bits = bits.cast();
        let previous = unsafe { SelectObject(surface.dc, surface.bitmap) };
        ensure!(
            !previous.is_null() && previous as isize != -1,
            "SelectObject failed"
        );
        surface.previous = previous;
        Ok(surface)
    }

    fn draw(&self, icon: HICON, background: u8) -> anyhow::Result<Vec<u8>> {
        // GDI and CPU access are sequenced on this worker's private DIB.
        unsafe {
            self.bits.write_bytes(background, BYTE_COUNT);
        }
        let drawn = unsafe {
            DrawIconEx(
                self.dc,
                0,
                0,
                icon,
                SIZE as i32,
                SIZE as i32,
                0,
                null_mut(),
                DI_NORMAL | DI_NOMIRROR,
            )
        };
        let flushed = unsafe { GdiFlush() };
        ensure!(drawn != 0 && flushed != 0, "DrawIconEx failed");
        Ok(unsafe { std::slice::from_raw_parts(self.bits, BYTE_COUNT) }.to_vec())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}

fn extract(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    let mut path: Vec<u16> = path.as_os_str().encode_wide().collect();
    ensure!(!path.contains(&0), "Icon path contains NUL");
    path.push(0);
    let mut icon = OwnedIcon(null_mut());
    let count = unsafe { ExtractIconExW(path.as_ptr(), 0, &mut icon.0, null_mut(), 1) };
    if count == 0 || count == u32::MAX || icon.0.is_null() {
        return Ok(None);
    }
    let surface = Surface::new()?;
    let black = surface.draw(icon.0, 0)?;
    let white = surface.draw(icon.0, 255)?;
    let mut rgba = Vec::with_capacity(BYTE_COUNT);
    // Two backgrounds recover alpha for both legacy masks and premultiplied icons.
    for (black, white) in black.chunks_exact(4).zip(white.chunks_exact(4)) {
        let alpha = 255
            - (0..3)
                .map(|i| white[i].saturating_sub(black[i]))
                .max()
                .unwrap_or(0);
        for i in [2, 1, 0] {
            rgba.push(if alpha == 0 {
                0
            } else {
                ((u32::from(black[i]) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                    as u8
            });
        }
        rgba.push(alpha);
    }
    ensure!(
        rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
        "Empty icon"
    );
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, SIZE as u32, SIZE as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("Encode icon PNG")?;
        writer.write_image_data(&rgba)?;
    }
    Ok(Some(png))
}
