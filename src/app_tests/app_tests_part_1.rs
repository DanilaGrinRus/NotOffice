#[test]
fn two_window_conflict_can_cancel_then_save_a_copy_without_overwriting_first_window() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("shared.docx");
    let copy = folder.path().join("window-b.docx");
    write_docx_fixture(&path, "Initial");
    let mut a = NotOfficeApp::default();
    let mut b = NotOfficeApp::default();
    a.request_open_path(path.clone());
    b.request_open_path(path.clone());
    edit_first_text(&mut a, "ALPHA-111");
    edit_first_text(&mut b, "BRAVO-222");
    assert!(a.save_document());
    let saved_by_a = std::fs::read(&path).unwrap();
    assert!(!b.request_close());
    assert!(!b.save_document());
    assert_eq!(b.conflict_destination.as_ref(), Some(&path));
    assert!(b.is_dirty());
    assert!(!b.close_approved);
    b.cancel_pending_action();
    assert!(b.is_dirty());
    assert!(!b.decision_pending());
    assert_eq!(loaded_docx_text(&b), "BRAVO-222");
    assert!(b.save_document_to(copy.clone()));
    assert!(!b.is_dirty());
    assert_eq!(std::fs::read(path).unwrap(), saved_by_a);
    assert_eq!(
        DocxPackage::open(copy)
            .unwrap()
            .model
            .fragment(0)
            .unwrap()
            .text,
        "BRAVO-222"
    );
}

#[test]
fn saving_before_pending_open_switches_only_after_the_save_succeeds() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let second = folder.path().join("second.docx");
    write_docx_fixture(&first, "First");
    write_docx_fixture(&second, "Second");
    let mut app = NotOfficeApp::default();
    app.request_open_path(first.clone());
    edit_first_text(&mut app, "Saved before opening second");
    app.request_open_path(second.clone());
    assert_eq!(app.opened_path.as_ref(), Some(&first));
    assert!(app.save_document());
    let ctx = egui::Context::default();
    let _output = ctx.run(egui::RawInput::default(), |ctx| {
        app.resume_pending_action(ctx)
    });
    assert_eq!(app.opened_path.as_ref(), Some(&second));
    assert_eq!(loaded_docx_text(&app), "Second");
    assert!(!app.is_dirty());
    assert_eq!(
        DocxPackage::open(first)
            .unwrap()
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Saved before opening second"
    );
}

#[test]
fn failed_conflict_reload_keeps_my_unsaved_model_and_choice_dialog() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("changed.docx");
    write_docx_fixture(&path, "Initial");
    let mut app = NotOfficeApp::default();
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "My unsaved text");
    std::fs::write(&path, b"not a DOCX now").unwrap();
    assert!(!app.save_document());
    app.open_path(path.clone());
    assert_eq!(loaded_docx_text(&app), "My unsaved text");
    assert!(app.is_dirty());
    assert_eq!(app.conflict_destination.as_ref(), Some(&path));
}

#[test]
fn explicit_conflict_reload_adopts_external_changes_and_cancels_pending_close() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("reload.docx");
    write_docx_fixture(&path, "Initial");
    let mut app = NotOfficeApp::default();
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "My unsaved text");
    write_docx_fixture(&path, "External version");
    assert!(!app.request_close());
    assert!(!app.save_document());
    app.open_path(path);
    assert_eq!(loaded_docx_text(&app), "External version");
    assert!(!app.is_dirty());
    assert!(!app.decision_pending());
    assert!(!app.close_approved);
}

#[test]
fn explicit_discard_can_close_without_writing_unsaved_text() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("discard.docx");
    write_docx_fixture(&path, "Original");
    let original = std::fs::read(&path).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "Discarded");
    assert!(!app.request_close());
    let ctx = egui::Context::default();
    let _output = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert!(app.close_approved);
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn opening_a_broken_replacement_after_discard_does_not_destroy_the_current_edits() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    let broken = folder.path().join("broken.docx");
    write_docx_fixture(&source, "Original");
    std::fs::write(&broken, b"not a ZIP").unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(source.clone());
    edit_first_text(&mut app, "Still unsaved");
    app.request_open_path(broken);
    let ctx = egui::Context::default();
    let _output = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert_eq!(app.opened_path.as_ref(), Some(&source));
    assert_eq!(loaded_docx_text(&app), "Still unsaved");
    assert!(app.is_dirty());
    assert!(app.status.contains("Не удалось открыть"));
}

