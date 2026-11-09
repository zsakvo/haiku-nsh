use haiku_palette::{extract_file, extract_rgba, fallback_palette, Color};
use material_colors::{color::Rgb, hct::Hct};

fn image(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&pixel(x, y));
        }
    }
    data
}

fn hex(colors: &[Color]) -> Vec<&str> {
    colors.iter().map(|color| color.hex.as_str()).collect()
}

fn hct(color: &Color) -> Hct {
    Hct::new(Rgb::new(color.r, color.g, color.b))
}

fn assert_tonal_separation(colors: &[Color]) {
    assert_eq!(colors.len(), 5);
    for (color, expected) in colors.iter().zip([30.0, 50.0, 70.0, 85.0, 95.0]) {
        assert!(
            (hct(color).get_tone() - expected).abs() < 0.5,
            "{} should have tone {expected}",
            color.hex
        );
        assert_eq!(
            color.hex,
            format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
        );
    }
    for pair in colors.windows(2) {
        assert!(hct(&pair[1]).get_tone() - hct(&pair[0]).get_tone() > 9.0);
        assert_ne!(pair[0].hex, pair[1].hex);
    }
}

fn assert_source_hue(colors: &[Color], source: Rgb) {
    let expected = Hct::new(source).get_hue();
    // Compare the middle tone, where sRGB can retain the intended hue/chroma.
    let difference = (hct(&colors[1]).get_hue() - expected).abs();
    assert!(
        difference.min(360.0 - difference) < 2.0,
        "expected source hue {expected}, got {}",
        hct(&colors[1]).get_hue()
    );
}

#[test]
fn solid_artwork_preserves_hue_and_produces_separated_material_tones() {
    for rgb in [[255, 0, 0], [0, 190, 80], [0, 70, 255], [80, 120, 160]] {
        let pixels = image(100, 100, |_, _| [rgb[0], rgb[1], rgb[2], 255]);
        let colors = extract_rgba(100, 100, &pixels, None).unwrap();
        assert_tonal_separation(&colors);
        assert_source_hue(&colors, Rgb::new(rgb[0], rgb[1], rgb[2]));
        assert!(hct(&colors[1]).get_chroma() <= 49.0);
    }
}

#[test]
fn muted_source_gets_the_minimum_chroma() {
    let source = Rgb::new(160, 130, 130);
    assert!((5.0..24.0).contains(&Hct::new(source).get_chroma()));
    let pixels = image(20, 20, |_, _| [source.red, source.green, source.blue, 255]);
    let colors = extract_rgba(20, 20, &pixels, None).unwrap();
    assert_source_hue(&colors, source);
    assert!((hct(&colors[1]).get_chroma() - 24.0).abs() < 1.0);
}

#[test]
fn colorful_subject_is_selected_over_dominant_gray_background() {
    let pixels = image(100, 100, |x, _| {
        if x < 85 {
            [150, 150, 150, 255]
        } else {
            [0, 190, 80, 255]
        }
    });
    let colors = extract_rgba(100, 100, &pixels, None).unwrap();
    assert_source_hue(&colors, Rgb::new(0, 190, 80));
    assert_tonal_separation(&colors);
}

#[test]
fn scoring_accounts_for_color_population() {
    let pixels = image(100, 100, |x, _| {
        if x < 80 {
            [0, 70, 255, 255]
        } else {
            [255, 0, 0, 255]
        }
    });
    let colors = extract_rgba(100, 100, &pixels, None).unwrap();
    assert_source_hue(&colors, Rgb::new(0, 70, 255));
}

#[test]
fn default_samples_the_full_image_including_edges() {
    let pixels = image(100, 100, |x, y| {
        if (15..85).contains(&x) && (15..85).contains(&y) {
            [0, 0, 255, 255]
        } else {
            [255, 0, 0, 255]
        }
    });
    let colors = extract_rgba(100, 100, &pixels, None).unwrap();
    assert_source_hue(&colors, Rgb::new(255, 0, 0));
    let focused = extract_rgba(100, 100, &pixels, Some([0.5, 0.5])).unwrap();
    assert_source_hue(&focused, Rgb::new(0, 0, 255));
}

