//! The overlay's window (follows `OverlayController` in `Overlay.swift`): a layered, click-through, never-activated,
//! topmost tool window, painted with `UpdateLayeredWindow` from a 32-bit DIB. It sits at the bottom (or top) centre of
//! the monitor with the mouse pointer, fades in and out, and hides itself a moment after a final state.
//!
//! The app's window drives it: `tick` about 30 times a second while `is_active`.

use super::canvas::Canvas;
use super::scene::{Phase, Scene, HEIGHT, WIDTH};
use super::text::GdiText;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, RegisterClassW, SetWindowPos, ShowWindow, UpdateLayeredWindow,
    HTTRANSPARENT, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

const FADE_IN: Duration = Duration::from_millis(180);
const FADE_OUT: Duration = Duration::from_millis(250);

pub struct Overlay {
    hwnd: HWND,
    /// Settings: show the overlay at all, and where.
    pub enabled: bool,
    pub at_top: bool,
    scene: Scene,
    surface: Option<Surface>,
    visible: bool,
    fade: Fade,
    hide_at: Option<Instant>,
}

/// Opacity going from one value to another.
struct Fade {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}

impl Fade {
    fn value(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32().max(1e-3);
        self.from + (self.to - self.from) * t.clamp(0.0, 1.0)
    }
}

/// The DIB the frames are painted into, for one monitor scale and position.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    canvas: Canvas,
    text: GdiText,
    origin: POINT,
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: ours, created in `Surface::new`.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Surface {
    fn new(scale: f32, origin: POINT) -> Option<Surface> {
        let (width, height) = ((WIDTH * scale).round() as usize, (HEIGHT * scale).round() as usize);
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
        // SAFETY: a memory DC and a top-down 32-bit DIB of width × height, released in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let previous = SelectObject(dc, bitmap.into());
            Some(Surface {
                dc,
                bitmap,
                previous,
                bits: bits as *mut u32,
                canvas: Canvas::new(width, height),
                text: GdiText::new(scale),
                origin,
            })
        }
    }
}

extern "system" fn overlay_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    // SAFETY: the default handling of our own window's messages.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

impl Overlay {
    pub fn new() -> windows::core::Result<Overlay> {
        // SAFETY: registers our class and creates a hidden popup with it.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance.into(),
                lpszClassName: w!("OpenVoiceType.Overlay"),
                ..Default::default()
            };
            RegisterClassW(&class);
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                w!("OpenVoiceType.Overlay"),
                w!("OpenVoiceType"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        Ok(Overlay {
            hwnd,
            enabled: true,
            at_top: false,
            scene: Scene::default(),
            surface: None,
            visible: false,
            fade: Fade { from: 0.0, to: 0.0, started: Instant::now(), duration: FADE_OUT },
            hide_at: None,
        })
    }

    /// Visible or fading: the app keeps calling `tick`.
    pub fn is_active(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self, phase: Phase) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        self.hide_at = None;
        self.scene.set_phase(phase, now);
        let current = self.fade.value(now);
        if !self.visible || self.fade.to < 1.0 {
            if !self.visible {
                self.place();
            }
            self.fade =
                Fade { from: if self.visible { current } else { 0.0 }, to: 1.0, started: now, duration: FADE_IN };
            self.visible = true;
            self.render(now);
            // SAFETY: our own window; shown without taking focus, and kept above other topmost windows.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ =
                    SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        } else {
            self.render(now);
        }
    }

    /// Shows a final state, then fades out after `after`.
    pub fn finish(&mut self, phase: Phase, after: Duration) {
        self.show(phase);
        if self.enabled {
            self.hide_at = Some(Instant::now() + after);
        }
    }

    pub fn hide(&mut self) {
        let now = Instant::now();
        self.hide_at = None;
        if self.visible && self.fade.to > 0.0 {
            self.fade = Fade { from: self.fade.value(now), to: 0.0, started: now, duration: FADE_OUT };
        }
    }

    /// One frame: the newest level (while recording), the hide timer, the fade, and the paint.
    pub fn tick(&mut self, level: f32) {
        if !self.visible {
            return;
        }
        let now = Instant::now();
        if self.scene.phase == Phase::Recording {
            self.scene.push_level(level);
        }
        if self.hide_at.is_some_and(|at| now >= at) {
            self.hide();
        }
        if self.fade.to == 0.0 && self.fade.value(now) <= 0.0 {
            self.visible = false;
            // SAFETY: our own window.
            let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            return;
        }
        self.render(now);
    }

    /// Centres the pill on the work area of the monitor with the mouse pointer, at that monitor's scale.
    fn place(&mut self) {
        // SAFETY: plain queries; a failed one leaves the defaults (the primary monitor at 100 %).
        let (work, dpi) = unsafe {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi_x, mut dpi_y) = (96, 96);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            (info.rcWork, dpi_x)
        };
        let scale = dpi as f32 / 96.0;
        let (width, height) = ((WIDTH * scale).round() as i32, (HEIGHT * scale).round() as i32);
        let x = work.left + (work.right - work.left - width) / 2;
        let y =
            if self.at_top { work.top + (8.0 * scale) as i32 } else { work.bottom - height - (28.0 * scale) as i32 };
        let origin = POINT { x, y };
        let same_scale = self.surface.as_ref().is_some_and(|s| (s.text.scale() - scale).abs() < 0.01);
        match &mut self.surface {
            Some(surface) if same_scale => surface.origin = origin,
            _ => self.surface = Surface::new(scale, origin),
        }
    }

    fn render(&mut self, now: Instant) {
        let alpha = self.fade.value(now);
        let Some(surface) = &mut self.surface else { return };
        let scale = surface.text.scale();
        self.scene.draw(&mut surface.canvas, scale, now, &mut surface.text);
        let size = SIZE { cx: surface.canvas.width as i32, cy: surface.canvas.height as i32 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (alpha * 255.0).round() as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: `bits` is the DIB's memory, exactly canvas-sized; the DC holds that DIB.
        unsafe {
            std::ptr::copy_nonoverlapping(surface.canvas.pixels.as_ptr(), surface.bits, surface.canvas.pixels.len());
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&surface.origin),
                Some(&size),
                Some(surface.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }
}