#[test]
fn opening_second_docx_replaces_active_model_and_records_each_stage() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let second = folder.path().join("second.docx");
    write_docx_fixture(&first, "Document A");
    write_docx_fixture(&second, "Document B");
    let mut app = NotOfficeApp::default();

    app.request_open_path(first.clone());
    assert_eq!(loaded_docx_text(&app), "Document A");
    assert!(!app.is_dirty());

    app.request_open_path(second.clone());
    assert_eq!(app.opened_path.as_deref(), Some(second.as_path()));
    assert_eq!(loaded_docx_text(&app), "Document B");
    assert!(!app.is_dirty());
    assert!(app.pending_action.is_none());
    assert!(app
        .open_diagnostics
        .iter()
        .any(|entry| entry.contains("выбран файл")));
    assert!(app
        .open_diagnostics
        .iter()
        .any(|entry| entry.contains("несохранённых правок DOCX нет")));
    assert!(app
        .open_diagnostics
        .iter()
        .any(|entry| entry.contains("загрузка пакета DOCX")));
    assert!(app
        .open_diagnostics
        .iter()
        .any(|entry| entry.contains("заменяем активную модель")));
    assert!(app
        .status
        .contains("активная модель заменена, интерфейс обновлён"));
}

#[test]
fn preview_plan_is_rebuilt_for_the_replacement_document() {
    let first = document(
        (0..120)
            .map(|index| paragraph(&format!("A{index} {}", "long text ".repeat(20))))
            .collect(),
    );
    let second = document(vec![paragraph("B")]);
    let (_, stale_ranges) = docx_preview_plan(&first, &[], print::PageSettings::default(), 1.0);
    let (replacement_flow, replacement_ranges) =
        docx_preview_plan(&second, &[], print::PageSettings::default(), 1.0);

    assert!(
        stale_ranges
            .iter()
            .any(|range| range.end > replacement_flow.len()),
        "the fixture must model the old unsafe A-plan/B-model combination"
    );
    assert!(replacement_ranges
        .iter()
        .all(|range| range.start <= range.end && range.end <= replacement_flow.len()));
}

#[test]
fn failed_second_docx_open_keeps_existing_model_and_shows_error() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let broken = folder.path().join("broken.docx");
    write_docx_fixture(&first, "Document A");
    std::fs::write(&broken, b"not a ZIP package").unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(first.clone());

    app.request_open_path(broken);

    assert_eq!(app.opened_path.as_deref(), Some(first.as_path()));
    assert_eq!(loaded_docx_text(&app), "Document A");
    assert!(app
        .status
        .starts_with("Не удалось открыть на этапе «загрузка DOCX»:"));
    assert!(app
        .open_diagnostics
        .iter()
        .any(|entry| entry.contains("загрузка пакета DOCX")));
}

#[test]
fn opening_another_document_waits_when_current_docx_is_dirty() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let second = folder.path().join("second.docx");
    write_docx_fixture(&first, "Document A");
    write_docx_fixture(&second, "Document B");
    let mut app = NotOfficeApp::default();
    app.request_open_path(first.clone());
    app.docx
        .as_mut()
        .unwrap()
        .model
        .fragment_mut(0)
        .unwrap()
        .text = "Unsaved A".into();

    app.request_open_path(second.clone());

    assert_eq!(app.opened_path.as_deref(), Some(first.as_path()));
    assert_eq!(loaded_docx_text(&app), "Unsaved A");
    assert_eq!(
        app.pending_action,
        Some(PendingAction::Open(second.clone()))
    );
    assert!(app.is_dirty());
    assert!(app.status.contains("обнаружены несохранённые правки DOCX"));
}

