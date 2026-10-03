//! What the overlay pill shows and how it's laid out (follows `OverlayModel` and `OverlayView` in `Overlay.swift`):
//! the phase, the recent input levels, and a draw function that paints one frame. Text comes from a `TextRenderer`, so
//! this module is pure and its layout is tested on every platform.

use super::canvas::{Canvas, Color, Mask};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The window's size in points (scaled by the monitor's DPI); the pill is centred in it.
pub const WIDTH: f32 = 560.0;
pub const HEIGHT: f32 = 72.0;
const PILL_HEIGHT: f32 = 40.0;
const PADDING: f32 = 16.0;
const SPACING: f32 = 10.0;
const BAR_COUNT: usize = 18;

const BLUE: Color = Color::rgb(0.35, 0.65, 1.0);
const ORANGE: Color = Color::rgb(1.0, 0.62, 0.2);
const PURPLE: Color = Color::rgb(0.75, 0.6, 1.0);
const GREEN: Color = Color::rgb(0.2, 0.78, 0.35);
const RED: Color = Color::rgb(1.0, 0.23, 0.19);

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// A slow (Bluetooth) mic is starting.
    Connecting(String),
    Recording,
    Transcribing,
    Polishing {
        offline: bool,
    },
    Success(String),
    Message {
        text: String,
        error: bool,
    },
}

/// Draws text in the overlay's font at the frame's scale.
pub trait TextRenderer {
    /// Width and height in pixels.
    fn measure(&mut self, text: &str) -> (f32, f32);
    fn render(&mut self, text: &str) -> Mask;
}

pub struct Scene {
    pub phase: Phase,
    levels: VecDeque<f32>,
    recording_started: Instant,
    /// The phase was set at (for the pulse and the shake).
    phase_started: Instant,
}

/// One thing in the pill's row.
enum Item {
    PulsingDot,
    Waveform,
    ProcessingBars(Color),
    Check,
    Alert(bool),
    Text { text: String, alpha: f32 },
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            phase: Phase::Recording,
            levels: VecDeque::from(vec![0.0; BAR_COUNT]),
            recording_started: Instant::now(),
            phase_started: Instant::now(),
        }
    }
}

impl Scene {
    pub fn set_phase(&mut self, phase: Phase, now: Instant) {
        if phase == Phase::Recording && self.phase != Phase::Recording {
            self.levels = VecDeque::from(vec![0.0; BAR_COUNT]);
            self.recording_started = now;
        }
        self.phase = phase;
        self.phase_started = now;
    }

    /// The newest level goes on the right.
    pub fn push_level(&mut self, level: f32) {
        self.levels.pop_front();
        self.levels.push_back(level.clamp(0.0, 1.0));
    }

    /// Paints one frame into a canvas of `WIDTH` × `HEIGHT` times `scale`.
    pub fn draw(&self, canvas: &mut Canvas, scale: f32, now: Instant, text: &mut dyn TextRenderer) {
        canvas.clear();
        let items = self.items(now);
        let max_text = WIDTH - 2.0 * PADDING - 60.0;
        let widths: Vec<f32> = items.iter().map(|item| item_width(item, text, scale).min(max_text * scale)).collect();
        let content = widths.iter().sum::<f32>() + SPACING * scale * (items.len().saturating_sub(1)) as f32;
        let pill_w = (content + 2.0 * PADDING * scale).min(canvas.width as f32 - 24.0 * scale);
        let pill_h = PILL_HEIGHT * scale;
        let x = (canvas.width as f32 - pill_w) / 2.0 + self.shake(now) * scale;
        let y = (canvas.height as f32 - pill_h) / 2.0;
        canvas.shadow((x, y + 4.0 * scale, pill_w, pill_h), pill_h / 2.0, 10.0 * scale, Color::BLACK.alpha(0.3));
        canvas.fill_rounded_rect((x, y, pill_w, pill_h), pill_h / 2.0, Color::BLACK.alpha(0.85));
        canvas.stroke_rounded_rect((x, y, pill_w, pill_h), pill_h / 2.0, scale, Color::WHITE.alpha(0.14));
        let mut cursor = x + PADDING * scale;
        let middle = y + pill_h / 2.0;
        for (item, width) in items.iter().zip(widths) {
            self.draw_item(canvas, item, cursor, middle, width, scale, now, text);
            cursor += width + SPACING * scale;
        }
    }

