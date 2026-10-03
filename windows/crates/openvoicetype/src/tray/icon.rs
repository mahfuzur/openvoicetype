//! The notification-area icon (follows `MenuBarIcon.swift`): still waveform bars, like the app icon, plus a red dot
//! while the app is working. Windows has no template images, so the bars are drawn white on a dark taskbar and black
//! on a light one. Pure, so it's tested on every platform.

use crate::overlay::canvas::{Canvas, Color};

/// Pixels per side: Windows scales it down to 16–24 px depending on the display scale.
pub const SIZE: usize = 32;

/// Straight-alpha RGBA, `SIZE` × `SIZE`.
pub fn rgba(busy: bool, light_taskbar: bool) -> Vec<u8> {
    // The Mac icon's geometry, on an 18-point square.
    let scale = SIZE as f32 / 18.0;
    let mut canvas = Canvas::new(SIZE, SIZE);
    let ink = if light_taskbar { Color::BLACK } else { Color::WHITE };
    let heights = [0.38, 0.7, 1.0, 0.7, 0.38];
    let (bar, gap, tallest) = (2.2 * scale, 1.3 * scale, 14.0 * scale);
    let total = heights.len() as f32 * bar + (heights.len() - 1) as f32 * gap;
    let centre = SIZE as f32 / 2.0;
    for (index, fraction) in heights.iter().enumerate() {
        let height = tallest * fraction;
        let x = centre - total / 2.0 + index as f32 * (bar + gap);
        canvas.fill_rounded_rect((x, centre - height / 2.0, bar, height), bar / 2.0, ink);
    }
    if busy {
        // Bottom right, with a gap cut around it so it reads at 16 px.
        let (cx, cy, r) = ((11.2 + 3.4) * scale, (18.0 - 1.0 - 3.4) * scale, 3.4 * scale);
        canvas.erase_circle(cx, cy, r + 1.4 * scale);
        canvas.fill_circle(cx, cy, r, Color::rgb(1.0, 0.23, 0.19));
    }
    canvas.to_rgba()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * SIZE + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn draws_bars_and_the_busy_dot() {
        let idle = rgba(false, false);
        assert_eq!(idle.len(), SIZE * SIZE * 4);
        assert_eq!(pixel(&idle, 16, 16), [255, 255, 255, 255]); // the middle bar
        assert_eq!(pixel(&idle, 1, 1)[3], 0);
        assert_eq!(pixel(&rgba(false, true), 16, 16), [0, 0, 0, 255]);
        let busy = rgba(true, false);
        assert_eq!(pixel(&busy, 26, 24), [255, 59, 48, 255]); // the red dot
        assert_eq!(pixel(&idle, 26, 24)[3], 0);
    }
}