#[test]
fn pagination_starts_a_new_page_when_content_does_not_fit() {
    let model = document(vec![paragraph("One"), paragraph("Two"), paragraph("Three")]);
    let flow = document_flow(&model);
    assert_eq!(
        paginate_blocks(&flow, &model, 100.0, 50.0, 1.0),
        vec![0..1, 1..2, 2..3]
    );
}

#[test]
fn pagination_keeps_an_empty_document_on_one_page() {
    let model = DocumentModel::default();
    assert_eq!(
        paginate_blocks(&document_flow(&model), &model, 100.0, 50.0, 1.0),
        vec![0..0]
    );
}

#[test]
fn table_rows_are_separate_flow_blocks() {
    let model = DocumentModel {
        paragraphs: vec![paragraph("Left"), paragraph("Right")],
        blocks: vec![DocumentBlock::Table {
            rows: vec![TableRow {
                cells: vec![vec![0], vec![1]],
            }],
            grid_widths: vec![1, 1],
        }],
    };
    assert_eq!(
        document_flow(&model),
        vec![FlowBlock::TableRow {
            row: TableRow {
                cells: vec![vec![0], vec![1]],
            },
            grid_widths: vec![1, 1],
            column_count: 2,
        }]
    );
}

#[test]
fn table_grid_widths_preserve_column_proportions() {
    assert_eq!(
        table_column_widths(300.0, &[1, 2], 2, 0.0),
        vec![100.0, 200.0]
    );
}

#[test]
fn explicit_page_break_starts_a_new_preview_page() {
    let mut first = paragraph("First");
    first.page_break_after = true;
    let model = document(vec![first, paragraph("Second")]);
    let flow = document_flow(&model);
    assert_eq!(
        paginate_blocks(&flow, &model, 100.0, 500.0, 1.0),
        vec![0..1, 1..2]
    );
}

#[test]
fn docx_print_blocks_preserve_explicit_preview_page_breaks() {
    let mut first = paragraph("First");
    first.page_break_after = true;
    let model = document(vec![first, paragraph("Second")]);

    assert_eq!(
        docx_print_blocks(&model, &[], print::PageSettings::default()),
        vec![
            print::PrintBlock::Text("First".into()),
            print::PrintBlock::PageBreak,
            print::PrintBlock::Text("Second".into()),
        ]
    );
}

#[test]
fn docx_print_blocks_preserve_table_borders_contract_and_embedded_images() {
    let model = DocumentModel {
        paragraphs: vec![paragraph("before"), paragraph("cell")],
        blocks: vec![
            DocumentBlock::Paragraph(0),
            DocumentBlock::Table {
                rows: vec![TableRow {
                    cells: vec![vec![1]],
                }],
                grid_widths: vec![1],
            },
        ],
    };
    let images = vec![liteoffice::docx::DocxImage {
        bytes: vec![137, 80, 78, 71],
        paragraph_index: 1,
        extent_emu: Some((914400, 457200)),
    }];
    let settings = print::PageSettings {
        image_scale: print::ImageScale::OriginalSize,
        ..Default::default()
    };

    assert_eq!(
        docx_print_blocks(&model, &images, settings),
        vec![
            print::PrintBlock::Text("before".into()),
            print::PrintBlock::TableRow(vec!["cell".into()]),
            print::PrintBlock::SizedImage {
                bytes: vec![137, 80, 78, 71],
                width_pt: 72.0,
                height_pt: 36.0,
            },
        ]
    );
}

