//! Cover-derived Material You tones using the Rust port of Google's Material Color Utilities.
use material_colors::{
    color::Rgb,
    hct::Hct,
    palette::TonalPalette,
    quantize::{Quantizer, QuantizerCelebi},
    score::Score,
};
use serde::Serialize;
use std::path::Path;

const SAMPLE_SIDE: u32 = 128;
const MIN_ALPHA: u8 = 125;
const TONES: [i32; 5] = [30, 50, 70, 85, 95];
const FALLBACK_SOURCE: Rgb = Rgb::new(115, 160, 177);

#[derive(Clone, Debug, Serialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub hex: String,
}
impl Color {
    fn rgb(rgb: Rgb) -> Self {
        Self {
            r: rgb.red,
            g: rgb.green,
            b: rgb.blue,
            hex: format!("#{:02x}{:02x}{:02x}", rgb.red, rgb.green, rgb.blue),
        }
    }
}

/// Straight-alpha RGBA color for cover-derived player presentation tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn from_hct(hue: f64, chroma: f64, tone: f64, alpha: f64) -> Self {
        let rgb = Rgb::from(Hct::from(hue, chroma.max(0.0), tone.clamp(0.0, 100.0)));
        Self {
            r: rgb.red,
            g: rgb.green,
            b: rgb.blue,
            a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}

/// Adaptive Now Playing color roles derived from a cover's visual dominant color.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CoverTheme {
    pub is_light: bool,
    pub background: RgbaColor,
    pub shade_top: RgbaColor,
    pub shade_bottom: RgbaColor,
    pub foreground: RgbaColor,
    pub muted: RgbaColor,
    pub surface: RgbaColor,
    pub lyric_active: RgbaColor,
    pub lyric_inactive: RgbaColor,
    pub progress_track: RgbaColor,
    pub progress_buffered: RgbaColor,
}

fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let linear = |c: u8| {
        let v = f64::from(c) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn composite_over(fg: RgbaColor, bg: RgbaColor) -> (u8, u8, u8) {
    let a = f64::from(fg.a) / 255.0;
    let mix = |f: u8, b: u8| (f64::from(f) * a + f64::from(b) * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
    (mix(fg.r, bg.r), mix(fg.g, bg.g), mix(fg.b, bg.b))
}

fn contrast_ratio(fg: RgbaColor, bg: RgbaColor) -> f64 {
    let (r, g, b) = composite_over(fg, bg);
    let l1 = relative_luminance(r, g, b);
    let l2 = relative_luminance(bg.r, bg.g, bg.b);
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn with_min_contrast(
    hue: f64,
    chroma: f64,
    tone: f64,
    base_alpha: f64,
    min_contrast: f64,
    bg: RgbaColor,
) -> RgbaColor {
    let mut alpha = base_alpha;
    while alpha < 0.96 {
        let candidate = RgbaColor::from_hct(hue, chroma, tone, alpha);
        if contrast_ratio(candidate, bg) >= min_contrast {
            return candidate;
        }
        alpha = (alpha + 0.04).min(0.96);
    }
    RgbaColor::from_hct(hue, chroma, tone, alpha)
}

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

/// Build an adaptive light/dark cover theme from a dominant HCT color and the cover's average tone.
fn build_cover_theme(hue: f64, chroma: f64, dominant_tone: f64, average_tone: f64) -> CoverTheme {
    let is_light = if dominant_tone >= 66.0 {
        true
    } else if dominant_tone < 46.0 {
        false
    } else {
        // In the mid-tone band, follow the cover's overall mood so light covers stay airy
        // and dark covers stay rich while guaranteeing strong foreground contrast.
        (dominant_tone * 0.6 + average_tone * 0.4) >= 56.0
    };

    let (bg_tone, bg_chroma) = if is_light {
        let tone = if dominant_tone >= 66.0 {
            dominant_tone.clamp(80.0, 94.0)
        } else {
            (dominant_tone + 18.0).clamp(78.0, 88.0)
        };
        (tone, chroma.min(34.0))
    } else {
        let tone = if dominant_tone < 46.0 {
            dominant_tone.clamp(14.0, 34.0)
        } else {
            (dominant_tone - 18.0).clamp(20.0, 34.0)
        };
        (tone, chroma.min(44.0))
    };

    let background = RgbaColor::from_hct(hue, bg_chroma, bg_tone, 1.0);
    // Keep typography and icons subtly tinted with the cover's hue; neutral covers stay grayscale.
    let text_chroma = if bg_chroma < 4.0 {
        0.0
    } else {
        (bg_chroma * 0.32).clamp(2.0, 14.0)
    };

    if is_light {
        let shade_top = RgbaColor::from_hct(hue, bg_chroma * 0.55, (bg_tone + 6.0).min(99.0), 0.22);
        let shade_bottom =
            RgbaColor::from_hct(hue, (bg_chroma * 1.05).min(38.0), (bg_tone - 12.0).max(64.0), 0.18);
        let foreground = RgbaColor::from_hct(hue, text_chroma, 14.0, 1.0);
        let muted = with_min_contrast(hue, text_chroma, 16.0, 0.62, 4.6, background);
        let surface = RgbaColor::from_hct(hue, text_chroma, 12.0, 0.08);
        let lyric_active = RgbaColor::from_hct(hue, text_chroma * 0.9, 10.0, 1.0);
        let lyric_inactive = with_min_contrast(hue, text_chroma, 14.0, 0.48, 3.2, background);
        let progress_track = RgbaColor::from_hct(hue, text_chroma, 12.0, 0.14);
        let progress_buffered = RgbaColor::from_hct(hue, text_chroma, 12.0, 0.26);
        CoverTheme {
            is_light,
            background,
            shade_top,
            shade_bottom,
            foreground,
            muted,
            surface,
            lyric_active,
            lyric_inactive,
            progress_track,
            progress_buffered,
        }
    } else {
        let shade_top =
            RgbaColor::from_hct(hue, (bg_chroma * 0.9).min(42.0), (bg_tone + 14.0).min(48.0), 0.14);
        let shade_bottom = RgbaColor::from_hct(hue, bg_chroma * 0.85, (bg_tone - 10.0).max(4.0), 0.28);
        let foreground = RgbaColor::from_hct(hue, text_chroma * 0.65, 95.0, 1.0);
        let muted = with_min_contrast(hue, text_chroma * 0.75, 92.0, 0.70, 4.6, background);
        let surface = RgbaColor::from_hct(hue, text_chroma * 0.5, 96.0, 0.12);
        let lyric_active = RgbaColor::from_hct(hue, text_chroma * 0.4, 98.0, 1.0);
        let lyric_inactive = with_min_contrast(hue, text_chroma * 0.7, 94.0, 0.56, 3.2, background);
        let progress_track = RgbaColor::from_hct(hue, text_chroma * 0.5, 96.0, 0.18);
        let progress_buffered = RgbaColor::from_hct(hue, text_chroma * 0.5, 96.0, 0.32);
        CoverTheme {
            is_light,
            background,
            shade_top,
            shade_bottom,
            foreground,
            muted,
            surface,
            lyric_active,
            lyric_inactive,
            progress_track,
            progress_buffered,
        }
    }
}

/// Derive a complete cover theme from a single representative RGB color.
pub fn cover_theme_from_rgb(r: u8, g: u8, b: u8) -> CoverTheme {
    let hct = Hct::new(Rgb::new(r, g, b));
    build_cover_theme(hct.get_hue(), hct.get_chroma(), hct.get_tone(), hct.get_tone())
}

/// Extract the visual dominant color and adaptive light/dark presentation theme from RGBA pixels.
pub fn extract_cover_theme_rgba(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<CoverTheme, String> {
    let expected_len = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|length| length.checked_mul(4));
    if width == 0 || height == 0 || expected_len != Some(pixels.len() as u64) {
        return Err("Invalid RGBA dimensions".into());
    }
    let sampled = sample_pixels(width, height, pixels, None);
    if sampled.is_empty() {
        return Ok(cover_theme_from_rgb(20, 22, 25));
    }
    let quantized = QuantizerCelebi::quantize(&sampled, 128);
    let mut entries: Vec<(Hct, u32)> = quantized
        .color_to_count
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(rgb, count)| (Hct::new(rgb), count))
        .collect();
    if entries.is_empty() {
        return Ok(cover_theme_from_rgb(20, 22, 25));
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    let total: f64 = entries.iter().map(|(_, count)| f64::from(*count)).sum();
    let average_tone: f64 = entries
        .iter()
        .map(|(hct, count)| hct.get_tone() * f64::from(*count))
        .sum::<f64>()
        / total;

    // Merge neighboring quantized bins so smooth gradients keep their true area weight.
    struct Cluster {
        seed_hue: f64,
        seed_chroma: f64,
        seed_tone: f64,
        weight: f64,
        hue_cos: f64,
        hue_sin: f64,
        chroma_sum: f64,
        tone_sum: f64,
    }
    let mut clusters: Vec<Cluster> = Vec::new();
    for (hct, count) in entries {
        let w = f64::from(count);
        let h = hct.get_hue();
        let c = hct.get_chroma();
        let t = hct.get_tone();
        let rad = h.to_radians();
        if let Some(cluster) = clusters.iter_mut().find(|existing| {
            let tone_close = (existing.seed_tone - t).abs() <= 14.0;
            let both_neutral = existing.seed_chroma < 8.0 && c < 8.0;
            tone_close && (both_neutral || hue_distance(existing.seed_hue, h) <= 18.0)
        }) {
            cluster.weight += w;
            cluster.hue_cos += rad.cos() * w * c.max(1.0);
            cluster.hue_sin += rad.sin() * w * c.max(1.0);
            cluster.chroma_sum += c * w;
            cluster.tone_sum += t * w;
        } else {
            clusters.push(Cluster {
                seed_hue: h,
                seed_chroma: c,
                seed_tone: t,
                weight: w,
                hue_cos: rad.cos() * w * c.max(1.0),
                hue_sin: rad.sin() * w * c.max(1.0),
                chroma_sum: c * w,
                tone_sum: t * w,
            });
        }
    }

    let winner = clusters
        .into_iter()
        .max_by(|a, b| {
            let score = |cluster: &Cluster| {
                let pop = cluster.weight / total;
                let chroma = cluster.chroma_sum / cluster.weight;
                let tone = cluster.tone_sum / cluster.weight;
                // Population leads so the background matches the cover's dominant area;
                // a gentle chroma preference breaks ties in favor of expressive artwork over borders.
                let chroma_boost = 1.0 + 0.65 * (chroma / 48.0).min(1.0);
                let extreme_penalty = if chroma < 6.0 && !(4.0..=98.0).contains(&tone) {
                    0.55
                } else {
                    1.0
                };
                pop * chroma_boost * extreme_penalty
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap();

    let hue = if winner.hue_sin.abs() < 1e-6 && winner.hue_cos.abs() < 1e-6 {
        winner.seed_hue
    } else {
        winner.hue_sin.atan2(winner.hue_cos).to_degrees().rem_euclid(360.0)
    };
    let chroma = winner.chroma_sum / winner.weight;
    let tone = winner.tone_sum / winner.weight;
    Ok(build_cover_theme(hue, chroma, tone, average_tone))
}

/// File decoding and adaptive cover theme extraction.
pub fn extract_cover_theme_file(path: impl AsRef<Path>) -> Result<CoverTheme, String> {
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    extract_cover_theme_rgba(image.width(), image.height(), image.as_raw())
}

fn tonal_palette(source: Rgb) -> Vec<Color> {
    let source = Hct::new(source);
    let palette = TonalPalette::of(source.get_hue(), source.get_chroma().clamp(24.0, 48.0));
    TONES.map(|tone| Color::rgb(palette.tone(tone))).to_vec()
}

/// Material tones from a stable blue-gray source, also used for neutral or transparent artwork.
pub fn fallback_palette() -> Vec<Color> {
    tonal_palette(FALLBACK_SOURCE)
}

/// Generate an exact HCT tone from a palette color, including player tones 15 and 38.
pub fn tone(source: &Color, tone: i32) -> Color {
    let hct = Hct::new(Rgb::new(source.r, source.g, source.b));
    Color::rgb(TonalPalette::of(hct.get_hue(), hct.get_chroma()).tone(tone.clamp(0, 100)))
}

/// Generate the five cover roles from an application accent when artwork is absent.
pub fn from_rgb(r: u8, g: u8, b: u8) -> Vec<Color> {
    tonal_palette(Rgb::new(r, g, b))
}

/// A request generation. Advancing invalidates every older asynchronous completion.
#[derive(Default)]
pub struct RequestVersion(u64);
impl RequestVersion {
    pub fn advance(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
    pub fn accepts(&self, version: u64) -> bool {
        self.0 == version
    }
}

// A fixed hash makes sampling repeatable while avoiding a regular sampling lattice
// that could repeatedly select the same column of striped artwork.
fn sample_hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn sample_pixels(width: u32, height: u32, pixels: &[u8], focus: Option<[f64; 2]>) -> Vec<Rgb> {
    let crop = focus.map_or([width, height], |_| {
        [width, height].map(|size| (f64::from(size) * 0.2).round().max(1.0) as u32)
    });
    let start = focus.map_or([0, 0], |point| {
        [0, 1].map(|axis| {
            (point[axis] * f64::from([width, height][axis]) - f64::from(crop[axis]) / 2.0)
                .round()
                .clamp(0.0, f64::from([width, height][axis] - crop[axis])) as u32
        })
    });
    let samples = crop.map(|size| size.min(SAMPLE_SIDE));
    let mut sampled = Vec::with_capacity((samples[0] * samples[1]) as usize);
    // Partition both dimensions into nonempty cells and pick one pixel per cell.
    // Small images are sampled in full; large images contribute at most 128² pixels.
    for y in 0..samples[1] {
        let top = (u64::from(y) * u64::from(crop[1]) / u64::from(samples[1])) as u32;
        let bottom = (u64::from(y + 1) * u64::from(crop[1]) / u64::from(samples[1])) as u32;
        for x in 0..samples[0] {
            let left = (u64::from(x) * u64::from(crop[0]) / u64::from(samples[0])) as u32;
            let right = (u64::from(x + 1) * u64::from(crop[0]) / u64::from(samples[0])) as u32;
            let index = y * samples[0] + x;
            let px = start[0] + left + sample_hash(index) % (right - left);
            let py = start[1] + top + sample_hash(index.wrapping_add(0x9e37_79b9)) % (bottom - top);
            let offset = ((u64::from(py) * u64::from(width) + u64::from(px)) * 4) as usize;
            if pixels[offset + 3] >= MIN_ALPHA {
                sampled.push(Rgb::new(
                    pixels[offset],
                    pixels[offset + 1],
                    pixels[offset + 2],
                ));
            }
        }
    }
    sampled
}

/// Extract five Material HCT tones (30, 50, 70, 85, 95), darkest first.
/// Pixels use straight-alpha RGBA; alpha below 125 is ignored.
/// The default samples the full image. A normalized focus selects a 20% crop.
pub fn extract_rgba(
    width: u32,
    height: u32,
    pixels: &[u8],
    focus: Option<[f64; 2]>,
) -> Result<Vec<Color>, String> {
    let expected_len = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|length| length.checked_mul(4));
    if width == 0 || height == 0 || expected_len != Some(pixels.len() as u64) {
        return Err("Invalid RGBA dimensions".into());
    }
    if focus.is_some_and(|f| f.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)) {
        return Err("Focus must be within 0..1".into());
    }
    let sampled = sample_pixels(width, height, pixels, focus);
    if sampled.is_empty() {
        return Ok(fallback_palette());
    }
    let result = QuantizerCelebi::quantize(&sampled, 256);
    let source = Score::score(
        &result.color_to_count,
        Some(1),
        Some(FALLBACK_SOURCE),
        Some(true),
    )[0];
    Ok(tonal_palette(source))
}

/// File decoding and extraction are independent of app/UI state.
pub fn extract_file(path: impl AsRef<Path>) -> Result<Vec<Color>, String> {
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    extract_rgba(image.width(), image.height(), image.as_raw(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_palette_results_never_replace_the_selected_cover() {
        let mut version = RequestVersion::default();
        let first = version.advance();
        let second = version.advance();
        assert!(!version.accepts(first));
        assert!(version.accepts(second));
        let cleared = version.advance();
        assert!(!version.accepts(second));
        assert!(version.accepts(cleared));
        let palette = fallback_palette();
        assert_ne!(tone(&palette[1], 15).hex, tone(&palette[1], 38).hex);
    }

    #[test]
    fn samples_are_bounded_and_cover_both_dimensions_without_stride_bias() {
        let width = 1024;
        let height = 512;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let offset = ((y * width + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&[
                    (x % 4) as u8,
                    (y % 4) as u8,
                    ((y / 128) * 4 + x / 256) as u8,
                    255,
                ]);
            }
        }
        let sampled = sample_pixels(width, height, &pixels, None);
        assert_eq!(sampled.len(), (SAMPLE_SIDE * SAMPLE_SIDE) as usize);
        for channel in 0..4 {
            assert!(sampled.iter().any(|rgb| rgb.red == channel));
            assert!(sampled.iter().any(|rgb| rgb.green == channel));
        }
        for region in 0..16 {
            assert!(sampled.iter().any(|rgb| rgb.blue == region));
        }
        assert_eq!(sampled, sample_pixels(width, height, &pixels, None));
    }

    #[test]
    fn samples_every_pixel_of_small_and_thin_images() {
        for (width, height) in [(1, 1), (1, 128), (128, 1), (17, 23)] {
            let pixels = vec![255; width as usize * height as usize * 4];
            assert_eq!(
                sample_pixels(width, height, &pixels, None).len(),
                (width * height) as usize
            );
        }
    }
}
