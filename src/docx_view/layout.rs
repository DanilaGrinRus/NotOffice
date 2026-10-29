use crate::{print, FRAGMENT_HEIGHT, FRAGMENT_MIN_WIDTH, PARAGRAPH_GAP};
use liteoffice::docx::{DocumentBlock, DocumentModel, DocxImage, Paragraph, TableRow};
use std::ops::Range;

pub(crate) fn estimated_fragment_width(
    text: &str,
    content_width: f32,
    zoom: f32,
    size_pt: f32,
) -> f32 {
    let longest = text
        .split('\n')
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(3);
    (longest as f32 * size_pt * 0.72 * zoom + 12.0)
        .max(FRAGMENT_MIN_WIDTH)
        .min(content_width)
}

pub(crate) fn fragment_point_size(fragment: &liteoffice::docx::TextFragment) -> f32 {
    fragment
        .font_size_half_points
        .map(|size| size as f32 / 2.0)
        .unwrap_or(12.0)
}

pub(crate) fn fragment_render_height(fragment: &liteoffice::docx::TextFragment, zoom: f32) -> f32 {
    let lines = fragment.text.split('\n').count().max(1) as f32;
    (fragment_point_size(fragment) * (96.0 / 72.0) * 1.35).max(FRAGMENT_HEIGHT) * lines * zoom
}