#[test]
fn docx_page_plan_covers_long_multilingual_document_without_gaps_or_duplicates() {
    let model = document(
        (0..180)
            .map(|index| {
                paragraph(&format!(
                    "Страница {index}: {}",
                    "Unicode ✓ long text ".repeat(8)
                ))
            })
            .collect(),
    );
    let ranges = docx_page_ranges(&model, print::PageSettings::default(), 1.0);
    let flow_len = document_flow(&model).len();
    assert!(ranges.len() > 1);
    assert!(ranges.iter().all(|range| !range.is_empty()));
    assert_eq!(
        ranges
            .iter()
            .flat_map(|range| range.clone())
            .collect::<Vec<_>>(),
        (0..flow_len).collect::<Vec<_>>()
    );

    let printed_text = docx_print_blocks(&model, &[], print::PageSettings::default())
        .into_iter()
        .filter_map(|block| match block {
            print::PrintBlock::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(printed_text.len(), model.paragraphs.len());
    assert_eq!(printed_text[0], model.paragraphs[0].fragments[0].text);
    assert_eq!(
        printed_text.last(),
        model
            .paragraphs
            .last()
            .map(|paragraph| &paragraph.fragments[0].text)
    );
}

#[test]
fn docx_page_plan_uses_selected_margins_and_orientation() {
    let model = document(
        (0..80)
            .map(|index| paragraph(&format!("Paragraph {index} {}", "text ".repeat(30))))
            .collect(),
    );
    let default_pages = docx_page_ranges(&model, print::PageSettings::default(), 1.0);
    let narrow_pages = docx_page_ranges(
        &model,
        print::PageSettings {
            margin_mm: 40.0,
            ..print::PageSettings::default()
        },
        1.0,
    );
    let landscape_pages = docx_page_ranges(
        &model,
        print::PageSettings {
            landscape: true,
            ..print::PageSettings::default()
        },
        1.0,
    );

    assert!(narrow_pages.len() >= default_pages.len());
    assert_ne!(landscape_pages, default_pages);
}

#[test]
fn search_finds_matching_document_fragments() {
    let model = document(vec![paragraph("Budget plan"), paragraph("Other budget")]);
    let result = search::docx(&model, "BUDGET");
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| &hit.target)
            .collect::<Vec<_>>(),
        vec![
            &SearchTarget::DocxParagraph(0),
            &SearchTarget::DocxParagraph(1)
        ]
    );
    assert!(search::docx(&model, "").hits.is_empty());
}

fn search_key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        modifiers,
        events: vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    }
}

#[test]
fn unified_find_ctrl_f_focus_enter_f3_escape_and_modal_guard() {
    let mut app = NotOfficeApp {
        legacy_doc: Some(LegacyDocPackage {
            paragraphs: vec!["Ёжик ёжик".into(), "ЁЖИК".into()],
            ..Default::default()
        }),
        search_query: "ёжик".into(),
        ..Default::default()
    };
    let original = app.legacy_doc.as_ref().unwrap().paragraphs.clone();
    let ctx = egui::Context::default();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::F, command), |ctx| {
        app.handle_search_shortcuts(ctx);
        egui::CentralPanel::default().show(ctx, |ui| app.show_search_bar(ui));
    });
    assert_eq!(ctx.memory(|memory| memory.focused()), app.search_input_id);
    assert!(app.toolbar_text_field_is_focused(&ctx));
    assert_eq!(app.search_results.hits.len(), 3);
    assert_eq!(app.current_match, 0);
    let _ = ctx.run(search_key(egui::Key::Enter, egui::Modifiers::NONE), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.show_search_bar(ui));
    });
    assert_eq!(app.current_match, 1);
    let _ = ctx.run(search_key(egui::Key::F3, egui::Modifiers::SHIFT), |ctx| {
        app.handle_search_shortcuts(ctx)
    });
    assert_eq!(app.current_match, 0);
    app.pending_action = Some(PendingAction::Close);
    let _ = ctx.run(search_key(egui::Key::F3, egui::Modifiers::NONE), |ctx| {
        app.handle_search_shortcuts(ctx)
    });
    assert_eq!(app.current_match, 0);
    app.pending_action = None;
    let _ = ctx.run(search_key(egui::Key::F, command), |ctx| {
        app.handle_search_shortcuts(ctx);
        egui::CentralPanel::default().show(ctx, |ui| app.show_search_bar(ui));
    });
    assert!(app.toolbar_text_field_is_focused(&ctx));
    let _ = ctx.run(
        search_key(egui::Key::Escape, egui::Modifiers::NONE),
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.show_search_bar(ui));
        },
    );
    assert!(app.search_query.is_empty());
    assert!(app.search_results.hits.is_empty());
    assert!(!app.search_scroll_requested);
    assert_eq!(app.legacy_doc.as_ref().unwrap().paragraphs, original);
    assert!(!app.is_dirty());
}

