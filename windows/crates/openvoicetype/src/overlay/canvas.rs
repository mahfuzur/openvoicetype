//! A tiny anti-aliased painter for the overlay pill and the tray icon (what SwiftUI and AppKit draw on the Mac):
//! rounded rectangles, circles and lines as signed distance fields, plus text masks, into premultiplied BGRA pixels,
//! the format `UpdateLayeredWindow` takes. Pure, so it's tested on every platform.

/// x, y, width, height in pixels.
pub type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
        Color { r, g, b, a: 1.0 }
    }

    pub fn alpha(self, a: f32) -> Color {
        Color { a: self.a * a, ..self }
    }
}

/// Coverage of one text bitmap (0–255 per pixel), drawn in any colour.
#[derive(Clone)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
}

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    /// Premultiplied 0xAARRGGBB: in memory B, G, R, A, as a 32-bit top-down DIB.
    pub pixels: Vec<u32>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Canvas {
        Canvas { width, height, pixels: vec![0; width * height] }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    /// Source-over of `color` at `coverage` (0–1) onto one pixel.
    fn blend(&mut self, x: usize, y: usize, color: Color, coverage: f32) {
        let a = (color.a * coverage).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let pixel = &mut self.pixels[y * self.width + x];
        let channel = |shift: u32| ((*pixel >> shift) & 0xFF) as f32;
        let mix = |source: f32, shift: u32| (source * a * 255.0 + channel(shift) * (1.0 - a)).round().min(255.0) as u32;
        let alpha = (a * 255.0 + channel(24) * (1.0 - a)).round().min(255.0) as u32;
        *pixel = alpha << 24 | mix(color.r, 16) << 16 | mix(color.g, 8) << 8 | mix(color.b, 0);
    }

    /// Fills where `distance` (negative inside, in pixels) says, within the box, anti-aliased over one pixel.
    fn fill_sdf(&mut self, bounds: (f32, f32, f32, f32), color: Color, distance: impl Fn(f32, f32) -> f32) {
        let (x0, y0, x1, y1) = self.clip(bounds);
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = (0.5 - distance(x as f32 + 0.5, y as f32 + 0.5)).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.blend(x, y, color, coverage);
                }
            }
        }
    }

    /// Pixel range of a box (with a pixel of margin for the anti-aliasing), inside the canvas.
    fn clip(&self, (left, top, right, bottom): (f32, f32, f32, f32)) -> (usize, usize, usize, usize) {
        let clamp = |v: f32, max: usize| (v.max(0.0) as usize).min(max);
        (
            clamp(left.floor() - 1.0, self.width),
            clamp(top.floor() - 1.0, self.height),
            clamp(right.ceil() + 1.0, self.width),
            clamp(bottom.ceil() + 1.0, self.height),
        )
    }

    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        let (x, y, w, h) = rect;
        self.fill_sdf((x, y, x + w, y + h), color, |px, py| rounded_rect(px, py, rect, radius));
    }

    /// A border of `width` inside the rectangle's edge.
    pub fn stroke_rounded_rect(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        let (x, y, w, h) = rect;
        self.fill_sdf((x, y, x + w, y + h), color, |px, py| {
            let d = rounded_rect(px, py, rect, radius);
            d.max(-d - width)
        });
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        self.fill_sdf((cx - r, cy - r, cx + r, cy + r), color, |px, py| (px - cx).hypot(py - cy) - r);
    }

    /// A line with round caps.
    pub fn stroke_line(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), width: f32, color: Color) {
        let r = width / 2.0;
        let bounds = (x0.min(x1) - r, y0.min(y1) - r, x0.max(x1) + r, y0.max(y1) + r);
        self.fill_sdf(bounds, color, |px, py| {
            let (dx, dy) = (x1 - x0, y1 - y0);
            let t = (((px - x0) * dx + (py - y0) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
            (px - (x0 + t * dx)).hypot(py - (y0 + t * dy)) - r
        });
    }

    /// A soft shadow around a rounded rectangle, fading out over `blur` pixels.
    pub fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, color: Color) {
        let (x, y, w, h) = rect;
        let (x0, y0, x1, y1) = self.clip((x - blur, y - blur, x + w + blur, y + h + blur));
        for py in y0..y1 {
            for px in x0..x1 {
                let d = rounded_rect(px as f32 + 0.5, py as f32 + 0.5, rect, radius);
                let fade = 1.0 - (d / blur).clamp(0.0, 1.0);
                self.blend(px, py, color, fade * fade);
            }
        }
    }

    /// Cuts a hole (for the gap around the tray icon's red dot).
    pub fn erase_circle(&mut self, cx: f32, cy: f32, r: f32) {
        let (x0, y0, x1, y1) = self.clip((cx - r, cy - r, cx + r, cy + r));
        for y in y0..y1 {
            for x in x0..x1 {
                let coverage = (0.5 - ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) - r)).clamp(0.0, 1.0);
                let keep = 1.0 - coverage;
                let pixel = &mut self.pixels[y * self.width + x];
                let scaled = |shift: u32| ((((*pixel >> shift) & 0xFF) as f32 * keep).round() as u32) << shift;
                *pixel = scaled(24) | scaled(16) | scaled(8) | scaled(0);
            }
        }
    }

    /// Draws a text mask with its top-left corner at (x, y), clipped to the canvas.
    pub fn draw_mask(&mut self, mask: &Mask, x: i32, y: i32, color: Color) {
        for my in 0..mask.height {
            for mx in 0..mask.width {
                let (cx, cy) = (x + mx as i32, y + my as i32);
                let coverage = mask.coverage[my * mask.width + mx];
                if coverage > 0 && cx >= 0 && cy >= 0 && (cx as usize) < self.width && (cy as usize) < self.height {
                    self.blend(cx as usize, cy as usize, color, coverage as f32 / 255.0);
                }
            }
        }
    }

    /// Straight-alpha RGBA bytes (for `tray_icon::Icon::from_rgba`).
    pub fn to_rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&p| {
                let a = (p >> 24) & 0xFF;
                let straight = |shift: u32| (((p >> shift) & 0xFF) * 255).checked_div(a).unwrap_or(0).min(255) as u8;
                [straight(16), straight(8), straight(0), a as u8]
            })
            .collect()
    }
}