pub(crate) fn estimated_paragraph_height(
    paragraph: &Paragraph,
    content_width: f32,
    zoom: f32,
) -> f32 {
    let mut total_height = 0.0;
    let mut row_height = FRAGMENT_HEIGHT * zoom;
    let mut used_width = 0.0;
    for fragment in &paragraph.fragments {
        let width = estimated_fragment_width(
            &fragment.text,
            content_width,
            zoom,
            fragment_point_size(fragment),
        );
        if used_width > 0.0 && used_width + width > content_width {
            total_height += row_height;
            row_height = FRAGMENT_HEIGHT * zoom;
            used_width = 0.0;
        }
        row_height = row_height.max(fragment_render_height(fragment, zoom));
        used_width += width;
    }
    total_height + row_height
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlowBlock {
    Paragraph(usize),
    Image {
        index: usize,
        size_pt: [f32; 2],
        page_break_after: bool,
    },
    TableRow {
        row: TableRow,
        grid_widths: Vec<u32>,
        column_count: usize,
    },
}

pub(crate) fn document_flow(model: &DocumentModel) -> Vec<FlowBlock> {
    let mut flow = Vec::new();
    for block in &model.blocks {
        match block {
            DocumentBlock::Paragraph(index) => flow.push(FlowBlock::Paragraph(*index)),
            DocumentBlock::Table { rows, grid_widths } => {
                let column_count = rows.iter().map(|row| row.cells.len()).max().unwrap_or(1);
                for row in rows {
                    flow.push(FlowBlock::TableRow {
                        row: row.clone(),
                        grid_widths: grid_widths.clone(),
                        column_count,
                    });
                }
            }
        }
    }
    flow
}

pub(crate) fn docx_image_size(image: &DocxImage, settings: print::PageSettings) -> [f32; 2] {
    let settings = settings.sanitized();
    let (native_width, native_height) = image
        .extent_emu
        .map(|(width, height)| (width as f32 / 12700.0, height as f32 / 12700.0))
        .or_else(|| {
            image::ImageReader::new(std::io::Cursor::new(&image.bytes))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()
                .map(|(width, height)| (width as f32 * 0.75, height as f32 * 0.75))
        })
        .unwrap_or((72.0, 72.0));
    let content_width = settings.printable_width_mm() * 72.0 / 25.4;
    // The GDI renderer reserves 30 printer units for the footer and 12 for
    // image spacing; keep the same physical limits in the screen page plan.
    let footer = if settings.print_page_numbers {
        21.6
    } else {
        0.0
    };
    let max_height = (settings.printable_height_mm() * 72.0 / 25.4 - footer - 8.64).max(1.0);
    let width = match settings.image_scale {
        print::ImageScale::OriginalSize => native_width.min(content_width),
        print::ImageScale::FitToWidth | print::ImageScale::CustomPercent => content_width,
    };
    let height = width * native_height / native_width.max(1.0);
    let scale = (max_height / height.max(1.0)).min(1.0);
    [width * scale, height * scale]
}

pub(crate) fn document_flow_with_images(
    model: &DocumentModel,
    images: &[DocxImage],
    settings: print::PageSettings,
) -> Vec<FlowBlock> {
    let mut flow = Vec::new();
    for block in document_flow(model) {
        let indexes = images
            .iter()
            .enumerate()
            .filter_map(|(index, image)| {
                let matches = match &block {
                    FlowBlock::Paragraph(paragraph) => *paragraph == image.paragraph_index,
                    FlowBlock::TableRow { row, .. } => row
                        .cells
                        .iter()
                        .any(|cell| cell.contains(&image.paragraph_index)),
                    FlowBlock::Image { .. } => false,
                };
                matches.then_some(index)
            })
            .collect::<Vec<_>>();
        let break_after = matches!(&block, FlowBlock::Paragraph(index) if model.paragraphs[*index].page_break_after);
        flow.push(block);
        for (position, index) in indexes.iter().copied().enumerate() {
            flow.push(FlowBlock::Image {
                index,
                size_pt: docx_image_size(&images[index], settings),
                page_break_after: break_after && position + 1 == indexes.len(),
            });
        }
    }
    flow
}

pub(crate) fn table_column_widths(
    content_width: f32,
    grid_widths: &[u32],
    column_count: usize,
    item_spacing: f32,
) -> Vec<f32> {
    let gaps = item_spacing * column_count.saturating_sub(1) as f32;
    let available_width = (content_width - gaps).max(FRAGMENT_MIN_WIDTH * column_count as f32);
    let weights: Vec<f32> = (0..column_count)
        .map(|index| grid_widths.get(index).copied().unwrap_or(1) as f32)
        .collect();
    let total_weight = weights.iter().sum::<f32>().max(1.0);
    weights
        .into_iter()
        .map(|weight| (available_width * weight / total_weight).max(FRAGMENT_MIN_WIDTH))
        .collect()
}

pub(crate) fn estimated_table_row_height(
    row: &TableRow,
    model: &DocumentModel,
    column_widths: &[f32],
    zoom: f32,
) -> f32 {
    row.cells
        .iter()
        .enumerate()
        .map(|(cell_index, cell)| {
            let cell_width = column_widths
                .get(cell_index)
                .copied()
                .unwrap_or(FRAGMENT_MIN_WIDTH);
            cell.iter()
                .enumerate()
                .fold(0.0, |height, (index, paragraph_index)| {
                    height
                        + estimated_paragraph_height(
                            &model.paragraphs[*paragraph_index],
                            cell_width,
                            zoom,
                        )
                        + if index == 0 {
                            0.0
                        } else {
                            PARAGRAPH_GAP * zoom
                        }
                })
        })
        .fold(FRAGMENT_HEIGHT * zoom, f32::max)
        + 8.0 * zoom
}

pub(crate) fn estimated_block_height(
    block: &FlowBlock,
    model: &DocumentModel,
    content_width: f32,
    zoom: f32,
) -> f32 {
    match block {
        FlowBlock::Image { size_pt, .. } => size_pt[1] * zoom,
        FlowBlock::Paragraph(index) => {
            estimated_paragraph_height(&model.paragraphs[*index], content_width, zoom)
        }
        FlowBlock::TableRow {
            row,
            grid_widths,
            column_count,
        } => estimated_table_row_height(
            row,
            model,
            &table_column_widths(content_width, grid_widths, *column_count, 8.0 * zoom),
            zoom,
        ),
    }
}

pub(crate) fn paginate_blocks(
    blocks: &[FlowBlock],
    model: &DocumentModel,
    content_width: f32,
    content_height: f32,
    zoom: f32,
) -> Vec<Range<usize>> {
    if blocks.is_empty() {
        return std::iter::once(0..0).collect();
    }

    let mut pages = Vec::new();
    let mut page_start = 0;
    let mut used_height = 0.0;
    for (index, block) in blocks.iter().enumerate() {
        let block_height = estimated_block_height(block, model, content_width, zoom);
        let required_height = if used_height == 0.0 {
            block_height
        } else {
            PARAGRAPH_GAP * zoom + block_height
        };
        if used_height > 0.0 && used_height + required_height > content_height {
            pages.push(page_start..index);
            page_start = index;
            used_height = block_height;
        } else {
            used_height += required_height;
        }
        let break_after = match block {
            FlowBlock::Paragraph(paragraph_index) => {
                model.paragraphs[*paragraph_index].page_break_after
                    && !matches!(blocks.get(index + 1), Some(FlowBlock::Image { .. }))
            }
            FlowBlock::Image {
                page_break_after, ..
            } => *page_break_after,
            FlowBlock::TableRow { .. } => false,
        };
        if index + 1 < blocks.len() && break_after {
            pages.push(page_start..index + 1);
            page_start = index + 1;
            used_height = 0.0;
        }
    }
    pages.push(page_start..blocks.len());
    pages
}

// DOCX preview and print share these approximate page boundaries.  This keeps
// explicit w:br page breaks and page order intact when the print pipeline turns
// the model into its platform-neutral PrintBlock manifest.
#[cfg(test)]
pub(crate) fn docx_page_ranges(
    model: &DocumentModel,
    settings: print::PageSettings,
    zoom: f32,
) -> Vec<Range<usize>> {
    docx_preview_plan(model, &[], settings, zoom).1
}

// Keep the flow list and its ranges together. The UI must obtain this plan
// after toolbar actions: an Open action can replace the active DOCX during the
// same egui frame, and ranges for the previous package must never index the
// new package's flow list.
pub(crate) fn docx_preview_plan(
    model: &DocumentModel,
    images: &[DocxImage],
    settings: print::PageSettings,
    zoom: f32,
) -> (Vec<FlowBlock>, Vec<Range<usize>>) {
    let settings = settings.sanitized();
    let points_per_mm = 72.0 / 25.4;
    let content_width = settings.printable_width_mm() * points_per_mm * zoom;
    let footer = if settings.print_page_numbers {
        21.6
    } else {
        0.0
    };
    let content_height = (settings.printable_height_mm() * points_per_mm - footer) * zoom;
    let flow = document_flow_with_images(model, images, settings);
    let ranges = paginate_blocks(&flow, model, content_width, content_height, zoom);
    (flow, ranges)
}

// DOCX preview and print share these approximate page boundaries. This keeps
// explicit w:br page breaks, orientation and margins aligned with the manifest.
pub(crate) fn docx_print_blocks(
    model: &DocumentModel,
    images: &[liteoffice::docx::DocxImage],
    settings: print::PageSettings,
) -> Vec<print::PrintBlock> {
    let (flow, ranges) = docx_preview_plan(model, images, settings, 1.0);
    let mut blocks = Vec::new();
    for (page_index, range) in ranges.iter().enumerate() {
        if page_index > 0 {
            blocks.push(print::PrintBlock::PageBreak);
        }
        for block_index in range.clone() {
            let text = match &flow[block_index] {
                FlowBlock::Image { index, size_pt, .. } => {
                    blocks.push(print::PrintBlock::SizedImage {
                        bytes: images[*index].bytes.clone(),
                        width_pt: size_pt[0],
                        height_pt: size_pt[1],
                    });
                    continue;
                }
                FlowBlock::Paragraph(index) => model
                    .paragraphs
                    .get(*index)
                    .map(|paragraph| {
                        paragraph
                            .fragments
                            .iter()
                            .map(|fragment| fragment.text.as_str())
                            .collect::<String>()
                    })
                    .unwrap_or_default(),
                FlowBlock::TableRow { row, .. } => {
                    blocks.push(print::PrintBlock::TableRow(
                        row.cells
                            .iter()
                            .map(|cell| {
                                cell.iter()
                                    .filter_map(|index| model.paragraphs.get(*index))
                                    .map(|paragraph| {
                                        paragraph
                                            .fragments
                                            .iter()
                                            .map(|fragment| fragment.text.as_str())
                                            .collect::<String>()
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect::<Vec<_>>(),
                    ));
                    continue;
                }
            };
            blocks.push(print::PrintBlock::Text(text));
        }
    }
    blocks
}