#[test]
fn unified_find_stays_on_the_selected_sheet_and_exposes_far_cell() {
    use liteoffice::xlsx::{SpreadsheetCell, SpreadsheetModel};
    let sheets = vec![
        SpreadsheetModel {
            sheet_name: "Первый".into(),
            cells: vec![SpreadsheetCell {
                row: 0,
                column: 0,
                value: "Поиск".into(),
            }],
            row_count: 1,
            column_count: 1,
        },
        SpreadsheetModel {
            sheet_name: "Второй".into(),
            cells: vec![SpreadsheetCell {
                row: 499,
                column: 29,
                value: "ПОИСК".into(),
            }],
            row_count: 500,
            column_count: 30,
        },
    ];
    let mut app = NotOfficeApp {
        spreadsheet: Some(SpreadsheetDocument::from_sheets(sheets.clone())),
        search_query: "поиск".into(),
        ..Default::default()
    };
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 1);
    app.navigate_search(false);
    let book = app.spreadsheet.as_ref().unwrap();
    assert_eq!(book.selected_sheet, 0);
    assert_eq!(book.selected_cell, Some((0, 0)));
    assert_eq!(book.preview_anchor, Some((0, 0)));

    // Switch to the second sheet: the same query must search only that sheet.
    app.spreadsheet.as_mut().unwrap().selected_sheet = 1;
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 1);
    app.navigate_search(false);
    let book = app.spreadsheet.as_ref().unwrap();
    assert_eq!(book.selected_sheet, 1);
    assert_eq!(book.selected_cell, Some((499, 29)));
    assert_eq!(book.preview_anchor, Some((499, 29)));
    assert!(app.search_context().contains("Второй!AD500"));
    assert!(app.search_context().contains("[ПОИСК]"));
    assert_eq!(book.sheets, sheets);
    app.search_scroll_requested = false; // rendering consumed this request
    app.refresh_search();
    assert!(
        !app.search_scroll_requested,
        "cached results must not request scrolling every frame"
    );
    app.navigate_search(false);
    assert_eq!(app.spreadsheet.as_ref().unwrap().selected_sheet, 1);
    app.spreadsheet.as_mut().unwrap().selected_sheet = 0;
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 1);
    app.search_query = "Missing".into();
    app.refresh_search();
    assert!(app.search_results.hits.is_empty());
    assert_eq!(app.current_match, 0);
    assert!(!app.search_scroll_requested);
}

#[test]
fn unified_find_refreshes_after_docx_edit_undo_redo_and_replacement_without_dirtying() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let second = folder.path().join("second.docx");
    write_docx_fixture(&first, "Поиск Поиск");
    write_docx_fixture(&second, "Поиск");
    let original = std::fs::read(&first).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(first.clone());
    app.search_query = "ПОИСК".into();
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 2);
    assert!(!app.is_dirty());
    let before = app.docx.as_ref().unwrap().model.clone();
    edit_first_text(&mut app, "Нет совпадений");
    app.record_docx_edit(before);
    app.refresh_search();
    assert!(app.search_results.hits.is_empty());
    app.undo();
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 2);
    assert!(!app.is_dirty());
    app.redo();
    app.refresh_search();
    assert!(app.search_results.hits.is_empty());
    app.undo();
    app.request_open_path(second);
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 1);
    assert_eq!(app.current_match, 0);
    assert_eq!(std::fs::read(first).unwrap(), original);
    assert!(!app.is_dirty());
}

