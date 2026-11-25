//! A ring of dots that turns while something loads.
//!
//! Every place in the window that says "wait" used to say it with a still
//! word — "Loading posts...", "Searching…", "Loading earlier messages..." —
//! so a slow answer and a stuck one looked the same. This is the one
//! picture of waiting, drawn on the kit's own timer: it asks iced to draw
//! again while it shows and sends no message, so nothing is built or laid
//! out anew for it.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use iced::{Color, Length, Point, Rectangle, Size, advanced::renderer};

use super::{Gauge, gauge};

/// How many dots the ring is drawn from: enough to read as a circle at
/// 16 px, and few enough that each one is a dot rather than a smudge.
const DOTS: usize = 8;
/// One full turn.
const TURN: Duration = Duration::from_millis(900);

/// A ring of dots that turns while something loads, `size` pixels across in
/// the theme's `color`. It draws itself on a timer of its own.
pub fn spinner<'a>(size: f32, color: Color) -> Gauge<'a> {
    let every = TURN / DOTS as u32;
    gauge(every, move |renderer, bounds| {
        draw(renderer, bounds, color);
    })
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
}

/// A spinner centred on its row, for a panel that is waiting: the ring and
/// the word beside it, in the theme's muted ink.
pub fn loading<'a, M: 'a>(size: f32, color: Color) -> iced::Element<'a, M> {
    spinner(size, color).into()
}

/// How far the turn has come, 0..1. The clock starts when the first spinner
/// is drawn, not when the window opens, so a spinner that appears an hour
/// into a session starts at its beginning.
fn phase() -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let elapsed = start.elapsed().as_secs_f32();
    (elapsed / TURN.as_secs_f32()).fract()
}

/// The ring: `DOTS` dots around a circle, the one at the head brightest and
/// the trail fading behind it, as Discord's waiting ring turns.
fn draw(renderer: &mut super::Renderer, bounds: Rectangle, color: Color) {
    use iced::advanced::Renderer as _;
    let side = bounds.width.min(bounds.height);
    if side < 4.0 {
        return;
    }
    let radius = side / 2.0;
    let dot = (radius * 0.26).max(1.0);
    let centre = bounds.center();
    let turn = phase();
    for index in 0..DOTS {
        let angle = TAU * (index as f32 / DOTS as f32) - FRAC_PI_2;
        // The head is the dot at `turn`; every dot behind it fades.
        let behind = (turn - index as f32 / DOTS as f32).rem_euclid(1.0);
        let alpha = (1.0 - behind).powi(2);
        let at = Point::new(
            centre.x + angle.cos() * (radius - dot),
            centre.y + angle.sin() * (radius - dot),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(at.x - dot, at.y - dot),
                    Size::new(dot * 2.0, dot * 2.0),
                ),
                border: iced::Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: dot.into(),
                },
                ..Default::default()
            },
            Color {
                a: alpha * color.a,
                ..color
            },
        );
    }
}
