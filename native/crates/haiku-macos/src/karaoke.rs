//! Whole-line CoreText rasterization; only Core Animation advances word clips.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Clock {
    pub position: f64,
    pub playing: bool,
    pub epoch: u64,
}

/// Divide a word spanning visual lines by its actual shaped fragment widths.
pub fn fragment_times(start: f64, end: f64, widths: &[f64]) -> Vec<(f64, f64)> {
    let total: f64 = widths
        .iter()
        .copied()
        .filter(|w| w.is_finite() && *w > 0.)
        .sum();
    if !start.is_finite() || !end.is_finite() || total <= 0. {
        return Vec::new();
    }
    let duration = (end - start).max(0.);
    let mut consumed = 0.;
    widths
        .iter()
        .filter(|w| w.is_finite() && **w > 0.)
        .map(|width| {
            let from = start + duration * consumed / total;
            consumed += width;
            (from, start + duration * consumed / total)
        })
        .collect()
}

/// Ordinary samples within 350ms drift leave the running CA timeline intact.
pub fn needs_resync(previous: &Clock, anchored_at: f64, next: &Clock, now: f64) -> bool {
    previous.playing != next.playing
        || previous.epoch != next.epoch
        || (next.position
            - (previous.position
                + if previous.playing {
                    now - anchored_at
                } else {
                    0.
                }))
        .abs()
            > 0.35
}

#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
pub use native::{available, register};
#[cfg(not(target_os = "macos"))]
pub fn available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_word_uses_visual_width_not_character_count() {
        assert_eq!(
            fragment_times(2., 6., &[30., 10.]),
            vec![(2., 5.), (5., 6.)]
        );
        assert_eq!(fragment_times(4., 3., &[8., 2.]), vec![(4., 4.), (4., 4.)]);
        assert_eq!(fragment_times(2., 6., &[0., f64::NAN, 10.]), vec![(2., 6.)]);
        assert!(fragment_times(0., 1., &[0., -1.]).is_empty());
    }
    #[test]
    fn timeline_changes_only_on_state_epoch_or_excess_drift() {
        let old = Clock {
            position: 2.,
            playing: true,
            epoch: 4,
        };
        let sample = Clock {
            position: 5.2,
            ..old.clone()
        };
        assert!(!needs_resync(&old, 10., &sample, 13.));
        assert!(needs_resync(
            &old,
            10.,
            &Clock {
                position: 5.351,
                ..old.clone()
            },
            13.
        ));
        assert!(needs_resync(
            &old,
            10.,
            &Clock {
                playing: false,
                ..sample.clone()
            },
            13.
        ));
        assert!(needs_resync(&old, 10., &Clock { epoch: 5, ..sample }, 13.));
        let paused = Clock::default();
        assert!(!needs_resync(
            &paused,
            0.,
            &Clock {
                position: 0.35,
                ..paused.clone()
            },
            20.
        ));
    }
}