#[test]
fn unified_find_keeps_docx_table_text_searchable() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("mixed.docx");
    write_print_fixture(&source);
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    app.search_query = "ПРАВАЯ".into();
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 1);
    assert!(app.search_context().contains("[Правая]"));
    assert!(!app.is_dirty());
}

#[test]
fn replace_current_then_all_are_single_undo_transactions_and_save_as_roundtrips() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    let copy = folder.path().join("copy.docx");
    write_docx_fixture(&source, "Ёжик ёжик ЁЖИК");
    let original_bytes = std::fs::read(&source).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(source.clone());
    app.search_query = "ёжик".into();
    app.replacement_text = "Кот".into();
    app.refresh_search();
    app.navigate_search(false);
    app.replace_docx(false);
    assert_eq!(loaded_docx_text(&app), "Ёжик Кот ЁЖИК");
    assert_eq!(app.undo_stack.len(), 1);
    assert_eq!(app.current_match, 1);
    assert!(app.is_dirty());
    app.undo();
    assert!(!app.is_dirty());
    app.redo();
    assert_eq!(loaded_docx_text(&app), "Ёжик Кот ЁЖИК");
    app.replacement_text.clear();
    app.replace_docx(true);
    assert_eq!(loaded_docx_text(&app), " Кот ");
    assert_eq!(app.undo_stack.len(), 2);
    app.undo();
    assert_eq!(loaded_docx_text(&app), "Ёжик Кот ЁЖИК");
    app.redo();
    assert!(app.save_document_to(copy.clone()));
    assert!(!app.is_dirty());
    let saved = app.docx.as_ref().unwrap().model.clone();
    app.request_open_path(copy);
    assert_eq!(app.docx.as_ref().unwrap().model, saved);
    assert_eq!(std::fs::read(source).unwrap(), original_bytes);
}

#[test]
fn replacement_skips_its_own_inserted_matches_and_uses_fresh_editor_text() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    write_docx_fixture(&source, "a a");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    app.search_query = "a".into();
    app.replacement_text = "a+a".into();
    app.replace_docx(false);
    assert_eq!(loaded_docx_text(&app), "a+a a");
    assert_eq!(app.current_match, 2);
    app.replace_docx(false);
    assert_eq!(loaded_docx_text(&app), "a+a a+a");
    app.undo();
    let before = app.docx.as_ref().unwrap().model.clone();
    edit_first_text(&mut app, "Different text");
    app.record_docx_edit(before);
    app.replace_docx(true);
    assert_eq!(loaded_docx_text(&app), "Different text");
}

#[test]
fn replace_noop_invalid_and_modal_operations_do_not_change_dirty_history_or_source() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    write_docx_fixture(&source, "same");
    let original = std::fs::read(&source).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(source.clone());
    app.search_query = "same".into();
    app.replacement_text = "same".into();
    app.replace_docx(true);
    assert!(!app.is_dirty());
    assert!(app.undo_stack.is_empty());
    app.replacement_text = "\0".into();
    app.replace_docx(true);
    assert!(!app.is_dirty());
    assert!(app.undo_stack.is_empty());
    assert!(app.status.contains("Замена отменена"));
    app.replacement_text = "changed".into();
    app.pending_action = Some(PendingAction::Close);
    app.replace_docx(true);
    assert_eq!(loaded_docx_text(&app), "same");
    assert!(app.undo_stack.is_empty());
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn replacement_is_per_window_and_two_window_saves_still_detect_conflicts() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("shared.docx");
    let copy = folder.path().join("second-window.docx");
    write_docx_fixture(&source, "Original Original");
    let original = std::fs::read(&source).unwrap();
    let mut first = NotOfficeApp::default();
    let mut second = NotOfficeApp::default();
    for app in [&mut first, &mut second] {
        app.request_open_path(source.clone());
        app.search_query = "Original".into();
    }
    first.replacement_text = "First".into();
    first.replace_docx(true);
    assert_eq!(loaded_docx_text(&second), "Original Original");
    assert!(second.undo_stack.is_empty());
    assert!(!second.is_dirty());
    second.replacement_text = "Second".into();
    second.replace_docx(true);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert!(first.save_document());
    assert!(!second.save_document());
    assert!(second.conflict_destination.is_some());
    assert!(second.is_dirty());
    assert_eq!(loaded_docx_text(&second), "Second Second");
    assert_eq!(loaded_docx_text(&first), "First First");
    assert!(second.save_document_to(copy.clone()));
    assert_eq!(
        DocxPackage::open(&source).unwrap().model,
        first.docx.as_ref().unwrap().model
    );
    assert_eq!(
        DocxPackage::open(&copy).unwrap().model,
        second.docx.as_ref().unwrap().model
    );
}

