//! AppKit screen coordinates, independent of AppKit and display availability.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Position {
    pub center_x: f64,
    pub y: f64,
    pub width: f64,
}
impl Default for Position {
    fn default() -> Self {
        Self {
            center_x: 300.,
            y: 32.,
            width: 600.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
/// Prefer the screen with most overlap; disconnected/offscreen positions use the nearest screen.
pub fn constrain(position: Position, height: f64, screens: &[Frame]) -> Frame {
    let finite = |v: f64, fallback| if v.is_finite() { v } else { fallback };
    let width = finite(position.width, 600.).clamp(420., 900.);
    let frame = Frame {
        x: finite(position.center_x, 300.) - width / 2.,
        y: finite(position.y, 32.),
        width,
        height: finite(height, 90.).max(1.),
    };
    let overlap = |s: &Frame| {
        ((frame.x + width).min(s.x + s.width) - frame.x.max(s.x)).max(0.)
            * ((frame.y + frame.height).min(s.y + s.height) - frame.y.max(s.y)).max(0.)
    };
    let distance = |s: &Frame| {
        (frame.x + width / 2. - (s.x + s.width / 2.)).powi(2)
            + (frame.y + frame.height / 2. - (s.y + s.height / 2.)).powi(2)
    };
    let best = screens
        .iter()
        .filter(|s| {
            s.width > 0.
                && s.height > 0.
                && [s.x, s.y, s.width, s.height].iter().all(|v| v.is_finite())
        })
        .max_by(|a, b| {
            overlap(a)
                .total_cmp(&overlap(b))
                .then_with(|| distance(b).total_cmp(&distance(a)))
        });
    let Some(s) = best else { return frame };
    let width = width.min(s.width);
    let height = frame.height.min(s.height);
    Frame {
        x: frame.x.clamp(s.x, s.x + s.width - width),
        y: frame.y.clamp(s.y, s.y + s.height - height),
        width,
        height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_disconnected_negative_small_and_invalid_positions() {
        let screens = [
            Frame {
                x: 0.,
                y: 30.,
                width: 1440.,
                height: 850.,
            },
            Frame {
                x: -1280.,
                y: 0.,
                width: 1280.,
                height: 800.,
            },
        ];
        let f = constrain(
            Position {
                center_x: -700.,
                y: 9000.,
                width: 700.,
            },
            120.,
            &screens,
        );
        assert!(f.x >= -1280. && f.x + f.width <= 0.);
        assert!(f.y + f.height <= 800.);
        let f = constrain(
            Position {
                center_x: 9000.,
                y: -999.,
                width: 5000.,
            },
            90.,
            &screens,
        );
        assert_eq!(f.width, 900.);
        assert!(f.x + f.width <= 1440. && f.y >= 30.);
        let tiny = [Frame {
            x: 20.,
            y: 40.,
            width: 320.,
            height: 80.,
        }];
        let f = constrain(
            Position {
                center_x: f64::NAN,
                y: f64::INFINITY,
                width: f64::NAN,
            },
            90.,
            &tiny,
        );
        assert_eq!(f, tiny[0]);
        assert_eq!(Position::default().width, 600.);
    }
}
