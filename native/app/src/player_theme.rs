//! Runtime cover colors are data; widgets consume only the semantic roles below.
use haiku_palette::{CoverTheme, RgbaColor};
use nsh_core::{Hsla, Rgba, prelude::*};

fn adaptive(c: RgbaColor) -> AdaptiveColor {
    AdaptiveColor::from(Hsla::from(Rgba {
        r: f32::from(c.r) / 255.0,
        g: f32::from(c.g) / 255.0,
        b: f32::from(c.b) / 255.0,
        a: f32::from(c.a) / 255.0,
    }))
}

pub(crate) fn apply(cover: Option<&CoverTheme>) {
    untracked(|| {
        let mut tokens = (*nsh_core::element::tokens()).clone();
        let fallback;
        let theme = match cover {
            Some(theme) => theme,
            None => {
                fallback = haiku_palette::cover_theme_from_rgb(20, 22, 25);
                &fallback
            }
        };
        tokens.set_color("now-playing-background", adaptive(theme.background));
        tokens.set_color("now-playing-shade-top", adaptive(theme.shade_top));
        tokens.set_color("now-playing-shade-bottom", adaptive(theme.shade_bottom));
        tokens.set_color("now-playing-foreground", adaptive(theme.foreground));
        tokens.set_color("now-playing-muted", adaptive(theme.muted));
        tokens.set_color("now-playing-surface", adaptive(theme.surface));
        tokens.set_color("lyric-active", adaptive(theme.lyric_active));
        tokens.set_color("lyric-inactive", adaptive(theme.lyric_inactive));
        tokens.set_color("now-playing-progress-track", adaptive(theme.progress_track));
        tokens.set_color(
            "now-playing-progress-buffered",
            adaptive(theme.progress_buffered),
        );
        set_tokens(tokens);
    });
}