/// Signed distance from a point to a rounded rectangle (negative inside).
fn rounded_rect(px: f32, py: f32, (x, y, w, h): Rect, radius: f32) -> f32 {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let (hw, hh) = (w / 2.0 - r, h / 2.0 - r);
    let (dx, dy) = ((px - x - w / 2.0).abs() - hw, (py - y - h / 2.0).abs() - hh);
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(canvas: &Canvas, x: usize, y: usize) -> u32 {
        canvas.pixels[y * canvas.width + x] >> 24
    }

    #[test]
    fn fills_and_antialiases() {
        let mut canvas = Canvas::new(20, 20);
        canvas.fill_rounded_rect((2.0, 2.0, 16.0, 16.0), 4.0, Color::WHITE);
        assert_eq!(canvas.pixels[10 * 20 + 10], 0xFFFF_FFFF);
        assert_eq!(alpha(&canvas, 0, 0), 0);
        assert_eq!(alpha(&canvas, 2, 2), 0); // the rounded corner
        canvas.clear();
        canvas.fill_circle(10.0, 10.0, 5.0, Color::rgb(1.0, 0.0, 0.0).alpha(0.5));
        assert_eq!(canvas.pixels[10 * 20 + 10], 0x80800000); // premultiplied half red
        let edge = alpha(&canvas, 14, 10);
        assert!(edge > 0 && edge < 128);
    }

    #[test]
    fn strokes_lines_masks_and_holes() {
        let mut canvas = Canvas::new(20, 20);
        canvas.stroke_rounded_rect((0.0, 0.0, 20.0, 20.0), 5.0, 1.0, Color::WHITE);
        assert_eq!(alpha(&canvas, 10, 0), 255);
        assert_eq!(alpha(&canvas, 10, 10), 0);
        canvas.stroke_line((2.0, 10.5), (18.0, 10.5), 3.0, Color::WHITE);
        assert_eq!(alpha(&canvas, 10, 10), 255);
        canvas.erase_circle(10.0, 10.0, 3.0);
        assert_eq!(alpha(&canvas, 10, 10), 0);
        let mask = Mask { width: 2, height: 1, coverage: vec![255, 0] };
        canvas.draw_mask(&mask, 4, 4, Color::rgb(0.0, 1.0, 0.0));
        assert_eq!(canvas.pixels[4 * 20 + 4], 0xFF00FF00);
        canvas.draw_mask(&mask, -1, 25, Color::WHITE); // off the canvas: ignored
    }

    #[test]
    fn rgba_is_straight_alpha() {
        let canvas = Canvas { width: 2, height: 1, pixels: vec![0x80800000, 0] };
        assert_eq!(canvas.to_rgba(), vec![255, 0, 0, 128, 0, 0, 0, 0]);
    }
}