#[test]
fn ctrl_h_replace_enter_escape_and_toolbar_undo_focus_are_guarded() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("replace.docx");
    write_docx_fixture(&source, "Find Find");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    app.search_query = "Find".into();
    app.replacement_text = "Done".into();
    let ctx = egui::Context::default();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::H, command), |ctx| {
        app.handle_search_shortcuts(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            app.show_search_bar(ui);
            app.show_replace_bar(ui);
        });
    });
    assert!(app.replace_open);
    assert_eq!(ctx.memory(|m| m.focused()), app.search_input_id);
    let id = app.replacement_input_id.unwrap();
    ctx.memory_mut(|m| m.request_focus(id));
    let _ = ctx.run(search_key(egui::Key::Enter, egui::Modifiers::NONE), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            app.show_search_bar(ui);
            app.show_replace_bar(ui);
        });
    });
    assert_eq!(loaded_docx_text(&app), "Done Find");
    assert_eq!(app.undo_stack.len(), 1);
    assert!(app.toolbar_text_field_is_focused(&ctx));
    let _ = ctx.run(search_key(egui::Key::Z, command), |ctx| {
        app.handle_edit_shortcuts(ctx);
        // A real app frame keeps the toolbar alive; omitting it would
        // surrender TextEdit focus before the following Escape event.
        egui::CentralPanel::default().show(ctx, |ui| {
            app.show_search_bar(ui);
            app.show_replace_bar(ui);
        });
    });
    assert_eq!(
        loaded_docx_text(&app),
        "Find Find",
        "Ctrl+Z in the replacement field must undo the document replacement"
    );
    let _ = ctx.run(
        search_key(egui::Key::Escape, egui::Modifiers::NONE),
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.show_search_bar(ui);
                app.show_replace_bar(ui);
            });
        },
    );
    assert!(!app.replace_open);
    ctx.memory_mut(|m| m.surrender_focus(id));
    let _ = ctx.run(search_key(egui::Key::Y, command), |ctx| {
        app.handle_edit_shortcuts(ctx)
    });
    assert_eq!(loaded_docx_text(&app), "Done Find");
    let _ = ctx.run(search_key(egui::Key::Z, command), |ctx| {
        app.handle_edit_shortcuts(ctx)
    });
    assert_eq!(loaded_docx_text(&app), "Find Find");
    app.pending_action = Some(PendingAction::Close);
    let _ = ctx.run(search_key(egui::Key::H, command), |ctx| {
        app.handle_search_shortcuts(ctx)
    });
    assert!(!app.replace_open);
}

#[test]
fn undo_history_retains_the_latest_hundred_edits_instead_of_dropping_newest() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("history.docx");
    write_docx_fixture(&source, "0");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    for index in 1..=105 {
        let before = app.docx.as_ref().unwrap().model.clone();
        edit_first_text(&mut app, &index.to_string());
        app.record_docx_edit(before);
    }
    assert_eq!(app.undo_stack.len(), 100);
    app.undo();
    assert_eq!(loaded_docx_text(&app), "104");
    for _ in 0..99 {
        app.undo();
    }
    assert_eq!(loaded_docx_text(&app), "5");
}

