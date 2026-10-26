//! Portable view and print preference serialization.
use crate::print::{ImageScale, PageSettings};
use std::path::Path;

pub const MIN_VIEW_ZOOM: f32 = 0.5;
pub const MAX_VIEW_ZOOM: f32 = 4.0;
pub const DEFAULT_VIEW_ZOOM: f32 = 1.0;

pub fn sanitize_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(MIN_VIEW_ZOOM, MAX_VIEW_ZOOM)
    } else {
        DEFAULT_VIEW_ZOOM
    }
}

pub fn parse(contents: &str) -> (PageSettings, f32) {
    let mut settings = PageSettings::default();
    let mut zoom = DEFAULT_VIEW_ZOOM;
    for line in contents.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "landscape" => settings.landscape = value.trim() == "true",
                "margin_mm" => {
                    if let Ok(value) = value.parse::<f32>() {
                        settings.margin_mm = value;
                    }
                }
                "image_scale" => {
                    settings.image_scale = match value.trim() {
                        "original" => ImageScale::OriginalSize,
                        "fit" => ImageScale::FitToWidth,
                        _ => ImageScale::CustomPercent,
                    }
                }
                "page_numbers" => settings.print_page_numbers = value.trim() == "true",
                "zoom" => {
                    if let Ok(value) = value.parse::<f32>() {
                        zoom = value;
                    }
                }
                _ => {}
            }
        }
    }
    (settings.sanitized(), sanitize_zoom(zoom))
}

pub fn load(path: &Path) -> (PageSettings, f32) {
    std::fs::read_to_string(path)
        .map(|contents| parse(&contents))
        .unwrap_or_else(|_| (PageSettings::default(), DEFAULT_VIEW_ZOOM))
}

pub fn save(path: &Path, settings: PageSettings, zoom: f32) -> std::io::Result<()> {
    let settings = settings.sanitized();
    let scale = match settings.image_scale {
        ImageScale::OriginalSize => "original",
        ImageScale::FitToWidth => "fit",
        ImageScale::CustomPercent => "percent",
    };
    std::fs::write(
        path,
        format!(
            "landscape={}\nmargin_mm={:.2}\nimage_scale={}\npage_numbers={}\nzoom={:.3}\n",
            settings.landscape,
            settings.margin_mm,
            scale,
            settings.print_page_numbers,
            sanitize_zoom(zoom)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_existing_settings_format_and_ignores_unknown_keys() {
        let (settings, zoom) = parse(
            "landscape=true\nmargin_mm=20\nimage_scale=fit\npage_numbers=true\nzoom=2.5\nfuture_key=value\n",
        );

        assert!(settings.landscape);
        assert_eq!(settings.margin_mm, 20.0);
        assert_eq!(settings.image_scale, crate::print::ImageScale::FitToWidth);
        assert!(settings.print_page_numbers);
        assert_eq!(zoom, 2.5);
    }

    #[test]
    fn invalid_values_fall_back_to_sanitized_defaults() {
        let (settings, zoom) = parse("margin_mm=not-a-number\nzoom=not-a-number\n");

        assert_eq!(
            settings.margin_mm,
            crate::print::PageSettings::default().margin_mm
        );
        assert_eq!(zoom, 1.0);
    }
}
