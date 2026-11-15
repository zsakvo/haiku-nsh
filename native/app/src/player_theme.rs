//! Runtime cover colors are data; widgets consume only the semantic roles below.
use haiku_palette::Color;
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) fn apply(palette: &[Color]) {
    untracked(|| {
        let mut tokens = (*nsh_core::element::tokens()).clone();
        let background = palette
            .get(3)
            .or_else(|| palette.first())
            .map(|c| {
                let half = |v: u8| (v as f32 * 0.5).round() as u32;
                AdaptiveColor::from(rgb((half(c.r) << 16) | (half(c.g) << 8) | half(c.b)))
            })
            .unwrap_or_else(|| theme().color("now-playing-background-fallback").unwrap());
        tokens.set_color("now-playing-background", background);
        set_tokens(tokens);
    });
}
