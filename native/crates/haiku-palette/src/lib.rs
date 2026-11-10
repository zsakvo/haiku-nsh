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
