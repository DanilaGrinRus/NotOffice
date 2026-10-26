//! Case-insensitive literal search of imported text. Never edits document data.
use crate::docx::DocumentModel;
use crate::xlsx::SpreadsheetModel;
use std::ops::Range;

pub const MAX_HITS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    DocxParagraph(usize),
    DocParagraph(usize),
    Cell {
        sheet: usize,
        row: usize,
        column: usize,
    },
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub target: Target,
    pub bytes: Range<usize>,
}

#[derive(Debug, Default)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub limited: bool,
}

// Most scripts, including Russian, need no index mapping. Record only scalar
// lowercasing changes that alter UTF-8 width or expand into multiple scalars.
// This avoids a per-byte mapping allocation for large ordinary paragraphs.
struct Change {
    folded: Range<usize>,
    original: Range<usize>,
}

fn original_offset(offset: usize, end: bool, changes: &[Change]) -> usize {
    let index = changes.partition_point(|change| change.folded.start <= offset);
    if index == 0 {
        return offset;
    }
    let change = &changes[index - 1];
    if offset >= change.folded.end {
        change.original.end + offset - change.folded.end
    } else if end && offset > change.folded.start {
        change.original.end
    } else {
        change.original.start
    }
}

/// Returned ranges are UTF-8 boundaries in ORIGINAL text, not its lowercase
/// copy. Unicode lowercasing is supported, not normalization/full case folding.
pub fn ranges(text: &str, query: &str, limit: usize) -> Vec<Range<usize>> {
    let query = query
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::with_capacity(text.len());
    let mut changes = Vec::new();
    for (index, character) in text.char_indices() {
        let start = folded.len();
        folded.extend(character.to_lowercase());
        if folded.len() - start != character.len_utf8() || character.to_lowercase().count() != 1 {
            changes.push(Change {
                folded: start..folded.len(),
                original: index..index + character.len_utf8(),
            });
        }
    }
    let mut result = Vec::new();
    for (start, _) in folded.match_indices(&query) {
        let bytes = original_offset(start, false, &changes)
            ..original_offset(start + query.len(), true, &changes);
        if result.last() != Some(&bytes) {
            if result.len() == limit {
                break;
            }
            result.push(bytes);
        }
    }
    result
}

impl Results {
    fn append(&mut self, target: Target, text: &str, query: &str) -> bool {
        let remaining = MAX_HITS - self.hits.len();
        let found = ranges(text, query, remaining + 1);
        self.limited = found.len() > remaining;
        self.hits
            .extend(found.into_iter().take(remaining).map(|bytes| Hit {
                target: target.clone(),
                bytes,
            }));
        !self.limited
    }
}

pub fn text(text: &str, query: &str) -> Results {
    let mut result = Results::default();
    if query.trim().is_empty() {
        return result;
    }
    result.append(Target::Text, text, query);
    result
}

pub fn docx(model: &DocumentModel, query: &str) -> Results {
    let mut result = Results::default();
    if query.trim().is_empty() {
        return result;
    }
    for (index, paragraph) in model.paragraphs.iter().enumerate() {
        let text = paragraph
            .fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();
        if !result.append(Target::DocxParagraph(index), &text, query) {
            break;
        }
    }
    result
}

pub fn legacy(paragraphs: &[String], query: &str) -> Results {
    let mut result = Results::default();
    if query.trim().is_empty() {
        return result;
    }
    for (index, text) in paragraphs.iter().enumerate() {
        if !result.append(Target::DocParagraph(index), text, query) {
            break;
        }
    }
    result
}

/// XLS and XLSX import into the same model; sheet order then numeric row/column
/// order is stable even when input XML stores cells in a different order.
pub fn sheets(sheets: &[SpreadsheetModel], query: &str) -> Results {
    let mut result = Results::default();
    if query.trim().is_empty() {
        return result;
    }
    for (sheet, model) in sheets.iter().enumerate() {
        let mut cells = model.cells.iter().collect::<Vec<_>>();
        cells.sort_by_key(|cell| (cell.row, cell.column));
        for cell in cells {
            if !result.append(
                Target::Cell {
                    sheet,
                    row: cell.row,
                    column: cell.column,
                },
                &cell.value,
                query,
            ) {
                return result;
            }
        }
    }
    result
}

pub fn next_index(current: usize, count: usize, backwards: bool) -> usize {
    if count == 0 {
        return 0;
    }
    let current = current.min(count - 1);
    if backwards {
        if current == 0 {
            count - 1
        } else {
            current - 1
        }
    } else if current + 1 == count {
        0
    } else {
        current + 1
    }
}