#[test]
fn focus_selects_twenty_percent_and_clamps_to_image_edges() {
    let pixels = image(100, 100, |x, _| {
        if x < 20 {
            [0, 190, 80, 255]
        } else {
            [255, 0, 0, 255]
        }
    });
    for focus in [[0.0, 0.0], [0.1, 0.5], [0.0, 1.0]] {
        let colors = extract_rgba(100, 100, &pixels, Some(focus)).unwrap();
        assert_source_hue(&colors, Rgb::new(0, 190, 80));
    }
    let colors = extract_rgba(100, 100, &pixels, Some([1.0, 1.0])).unwrap();
    assert_source_hue(&colors, Rgb::new(255, 0, 0));
    let tiny = extract_rgba(1, 1, &[255, 0, 0, 255], Some([1.0, 0.0])).unwrap();
    assert_source_hue(&tiny, Rgb::new(255, 0, 0));
}

#[test]
fn transparent_and_neutral_images_use_the_same_stable_fallback() {
    let expected = fallback_palette();
    assert_tonal_separation(&expected);
    for pixels in [
        image(100, 100, |x, y| [x as u8, y as u8, 255, 0]),
        image(100, 100, |x, _| {
            let gray = (x * 255 / 99) as u8;
            [gray, gray, gray, 255]
        }),
        image(100, 100, |_, _| [0, 0, 0, 255]),
        image(100, 100, |_, _| [255, 255, 255, 255]),
    ] {
        let colors = extract_rgba(100, 100, &pixels, None).unwrap();
        assert_eq!(hex(&colors), hex(&expected));
    }
}

#[test]
fn alpha_threshold_ignores_hidden_rgb_without_dimming_accepted_colors() {
    let pixels = image(100, 100, |x, _| {
        if x < 90 {
            [255, 0, 0, 124]
        } else {
            [0, 190, 80, 125]
        }
    });
    let colors = extract_rgba(100, 100, &pixels, None).unwrap();
    let opaque = extract_rgba(1, 1, &[0, 190, 80, 255], None).unwrap();
    assert_eq!(hex(&colors), hex(&opaque));
    let transparent = image(10, 10, |_, _| [255, 0, 0, 124]);
    assert_eq!(
        hex(&extract_rgba(10, 10, &transparent, None).unwrap()),
        hex(&fallback_palette())
    );
}

#[test]
fn large_striped_artwork_does_not_alias_to_every_fourth_column() {
    let pixels = image(1024, 512, |x, _| {
        if x % 4 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 70, 255, 255]
        }
    });
    let colors = extract_rgba(1024, 512, &pixels, None).unwrap();
    assert_source_hue(&colors, Rgb::new(0, 70, 255));
    assert_eq!(
        hex(&colors),
        hex(&extract_rgba(1024, 512, &pixels, None).unwrap())
    );
}

#[test]
fn rejects_invalid_dimensions_and_focus() {
    for (width, height, pixels) in [
        (0, 0, &[][..]),
        (1, 1, &[0][..]),
        (u32::MAX, u32::MAX, &[][..]),
    ] {
        assert!(extract_rgba(width, height, pixels, None).is_err());
    }
    for focus in [
        [f64::NAN, 0.0],
        [0.0, f64::INFINITY],
        [-0.1, 0.0],
        [0.5, 1.1],
    ] {
        assert!(extract_rgba(1, 1, &[0; 4], Some(focus)).is_err());
    }
}

#[test]
fn decodes_real_files_with_the_same_palette_as_rgba() {
    let path = std::env::temp_dir().join(format!("haiku-palette-{}.png", std::process::id()));
    let pixels = image::RgbaImage::from_pixel(100, 100, image::Rgba([80, 120, 160, 255]));
    pixels.save(&path).unwrap();
    let colors = extract_file(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let expected = extract_rgba(100, 100, pixels.as_raw(), None).unwrap();
    assert_eq!(hex(&colors), hex(&expected));
    assert!(extract_file("/nonexistent/haiku-cover.png").is_err());
}
