//! Transactional, literal DOCX replacement over the same ranges used by Find.
//! Only text is changed: paragraph/cell structure, fragment IDs and run styles
//! stay intact. Replacement text inherits the first overlapping run's style.
use crate::{docx::DocumentModel, search};

pub const MAX_TEXT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_REPLACEMENT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub enum Selection {
    Current(usize),
    All,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    EmptyQuery,
    TooManyMatches,
    InvalidReplacement,
    TooLarge,
    InvalidRange,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyQuery => "enter non-empty Find text first",
            Self::TooManyMatches => {
                "Replace all blocked: more than 10,000 matches; narrow the query"
            }
            Self::InvalidReplacement => {
                "replacement must be single-line text without control characters"
            }
            Self::TooLarge => "replacement exceeds the text size limit; document unchanged",
            Self::InvalidRange => "match is no longer valid; search again",
        })
    }
}

#[derive(Debug)]
pub struct Edit {
    pub model: DocumentModel,
    pub count: usize,
    /// Continue beyond the newly inserted text rather than replacing it again.
    pub resume_from: Option<(usize, usize)>,
}

/// Builds a complete candidate without mutating the caller. Never silently
/// replace only the first capped search page for an operation called "all".
pub fn docx(
    model: &DocumentModel,
    query: &str,
    replacement: &str,
    selection: Selection,
) -> Result<Edit, Error> {
    if query.trim().is_empty() {
        return Err(Error::EmptyQuery);
    }
    if replacement.len() > MAX_REPLACEMENT_BYTES {
        return Err(Error::TooLarge);
    }
    if replacement
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}'))
    {
        return Err(Error::InvalidReplacement);
    }
    let results = search::docx(model, query);
    if matches!(selection, Selection::All) && results.limited {
        return Err(Error::TooManyMatches);
    }
    let hits = match selection {
        Selection::All => results.hits.as_slice(),
        Selection::Current(index) => results
            .hits
            .get(index)
            .map(std::slice::from_ref)
            .unwrap_or(&[]),
    };
    let original_bytes = model
        .paragraphs
        .iter()
        .flat_map(|p| &p.fragments)
        .try_fold(0_usize, |size, fragment| {
            size.checked_add(fragment.text.len())
        })
        .ok_or(Error::TooLarge)?;
    let inserted = replacement
        .len()
        .checked_mul(hits.len())
        .ok_or(Error::TooLarge)?;
    let removed = hits.iter().map(|hit| hit.bytes.len()).sum::<usize>();
    let final_size = original_bytes
        .checked_sub(removed)
        .and_then(|size| size.checked_add(inserted))
        .ok_or(Error::InvalidRange)?;
    if final_size > MAX_TEXT_BYTES {
        return Err(Error::TooLarge);
    }
    let resume_from = if matches!(selection, Selection::Current(_)) {
        hits.first().and_then(|hit| match hit.target {
            search::Target::DocxParagraph(index) => {
                Some((index, hit.bytes.start + replacement.len()))
            }
            _ => None,
        })
    } else {
        None
    };
    let mut candidate = model.clone();
    let mut first_hit = 0;
    for (paragraph_index, paragraph) in candidate.paragraphs.iter_mut().enumerate() {
        let count = hits[first_hit..]
            .iter()
            .take_while(|hit| hit.target == search::Target::DocxParagraph(paragraph_index))
            .count();
        let paragraph_hits = &hits[first_hit..first_hit + count];
        first_hit += count;
        if paragraph_hits.is_empty() {
            continue;
        }
        let text = paragraph
            .fragments
            .iter()
            .map(|f| f.text.as_str())
            .collect::<String>();
        for (index, hit) in paragraph_hits.iter().enumerate() {
            if hit.bytes.is_empty()
                || text.get(hit.bytes.clone()).is_none()
                || (index > 0 && paragraph_hits[index - 1].bytes.end > hit.bytes.start)
            {
                return Err(Error::InvalidRange);
            }
        }
        let mut offset = 0;
        let mut hit_index = 0;
        for fragment in &mut paragraph.fragments {
            let start = offset;
            let end = start + fragment.text.len();
            offset = end;
            while hit_index < paragraph_hits.len() && paragraph_hits[hit_index].bytes.end <= start {
                hit_index += 1;
            }
            let mut scan = hit_index;
            let mut cursor = 0;
            let mut updated = String::new();
            while scan < paragraph_hits.len() && paragraph_hits[scan].bytes.start < end {
                let hit = &paragraph_hits[scan].bytes;
                let local_start = hit.start.saturating_sub(start);
                let local_end = hit.end.min(end) - start;
                updated.push_str(&fragment.text[cursor..local_start]);
                if hit.start >= start {
                    updated.push_str(replacement);
                }
                cursor = local_end;
                scan += 1;
            }
            updated.push_str(&fragment.text[cursor..]);
            fragment.text = updated;
        }
    }
    if first_hit != hits.len() {
        return Err(Error::InvalidRange);
    }
    Ok(Edit {
        model: candidate,
        count: hits.len(),
        resume_from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::{parse_document_xml, rewrite_document_xml};

    const XML: &[u8] = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:b/><w:sz w:val="24"/></w:rPr><w:t>ab</w:t></w:r><w:r><w:rPr><w:i/><w:sz w:val="32"/></w:rPr><w:t>cd abcd tail</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>ABCD</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;

    fn text(model: &DocumentModel) -> String {
        model
            .paragraphs
            .iter()
            .flat_map(|p| &p.fragments)
            .map(|f| f.text.as_str())
            .collect()
    }

    #[test]
    fn current_replaces_only_chosen_occurrence_and_all_crosses_runs_and_table_cells() {
        let model = parse_document_xml(XML).unwrap();
        let current = docx(&model, "abcd", "X", Selection::Current(1)).unwrap();
        assert_eq!(text(&current.model), "abcd X tailABCD");
        assert_eq!(current.resume_from, Some((0, 6)));
        let all = docx(&model, "abcd", "X", Selection::All).unwrap();
        assert_eq!(all.count, 3);
        assert_eq!(text(&all.model), "X X tailX");
        assert_eq!(all.model.blocks, model.blocks);
        for (before, after) in model
            .paragraphs
            .iter()
            .flat_map(|p| &p.fragments)
            .zip(all.model.paragraphs.iter().flat_map(|p| &p.fragments))
        {
            assert_eq!(
                (before.id, before.run_id, before.font_size_half_points),
                (after.id, after.run_id, after.font_size_half_points)
            );
        }
        let rewritten = rewrite_document_xml(XML, &all.model).unwrap();
        assert_eq!(parse_document_xml(&rewritten).unwrap(), all.model);
        let xml = String::from_utf8(rewritten).unwrap();
        assert!(xml.contains("<w:b/>") && xml.contains("<w:i/>"));
        assert_eq!(text(&model), "abcd abcd tailABCD");
    }

    #[test]
    fn empty_replacement_and_special_characters_roundtrip_without_losing_runs() {
        let model = parse_document_xml(XML).unwrap();
        for replacement in ["", " <&> ", "Ёжик 🦔"] {
            let edit = docx(&model, "abcd", replacement, Selection::All).unwrap();
            let rewritten = rewrite_document_xml(XML, &edit.model).unwrap();
            assert_eq!(parse_document_xml(&rewritten).unwrap(), edit.model);
            assert_eq!(edit.model.paragraphs[0].fragments[0].text, replacement);
        }
    }

    #[test]
    fn unicode_expansion_and_fragment_boundaries_remain_valid_utf8() {
        let xml = "<w:document><w:body><w:p><w:r><w:t>Ё</w:t></w:r><w:r><w:t>жик İİ ЁЖИК</w:t></w:r></w:p></w:body></w:document>";
        let model = parse_document_xml(xml.as_bytes()).unwrap();
        let edit = docx(&model, "ёжик", "Кот", Selection::All).unwrap();
        assert_eq!(text(&edit.model), "Кот İİ Кот");
        let edit = docx(&edit.model, "i", "X", Selection::All).unwrap();
        assert_eq!(text(&edit.model), "Кот XX Кот");
        assert_eq!(
            parse_document_xml(&rewrite_document_xml(xml.as_bytes(), &edit.model).unwrap())
                .unwrap(),
            edit.model
        );
    }

    #[test]
    fn identical_replacement_missing_match_and_stale_index_are_noops() {
        let model = parse_document_xml(XML).unwrap();
        for (query, replacement, selection, count) in [
            ("ab", "ab", Selection::Current(0), 1),
            ("missing", "x", Selection::All, 0),
            ("ab", "x", Selection::Current(99), 0),
        ] {
            let edit = docx(&model, query, replacement, selection).unwrap();
            assert_eq!(edit.count, count);
            assert_eq!(edit.model, model);
        }
        assert_eq!(
            docx(&model, "  ", "x", Selection::All).unwrap_err(),
            Error::EmptyQuery
        );
    }

    #[test]
    fn invalid_or_expansive_replacement_is_rejected_before_mutation() {
        let model = parse_document_xml(XML).unwrap();
        for replacement in ["\n", "\r", "\t", "\0", "\u{fffe}"] {
            assert_eq!(
                docx(&model, "ab", replacement, Selection::All).unwrap_err(),
                Error::InvalidReplacement
            );
        }
        assert_eq!(
            docx(
                &model,
                "ab",
                &"X".repeat(MAX_REPLACEMENT_BYTES + 1),
                Selection::All
            )
            .unwrap_err(),
            Error::TooLarge
        );
        let mut many = model.clone();
        many.paragraphs[0].fragments[0].text = "x ".repeat(40);
        assert_eq!(
            docx(
                &many,
                "x",
                &"z".repeat(MAX_REPLACEMENT_BYTES),
                Selection::All
            )
            .unwrap_err(),
            Error::TooLarge
        );
        assert_eq!(text(&model), "abcd abcd tailABCD");
    }

    #[test]
    fn capped_find_cannot_silently_become_partial_replace_all() {
        let mut model = parse_document_xml(XML).unwrap();
        model.paragraphs[0].fragments[0].text = "x ".repeat(search::MAX_HITS + 1);
        let original = model.clone();
        assert_eq!(
            docx(&model, "x", "y", Selection::All).unwrap_err(),
            Error::TooManyMatches
        );
        assert_eq!(model, original);
        let single = docx(&model, "x", "y", Selection::Current(0)).unwrap();
        assert_eq!(single.count, 1);
    }

    #[test]
    fn replacement_agrees_with_flat_ranges_at_every_utf8_run_split_including_empty_runs() {
        use crate::docx::{Paragraph, TextFragment};
        for original in ["abaaba", "Ёжикёжик", "İİx", "😀x😀", "Ⱥёa", ""] {
            let boundaries = original
                .char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(original.len()))
                .collect::<Vec<_>>();
            for first in &boundaries {
                for second in boundaries.iter().filter(|second| *second >= first) {
                    let model = DocumentModel {
                        paragraphs: vec![Paragraph {
                            fragments: [
                                &original[..*first],
                                &original[*first..*second],
                                &original[*second..],
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(id, text)| TextFragment {
                                id,
                                run_id: id,
                                text: text.into(),
                                font_size_half_points: Some(20 + id as u16),
                                format: Default::default(),
                            })
                            .collect(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    };
                    for query in ["a", "ёжик", "i", "😀", "x"] {
                        for replacement in ["", "a", " <&> "] {
                            let mut expected = original.to_owned();
                            for hit in search::ranges(original, query, search::MAX_HITS)
                                .into_iter()
                                .rev()
                            {
                                expected.replace_range(hit, replacement);
                            }
                            let edit = docx(&model, query, replacement, Selection::All).unwrap();
                            assert_eq!(
                                text(&edit.model),
                                expected,
                                "{original:?}/{query:?} at {first}/{second}"
                            );
                            assert_eq!(edit.model.paragraphs[0].fragments.len(), 3);
                        }
                    }
                }
            }
        }
    }
}
