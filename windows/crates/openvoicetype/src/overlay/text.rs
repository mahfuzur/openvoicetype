//! The overlay's text through GDI: Segoe UI, grayscale anti-aliased, white on black into a scratch bitmap whose
//! brightness becomes the mask's coverage. (GDI can't draw into a layered window's alpha channel itself.) Masks are
//! cached by text, since the same few labels are drawn 30 times a second.

use super::canvas::Mask;
use super::scene::TextRenderer;
use std::collections::HashMap;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetTextExtentPoint32W,
    SelectObject, SetBkMode, SetTextColor, TextOutW, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, FF_DONTCARE, FW_SEMIBOLD, HDC, HFONT, OUT_TT_PRECIS,
    TRANSPARENT,
};

pub struct GdiText {
    dc: HDC,
    font: HFONT,
    scale: f32,
    cache: HashMap<String, Mask>,
}

impl GdiText {
    pub fn new(scale: f32) -> GdiText {
        // SAFETY: a memory DC and a font we own and delete in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let font = CreateFontW(
                -(14.0 * scale).round() as i32,
                0,
                0,
                0,
                FW_SEMIBOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_TT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                FF_DONTCARE.0 as u32,
                w!("Segoe UI"),
            );
            SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            GdiText { dc, font, scale, cache: HashMap::new() }
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }
}

impl Drop for GdiText {
    fn drop(&mut self) {
        // SAFETY: ours, created in `new`.
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.font.into());
        }
    }
}

impl TextRenderer for GdiText {
    fn measure(&mut self, text: &str) -> (f32, f32) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        // SAFETY: our DC with our font selected.
        let _ = unsafe { GetTextExtentPoint32W(self.dc, &wide, &mut size) };
        (size.cx as f32, size.cy as f32)
    }

    fn render(&mut self, text: &str) -> Mask {
        if let Some(mask) = self.cache.get(text) {
            return mask.clone();
        }
        let mask = self.draw(text);
        if self.cache.len() > 64 {
            self.cache.clear();
        }
        self.cache.insert(text.to_string(), mask.clone());
        mask
    }
}

impl GdiText {
    /// White text on the black of a fresh DIB; the green channel is the coverage.
    fn draw(&mut self, text: &str) -> Mask {
        let (w, h) = self.measure(text);
        let (width, height) = (w.max(1.0) as usize, h.max(1.0) as usize);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut coverage = vec![0u8; width * height];
        // SAFETY: the DIB is width × height 32-bit pixels, read only after GdiFlush; we restore and delete it.
        unsafe {
            let mut bits = std::ptr::null_mut();
            if let Ok(bitmap) = CreateDIBSection(Some(self.dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                let previous = SelectObject(self.dc, bitmap.into());
                let _ = TextOutW(self.dc, 0, 0, &wide);
                let _ = GdiFlush();
                let pixels = std::slice::from_raw_parts(bits as *const u32, width * height);
                for (out, pixel) in coverage.iter_mut().zip(pixels) {
                    *out = ((pixel >> 8) & 0xFF) as u8;
                }
                SelectObject(self.dc, previous);
                let _ = DeleteObject(bitmap.into());
            }
        }
        Mask { width, height, coverage }
    }
}
