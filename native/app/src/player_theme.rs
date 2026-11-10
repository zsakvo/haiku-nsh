//! Runtime cover colors are data; widgets consume only the semantic roles below.
use haiku_palette::{Color, from_rgb, tone};
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) fn accent_palette() -> Vec<Color> {
    untracked(|| {
        let c = color(theme().primary).to_rgb();
        from_rgb((c.r * 255.) as u8, (c.g * 255.) as u8, (c.b * 255.) as u8)
    })
}
fn adaptive(c: &Color, scale: f32) -> AdaptiveColor {
    rgb((((c.r as f32 * scale) as u32) << 16)
        | (((c.g as f32 * scale) as u32) << 8)
        | (c.b as f32 * scale) as u32)
    .into()
}
pub(crate) fn apply(palette: &[Color]) {
    untracked(|| {
        let mut tokens = (*nsh_core::element::tokens()).clone();
        for (name, value) in [
            ("now-playing-background", adaptive(&palette[3], 0.5)),
            ("now-playing-foreground", rgb(0xffffff).into()),
            ("now-playing-muted", adaptive(&palette[4], 0.8)),
            ("now-playing-surface", adaptive(&palette[0], 0.6)),
            ("lyric-active", rgb(0xffffff).into()),
            ("lyric-inactive", adaptive(&palette[4], 0.65)),
            ("now-playing-shade-top", rgba(0x00000080).into()),
            ("now-playing-shade-bottom", rgba(0x000000b3).into()),
            (
                "progress-now-playing-played",
                adaptive(&tone(&palette[1], 15), 1.),
            ),
            (
                "progress-now-playing-buffered",
                adaptive(&tone(&palette[1], 38), 1.),
            ),
        ] {
            tokens.set_color(name, value);
        }
        set_tokens(tokens);
    });
}
