//! Portable view and print preference serialization.
use crate::print::{ImageScale, PageSettings};
use crate::theme::Theme;
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

pub fn parse(contents: &str) -> (PageSettings, f32, Theme) {
    let mut settings = PageSettings::default();
    let mut zoom = DEFAULT_VIEW_ZOOM;
    let mut theme = Theme::Light;
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
                "theme" => theme = Theme::from_slug(value.trim()),
                _ => {}
            }
        }
    }
    (settings.sanitized(), sanitize_zoom(zoom), theme)
}

pub fn load(path: &Path) -> (PageSettings, f32, Theme) {
    std::fs::read_to_string(path)
        .map(|contents| parse(&contents))
        .unwrap_or_else(|_| (PageSettings::default(), DEFAULT_VIEW_ZOOM, Theme::Light))
}

pub fn save(path: &Path, settings: PageSettings, zoom: f32, theme: Theme) -> std::io::Result<()> {
    let settings = settings.sanitized();
    let scale = match settings.image_scale {
        ImageScale::OriginalSize => "original",
        ImageScale::FitToWidth => "fit",
        ImageScale::CustomPercent => "percent",
    };
    std::fs::write(
        path,
        format!(
            "landscape={}\nmargin_mm={:.2}\nimage_scale={}\npage_numbers={}\nzoom={:.3}\ntheme={}\n",
            settings.landscape,
            settings.margin_mm,
            scale,
            settings.print_page_numbers,
            sanitize_zoom(zoom),
            theme.slug()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{load, parse, save};
    use crate::theme::Theme;

    #[test]
    fn reads_existing_settings_format_and_ignores_unknown_keys() {
        let (settings, zoom, theme) = parse(
            "landscape=true\nmargin_mm=20\nimage_scale=fit\npage_numbers=true\nzoom=2.5\nfuture_key=value\n",
        );

        assert!(settings.landscape);
        assert_eq!(settings.margin_mm, 20.0);
        assert_eq!(settings.image_scale, crate::print::ImageScale::FitToWidth);
        assert!(settings.print_page_numbers);
        assert_eq!(zoom, 2.5);
        assert_eq!(theme, Theme::Light);
    }

    #[test]
    fn every_theme_roundtrips_without_changing_other_settings() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        let settings = crate::print::PageSettings {
            landscape: true,
            margin_mm: 21.0,
            print_page_numbers: true,
            ..Default::default()
        };
        for theme in Theme::ALL {
            save(&path, settings, 1.75, theme).unwrap();
            let (actual_settings, zoom, actual_theme) = load(&path);
            assert_eq!(actual_settings, settings.sanitized());
            assert_eq!(zoom, 1.75);
            assert_eq!(actual_theme, theme);
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains("theme="));
            assert!(contents.contains("margin_mm=21.00"));
        }
    }

    #[test]
    fn damaged_or_unknown_theme_uses_light_and_keeps_other_values() {
        let (settings, zoom, theme) = parse(
            "landscape=true\nmargin_mm=18\nimage_scale=fit\npage_numbers=true\nzoom=1.4\ntheme=no-such-theme\n",
        );
        assert!(settings.landscape);
        assert_eq!(settings.margin_mm, 18.0);
        assert_eq!(zoom, 1.4);
        assert_eq!(theme, Theme::Light);

        let (_, _, missing) = parse("landscape=false\nzoom=1.0\n");
        assert_eq!(missing, Theme::Light);
    }

    #[test]
    fn invalid_values_fall_back_to_sanitized_defaults() {
        let (settings, zoom, theme) = parse("margin_mm=not-a-number\nzoom=not-a-number\n");

        assert_eq!(
            settings.margin_mm,
            crate::print::PageSettings::default().margin_mm
        );
        assert_eq!(zoom, 1.0);
        assert_eq!(theme, Theme::Light);
    }
}