    fn items(&self, now: Instant) -> Vec<Item> {
        let working = |label: &str| Item::Text { text: label.to_string(), alpha: self.pulse(now) };
        match &self.phase {
            Phase::Connecting(device) => {
                vec![Item::ProcessingBars(BLUE), working(&format!("Connecting to {device}"))]
            }
            Phase::Recording => {
                let seconds = now.saturating_duration_since(self.recording_started).as_secs();
                let timer = format!("{}:{:02}", seconds / 60, seconds % 60);
                vec![Item::PulsingDot, Item::Waveform, Item::Text { text: timer, alpha: 0.7 }]
            }
            Phase::Transcribing => vec![Item::ProcessingBars(ORANGE), working("Transcribing")],
            Phase::Polishing { offline } => {
                vec![Item::ProcessingBars(PURPLE), working(if *offline { "Polishing offline" } else { "Polishing" })]
            }
            Phase::Success(label) => vec![Item::Check, Item::Text { text: label.clone(), alpha: 1.0 }],
            Phase::Message { text, error } => vec![Item::Alert(*error), Item::Text { text: text.clone(), alpha: 1.0 }],
        }
    }

    /// The working labels breathe a little (the Mac shimmers them).
    fn pulse(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.phase_started).as_secs_f32();
        0.7 + 0.25 * (t * std::f32::consts::TAU / 1.5).sin()
    }

    /// An error shakes the pill sideways for 0.45 s.
    fn shake(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.phase_started);
        match self.phase {
            Phase::Message { error: true, .. } if t < Duration::from_millis(450) => {
                7.0 * (t.as_secs_f32() / 0.45 * std::f32::consts::PI * 6.0).sin()
            }
            _ => 0.0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_item(
        &self,
        canvas: &mut Canvas,
        item: &Item,
        x: f32,
        middle: f32,
        width: f32,
        scale: f32,
        now: Instant,
        text: &mut dyn TextRenderer,
    ) {
        match item {
            Item::PulsingDot => {
                let p = now.saturating_duration_since(self.phase_started).as_secs_f32() % 1.1 / 1.1;
                let centre = x + 9.0 * scale;
                canvas.fill_circle(centre, middle, (4.5 + 4.5 * p) * scale, RED.alpha(0.35 * (1.0 - p)));
                canvas.fill_circle(centre, middle, 4.5 * scale, RED);
            }
            Item::Waveform => {
                for (index, level) in self.levels.iter().enumerate() {
                    let height = (3.0 + level * 21.0) * scale;
                    let bar_x = x + index as f32 * 6.0 * scale;
                    let color = Color::WHITE.alpha(0.55 + 0.45 * level);
                    canvas.fill_rounded_rect((bar_x, middle - height / 2.0, 3.0 * scale, height), 1.5 * scale, color);
                }
            }
            Item::ProcessingBars(color) => {
                let t = now.saturating_duration_since(self.phase_started).as_secs_f32();
                for index in 0..5 {
                    let phase = (t * 7.0 - index as f32 * 0.7).sin();
                    let height = (5.0 + 11.0 * (phase + 1.0) / 2.0) * scale;
                    let bar_x = x + index as f32 * 6.0 * scale;
                    canvas.fill_rounded_rect((bar_x, middle - height / 2.0, 3.0 * scale, height), 1.5 * scale, *color);
                }
            }
            Item::Check => {
                let (cx, r) = (x + 8.0 * scale, 8.0 * scale);
                canvas.fill_circle(cx, middle, r, GREEN);
                let point = |dx: f32, dy: f32| (cx + dx * scale, middle + dy * scale);
                canvas.stroke_line(point(-3.5, 0.2), point(-1.0, 2.8), 1.8 * scale, Color::WHITE);
                canvas.stroke_line(point(-1.0, 2.8), point(3.8, -2.6), 1.8 * scale, Color::WHITE);
            }
            Item::Alert(error) => {
                let (cx, r) = (x + 8.0 * scale, 8.0 * scale);
                canvas.fill_circle(cx, middle, r, if *error { RED } else { Color::WHITE.alpha(0.3) });
                canvas.stroke_line((cx, middle - 4.0 * scale), (cx, middle + 0.8 * scale), 1.8 * scale, Color::WHITE);
                canvas.fill_circle(cx, middle + 3.8 * scale, 1.1 * scale, Color::WHITE);
            }
            Item::Text { text: label, alpha } => {
                let label = fitted(label, width, text);
                let mask = text.render(&label);
                let top = (middle - mask.height as f32 / 2.0).round() as i32;
                canvas.draw_mask(&mask, x.round() as i32, top, Color::WHITE.alpha(*alpha));
            }
        }
    }
}

