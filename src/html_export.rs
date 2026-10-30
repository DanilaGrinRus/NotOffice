//! Small, dependency-free HTML export for DOCX text/tables and spreadsheet sheets.
use crate::docx::DocumentModel;
use crate::xlsx::SpreadsheetModel;

pub fn document(title: &str, model: &DocumentModel) -> String {
    let mut body = String::new();
    for block in &model.blocks {
        match block {
            crate::docx::DocumentBlock::Paragraph(index) => {
                if let Some(paragraph) = model.paragraphs.get(*index) {
                    let text = paragraph
                        .fragments
                        .iter()
                        .map(|fragment| fragment.text.as_str())
                        .collect::<String>();
                    if !text.trim().is_empty() {
                        body.push_str("<p>");
                        body.push_str(&escape(&text));
                        body.push_str("</p>\n");
                    }
                }
            }
            crate::docx::DocumentBlock::Table { rows, .. } => {
                body.push_str("<table>\n");
                for row in rows {
                    body.push_str("<tr>");
                    for cell in &row.cells {
                        body.push_str("<td>");
                        let text = cell
                            .iter()
                            .filter_map(|index| model.paragraphs.get(*index))
                            .map(|paragraph| {
                                paragraph
                                    .fragments
                                    .iter()
                                    .map(|fragment| fragment.text.as_str())
                                    .collect::<String>()
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        body.push_str(&escape(&text));
                        body.push_str("</td>");
                    }
                    body.push_str("</tr>\n");
                }
                body.push_str("</table>\n");
            }
        }
    }
    if body.is_empty() {
        for paragraph in &model.paragraphs {
            let text = paragraph
                .fragments
                .iter()
                .map(|fragment| fragment.text.as_str())
                .collect::<String>();
            if !text.trim().is_empty() {
                body.push_str("<p>");
                body.push_str(&escape(&text));
                body.push_str("</p>\n");
            }
        }
    }
    page(title, &body)
}

pub fn spreadsheet(title: &str, sheet: &SpreadsheetModel) -> String {
    let max_row = sheet
        .cells
        .iter()
        .map(|cell| cell.row + 1)
        .max()
        .unwrap_or(0);
    let max_column = sheet
        .cells
        .iter()
        .map(|cell| cell.column + 1)
        .max()
        .unwrap_or(0);
    let mut body = String::from("<table>\n");
    for row in 0..max_row {
        body.push_str("<tr>");
        for column in 0..max_column {
            let value = sheet.cell_value(row, column);
            body.push_str("<td>");
            body.push_str(&escape(value));
            body.push_str("</td>");
        }
        body.push_str("</tr>\n");
    }
    body.push_str("</table>\n");
    page(title, &body)
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"ru\"><head><meta charset=\"utf-8\"><title>{}</title>\
<style>body{{font-family:Segoe UI,Arial,sans-serif;max-width:900px;margin:2em auto;line-height:1.45;color:#1d2522}}\
table{{border-collapse:collapse;margin:1em 0}}td,th{{border:1px solid #b9c4bf;padding:.35em .6em;vertical-align:top}}</style>\
</head><body>\n{}</body></html>\n",
        escape(title),
        body
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            character => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::{DocumentBlock, Paragraph, TextFragment};
    use crate::xlsx::{SpreadsheetCell, SpreadsheetModel};

    #[test]
    fn document_html_escapes_text_and_exports_tables() {
        let model = DocumentModel {
            paragraphs: vec![
                Paragraph {
                    fragments: vec![TextFragment {
                        id: 0,
                        text: "Ёжик & <script>".into(),
                        run_id: 0,
                        font_size_half_points: None,
                        format: Default::default(),
                    }],
                    ..Default::default()
                },
                Paragraph {
                    fragments: vec![TextFragment {
                        id: 1,
                        text: "Ячейка".into(),
                        run_id: 0,
                        font_size_half_points: None,
                        format: Default::default(),
                    }],
                    ..Default::default()
                },
            ],
            blocks: vec![
                DocumentBlock::Paragraph(0),
                DocumentBlock::Table {
                    rows: vec![crate::docx::TableRow {
                        cells: vec![vec![1]],
                    }],
                    grid_widths: vec![],
                },
            ],
        };
        let html = document("T & T", &model);
        assert!(html.contains("<title>T &amp; T</title>"));
        assert!(html.contains("Ёжик &amp; &lt;script&gt;"));
        assert!(html.contains("<td>Ячейка</td>"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn spreadsheet_html_exports_visible_grid() {
        let sheet = SpreadsheetModel {
            sheet_name: "Data".into(),
            cells: vec![
                SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "A&B".into(),
                },
                SpreadsheetCell {
                    row: 1,
                    column: 1,
                    value: "2".into(),
                },
            ],
            row_count: 2,
            column_count: 2,
        };
        let html = spreadsheet("Data", &sheet);
        assert!(html.contains("<td>A&amp;B</td>"));
        assert!(html.contains("<td>2</td>"));
    }
}