#[test]
fn delete_after_ctrl_a_clears_all_docx_text_as_one_undo_without_removing_images_or_tables() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    let output = folder.path().join("deleted.docx");
    write_print_fixture(&source);
    let original_bytes = std::fs::read(&source).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(source.clone());
    let original_model = app.docx.as_ref().unwrap().model.clone();
    let images = app.docx.as_ref().unwrap().images.clone();
    assert!(!images.is_empty());
    let ctx = egui::Context::default();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::A, command), |ctx| {
        app.handle_docx_selection_shortcuts(ctx)
    });
    assert!(app.select_all_docx);
    assert!(!app.is_dirty());
    let _ = ctx.run(
        search_key(egui::Key::Delete, egui::Modifiers::NONE),
        |ctx| app.handle_docx_selection_shortcuts(ctx),
    );
    assert!(app
        .docx
        .as_ref()
        .unwrap()
        .model
        .paragraphs
        .iter()
        .flat_map(|p| &p.fragments)
        .all(|f| f.text.is_empty()));
    assert_eq!(app.undo_stack.len(), 1);
    assert!(app.is_dirty());
    assert_eq!(
        app.docx.as_ref().unwrap().model.blocks,
        original_model.blocks
    );
    assert_eq!(app.docx.as_ref().unwrap().images, images);
    app.undo();
    assert_eq!(app.docx.as_ref().unwrap().model, original_model);
    assert!(!app.is_dirty());
    app.redo();
    assert!(app.save_document_to(output));
    assert!(!app.is_dirty());
    assert_eq!(app.docx.as_ref().unwrap().images, images);
    assert_eq!(std::fs::read(source).unwrap(), original_bytes);
}

#[test]
fn native_fragment_delete_removes_selected_text_and_character_at_cursor() {
    use egui::text::{CCursor, CCursorRange};
    let ctx = egui::Context::default();
    let mut paragraph = paragraph("Ёжик ABC");
    let mut selected = None;
    let mut all = false;
    let mut focus_pending = false;
    let mut render = |input| {
        ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_paragraph(
                    ui,
                    &mut paragraph,
                    0,
                    &mut ParagraphRenderOptions {
                        content_width: 600.0,
                        zoom: 1.0,
                        query: "",
                        current_match: None,
                        scroll_to_match: false,
                        selected_fragment_id: &mut selected,
                        select_all_docx: &mut all,
                        editor_generation: 0,
                        focus_pending: &mut focus_pending,
                        reveal_focused: false,
                    },
                )
            });
        })
    };
    let _ = render(search_key(egui::Key::ArrowRight, egui::Modifiers::NONE));
    for pressed in [true, false] {
        let mut input = search_key(egui::Key::ArrowRight, egui::Modifiers::NONE);
        input.events = vec![
            egui::Event::PointerMoved(egui::pos2(20.0, 20.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(20.0, 20.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let _ = render(input);
    }
    let id = ctx
        .memory(|m| m.focused())
        .expect("click must focus the real fragment editor");
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(4))));
    egui::TextEdit::store_state(&ctx, id, state);
    let _ = render(search_key(egui::Key::Delete, egui::Modifiers::NONE));
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(1))));
    egui::TextEdit::store_state(&ctx, id, state);
    let _ = render(search_key(egui::Key::Delete, egui::Modifiers::NONE));
    assert_eq!(paragraph.fragments[0].text, " BC");
}

fn assert_preview_word_ink_is_not_clipped(output: &egui::FullOutput, word: &str) {
    fn check(shape: &egui::epaint::Shape, clip: egui::Rect, word: &str) -> usize {
        match shape {
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().map(|shape| check(shape, clip, word)).sum()
            }
            egui::epaint::Shape::Text(text) if text.galley.job.text == word => {
                let ink = text.visual_bounding_rect();
                assert!(
                    clip.expand(0.01).contains_rect(ink),
                    "last glyph clipped for {word}: ink {ink:?}, clip {clip:?}"
                );
                1
            }
            _ => 0,
        }
    }
    assert!(
        output
            .shapes
            .iter()
            .map(|shape| check(&shape.shape, shape.clip_rect, word))
            .sum::<usize>()
            > 0,
        "real preview must paint {word}"
    );
}