fn item_width(item: &Item, text: &mut dyn TextRenderer, scale: f32) -> f32 {
    match item {
        Item::PulsingDot => 18.0 * scale,
        Item::Waveform => (BAR_COUNT as f32 * 6.0 - 3.0) * scale,
        Item::ProcessingBars(_) => 27.0 * scale,
        Item::Check | Item::Alert(_) => 16.0 * scale,
        Item::Text { text: label, .. } => text.measure(label).0,
    }
}

/// The label, cut with "…" until it fits `width`.
fn fitted(label: &str, width: f32, text: &mut dyn TextRenderer) -> String {
    if text.measure(label).0 <= width + 0.5 {
        return label.to_string();
    }
    let mut chars: Vec<char> = label.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = format!("{}…", chars.iter().collect::<String>().trim_end());
        if text.measure(&candidate).0 <= width + 0.5 {
            return candidate;
        }
    }
    String::new()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every character is a 7 × 13 block.
    pub struct BlockText;

    impl TextRenderer for BlockText {
        fn measure(&mut self, text: &str) -> (f32, f32) {
            (text.chars().count() as f32 * 7.0, 13.0)
        }
        fn render(&mut self, text: &str) -> Mask {
            let width = text.chars().count() * 7;
            Mask { width, height: 13, coverage: vec![255; width * 13] }
        }
    }

    fn frame(scene: &Scene, now: Instant) -> Canvas {
        let mut canvas = Canvas::new(WIDTH as usize, HEIGHT as usize);
        scene.draw(&mut canvas, 1.0, now, &mut BlockText);
        canvas
    }

    /// The pill's left edge on the middle row.
    fn pill_left(canvas: &Canvas) -> usize {
        let row = canvas.height / 2 * canvas.width;
        (0..canvas.width).find(|&x| canvas.pixels[row + x] >> 24 > 200).unwrap()
    }

    #[test]
    fn pill_fits_its_content() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.set_phase(Phase::Success("Pasted".into()), start);
        let short = frame(&scene, start);
        scene.set_phase(Phase::Success("Copied: you switched to another window. Press Ctrl+V".into()), start);
        let long = frame(&scene, start);
        assert!(pill_left(&long) < pill_left(&short));
        // Width: padding + check + spacing + 6 characters + padding.
        assert_eq!(WIDTH as usize - 2 * pill_left(&short), (16.0 + 16.0 + 10.0 + 42.0 + 16.0) as usize);
    }

    #[test]
    fn long_text_is_cut() {
        assert_eq!(fitted("abcdefgh", 35.0, &mut BlockText), "abcd…");
        assert_eq!(fitted("abc", 35.0, &mut BlockText), "abc");
    }

    #[test]
    fn recording_shows_levels_and_errors_shake() {
        let start = Instant::now();
        let mut scene = Scene::default();
        scene.set_phase(Phase::Recording, start);
        for _ in 0..BAR_COUNT {
            scene.push_level(1.0);
        }
        let loud = frame(&scene, start);
        scene.set_phase(Phase::Transcribing, start);
        scene.set_phase(Phase::Recording, start);
        assert!(scene.levels.iter().all(|l| *l == 0.0));
        let quiet = frame(&scene, start);
        let opaque = |c: &Canvas| c.pixels.iter().filter(|p| **p >> 24 == 255).count();
        assert!(opaque(&loud) > opaque(&quiet));

        scene.set_phase(Phase::Message { text: "Transcription failed".into(), error: true }, start);
        assert!(scene.shake(start + Duration::from_millis(40)).abs() > 1.0);
        assert_eq!(scene.shake(start + Duration::from_millis(500)), 0.0);
    }
}