/// DOCX fragments overlapping an active paragraph hit (also across run splits).
pub fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::{Paragraph, TextFragment};
    use crate::xlsx::SpreadsheetCell;

    #[test]
    fn cyrillic_occurrences_have_original_utf8_ranges_and_no_empty_matches() {
        let text = "Ёжик ёЖИК — ёжик";
        let hits = ranges(text, "ЁЖИК", 20);
        assert_eq!(hits.len(), 3);
        for hit in hits {
            assert_eq!(text[hit].to_lowercase(), "ёжик");
        }
        assert!(ranges(text, "  ", 20).is_empty());
        assert!(ranges(text, "нет", 20).is_empty());
        assert!(ranges(text, "ё", 0).is_empty());
    }

    #[test]
    fn unicode_lowercase_expansion_and_width_changes_map_back_to_safe_boundaries() {
        assert_eq!(ranges("ΟΣ", "ΟΣ", 10), vec![0..4]);
        for (text, query, expected) in [
            ("İ-Ё-İ", "i", vec![0..2, 6..8]),
            ("ȺЁA", "ёa", std::iter::once(2..5).collect()),
            ("İİx", "x", std::iter::once(4..5).collect()),
        ] {
            let result = ranges(text, query, 10);
            assert_eq!(result, expected);
            for range in result {
                assert!(text.get(range).is_some());
            }
        }
    }

    #[test]
    fn docx_search_crosses_runs_and_counts_each_occurrence_without_mutating_model() {
        let model = DocumentModel {
            paragraphs: vec![Paragraph {
                fragments: ["Ки", "риллица Кириллица"]
                    .iter()
                    .enumerate()
                    .map(|(id, text)| TextFragment {
                        id,
                        run_id: id,
                        text: text.to_string(),
                        font_size_half_points: None,
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let original = model.clone();
        let hits = docx(&model, "кириллица");
        assert_eq!(hits.hits.len(), 2);
        assert_eq!(
            hits.hits[0],
            Hit {
                target: Target::DocxParagraph(0),
                bytes: 0..18
            }
        );
        assert_eq!(model, original);
        assert!(overlaps(&(0..4), &hits.hits[0].bytes));
        assert!(overlaps(&(4..40), &hits.hits[0].bytes));
        assert!(!overlaps(&(18..19), &hits.hits[0].bytes));
    }

    #[test]
    fn legacy_search_counts_occurrences_and_caps_large_results_explicitly() {
        let result = legacy(&["тест тест".into(), "ТЕСТ".into()], "тест");
        assert_eq!(result.hits.len(), 3);
        assert_eq!(result.hits[2].target, Target::DocParagraph(1));
        let result = legacy(&["x".repeat(MAX_HITS + 2)], "x");
        assert_eq!(result.hits.len(), MAX_HITS);
        assert!(result.limited);
    }

    #[test]
    fn spreadsheets_search_every_sheet_and_far_cells_in_coordinate_order() {
        let model = SpreadsheetModel {
            sheet_name: "Лист".into(),
            cells: vec![
                SpreadsheetCell {
                    row: 499,
                    column: 29,
                    value: "Ёжик ЁЖИК".into(),
                },
                SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "Ёжик".into(),
                },
            ],
            row_count: 500,
            column_count: 30,
        };
        let sheets = vec![model.clone(), model];
        let original = sheets.clone();
        let result = super::sheets(&sheets, "ёжик");
        assert_eq!(result.hits.len(), 6);
        assert_eq!(
            result.hits[0].target,
            Target::Cell {
                sheet: 0,
                row: 0,
                column: 0
            }
        );
        assert_eq!(
            result.hits[1].target,
            Target::Cell {
                sheet: 0,
                row: 499,
                column: 29
            }
        );
        assert_eq!(
            result.hits[3].target,
            Target::Cell {
                sheet: 1,
                row: 0,
                column: 0
            }
        );
        assert_eq!(sheets, original);
    }

    #[test]
    fn next_previous_wrap_and_handle_empty_or_stale_indices() {
        assert_eq!(next_index(0, 0, true), 0);
        assert_eq!(next_index(0, 3, true), 2);
        assert_eq!(next_index(2, 3, false), 0);
        assert_eq!(next_index(99, 2, true), 0);
        assert_eq!(next_index(99, 1, false), 0);
    }
}
