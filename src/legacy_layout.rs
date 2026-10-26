//! Shared, approximate page plan for recovered Word 97–2003 documents.
//!
//! The DOC parser cannot reconstruct Word's original layout.  This module is
//! deliberately limited to the display and print order NotOffice controls:
//! recovered paragraphs, manually anchored raster images, and unplaced images.
//! Both the preview and the print manifest consume this same plan so an
//! approximate page boundary is never calculated independently in each path.
use liteoffice::print::PageSettings;

const PIXELS_PER_MM: f32 = 96.0 / 25.4;
const LEGACY_FONT_LINE_HEIGHT_PX: f32 = 18.0;
const LEGACY_PARAGRAPH_GAP_PX: f32 = 4.0;
const LEGACY_IMAGE_GAP_PX: f32 = 8.0;
const UNPLACED_HEADING_HEIGHT_PX: f32 = 24.0;
const APPROXIMATE_GLYPH_WIDTH_PX: f32 = 7.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterInfo {
    pub width_px: u32,
    pub height_px: u32,
    pub dpi_x: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Paragraph(usize),
    Image(usize),
    UnplacedHeading,
}

fn paragraph_height(text: &str, content_width_px: f32) -> f32 {
    let characters_per_line = (content_width_px / APPROXIMATE_GLYPH_WIDTH_PX)
        .floor()
        .max(1.0) as usize;
    let line_count = text
        .split('\n')
        .map(|line| line.chars().count().max(1).div_ceil(characters_per_line))
        .sum::<usize>()
        .max(1);
    line_count as f32 * LEGACY_FONT_LINE_HEIGHT_PX + LEGACY_PARAGRAPH_GAP_PX
}

fn image_height(raster: RasterInfo, width_percent: f32, content_width_px: f32) -> f32 {
    if raster.width_px == 0 || raster.height_px == 0 {
        return 0.0;
    }
    let requested_width = content_width_px * width_percent.clamp(15.0, 100.0) / 100.0;
    // `draw_legacy_picture` uses the same two-times-intrinsic upper bound.
    let scale = (requested_width / raster.width_px as f32).min(2.0);
    raster.height_px as f32 * scale + LEGACY_IMAGE_GAP_PX
}

fn ordered_items(paragraphs: &[String], anchors: &[Option<usize>]) -> Vec<Item> {
    let mut items = Vec::with_capacity(paragraphs.len() + anchors.len() + 1);
    for paragraph_index in 0..paragraphs.len() {
        items.push(Item::Paragraph(paragraph_index));
        items.extend(
            anchors
                .iter()
                .enumerate()
                .filter_map(|(image_index, anchor)| {
                    (*anchor == Some(paragraph_index)).then_some(Item::Image(image_index))
                }),
        );
    }

    let has_unplaced_images = anchors
        .iter()
        .any(|anchor| anchor.is_none() || anchor.is_some_and(|index| index >= paragraphs.len()));
    if has_unplaced_images {
        items.push(Item::UnplacedHeading);
        items.extend(
            anchors
                .iter()
                .enumerate()
                .filter_map(|(image_index, anchor)| {
                    (anchor.is_none() || anchor.is_some_and(|index| index >= paragraphs.len()))
                        .then_some(Item::Image(image_index))
                }),
        );
    }
    items
}

/// Produces the single approximate DOC page plan used by screen preview and
/// Windows print preview.  Missing anchor/width/raster entries remain safe:
/// they become unplaced, use the default width, or reserve no image height.
pub fn paginate(
    paragraphs: &[String],
    anchors: &[Option<usize>],
    widths: &[f32],
    rasters: &[RasterInfo],
    settings: PageSettings,
) -> Vec<Vec<Item>> {
    let settings = settings.sanitized();
    let content_width_px = (settings.printable_width_mm() * PIXELS_PER_MM).max(100.0);
    let content_height_px =
        ((settings.page_size_mm().1 - 2.0 * settings.margin_mm) * PIXELS_PER_MM).max(20.0);
    let items = ordered_items(paragraphs, anchors);
    if items.is_empty() {
        return vec![Vec::new()];
    }

    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut used_height = 0.0;
    for item in items {
        let item_height = match item {
            Item::Paragraph(index) => paragraphs
                .get(index)
                .map(|text| paragraph_height(text, content_width_px))
                .unwrap_or_default(),
            Item::Image(index) => image_height(
                rasters.get(index).copied().unwrap_or(RasterInfo {
                    width_px: 0,
                    height_px: 0,
                    dpi_x: 96.0,
                }),
                widths.get(index).copied().unwrap_or(85.0),
                content_width_px,
            ),
            Item::UnplacedHeading => UNPLACED_HEADING_HEIGHT_PX,
        };
        if !page.is_empty() && used_height + item_height > content_height_px {
            pages.push(page);
            page = Vec::new();
            used_height = 0.0;
        }
        page.push(item);
        used_height += item_height;
    }
    pages.push(page);
    pages
}

#[cfg(test)]
mod tests {
    use super::{paginate, Item, RasterInfo};
    use crate::print::PageSettings;

    fn raster(width_px: u32, height_px: u32) -> RasterInfo {
        RasterInfo {
            width_px,
            height_px,
            dpi_x: 96.0,
        }
    }

    #[test]
    fn keeps_paragraph_and_image_order_and_appends_unplaced_images() {
        let pages = paginate(
            &["First".into(), "Second".into()],
            &[Some(0), None, Some(1)],
            &[50.0, 75.0, 100.0],
            &[raster(100, 50), raster(100, 50), raster(100, 50)],
            PageSettings::default(),
        );

        assert_eq!(
            pages.concat(),
            vec![
                Item::Paragraph(0),
                Item::Image(0),
                Item::Paragraph(1),
                Item::Image(2),
                Item::UnplacedHeading,
                Item::Image(1),
            ]
        );
    }

    #[test]
    fn starts_a_new_page_without_dropping_an_oversized_image() {
        let pages = paginate(
            &["Before".into()],
            &[Some(0)],
            &[100.0],
            &[raster(100, 10_000)],
            PageSettings::default(),
        );

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0], vec![Item::Paragraph(0)]);
        assert_eq!(pages[1], vec![Item::Image(0)]);
    }

    #[test]
    fn invalid_anchor_is_treated_as_unplaced() {
        let pages = paginate(
            &["Only paragraph".into()],
            &[Some(3)],
            &[85.0],
            &[raster(100, 50)],
            PageSettings::default(),
        );

        assert_eq!(
            pages.concat(),
            vec![Item::Paragraph(0), Item::UnplacedHeading, Item::Image(0),]
        );
    }
}
