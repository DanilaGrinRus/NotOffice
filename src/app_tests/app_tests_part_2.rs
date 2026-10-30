#[test]
fn full_preview_keeps_last_cyrillic_and_wide_glyphs_visible_at_all_view_zooms() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("wide.docx");
    for (word, table) in [
        ("ЁЖИК", false),
        ("ШЩЖМ", false),
        ("WWMM", false),
        ("ЁЖИК", true),
        ("ШЩЖМ", true),
        ("WWMM", true),
    ] {
        let paragraph = format!("<w:p><w:r><w:t>{word}</w:t></w:r></w:p>");
        let body = if table {
            format!("<w:tbl><w:tr><w:tc>{paragraph}</w:tc></w:tr></w:tbl>")
        } else {
            paragraph
        };
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        write_docx_xml_fixture(&source, &xml);
        for zoom in [0.5, 1.0, 1.25, 2.0, 4.0] {
            let ctx = egui::Context::default();
            install_windows_fonts(&ctx);
            let mut app = NotOfficeApp {
                zoom,
                fonts_initialized: true,
                ..Default::default()
            };
            app.request_open_path(source.clone());
            let original = app.docx.as_ref().unwrap().model.clone();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(4000.0, 4000.0),
                )),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| app.show(ctx));
            assert_preview_word_ink_is_not_clipped(&output, word);
            assert_eq!(app.docx.as_ref().unwrap().model, original);
            assert!(!app.is_dirty());
        }
    }
}

#[test]
fn full_ui_accepts_first_typed_text_after_open_without_pointer_or_tab() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("typing.docx");
    write_docx_fixture(&source, "Original");
    let source_bytes = std::fs::read(&source).unwrap();
    for type_in_first_frame in [true, false] {
        let ctx = egui::Context::default();
        let mut app = NotOfficeApp::default();
        app.request_open_path(source.clone());
        if !type_in_first_frame {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
            assert!(!app.is_dirty(), "focus alone must not edit");
        }
        let input = egui::RawInput {
            events: vec![egui::Event::Text("Новый ".into())],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.show(ctx));
        assert_eq!(
            loaded_docx_text(&app),
            "Новый Original",
            "typing after open must not need a click"
        );
        assert_eq!(app.undo_stack.len(), 1);
    }
    assert_eq!(std::fs::read(source).unwrap(), source_bytes);
}

#[test]
fn full_ui_new_docx_focus_does_not_deliver_typing_to_previous_find_field() {
    let folder = tempfile::tempdir().unwrap();
    let first = folder.path().join("first.docx");
    let second = folder.path().join("second.docx");
    write_docx_fixture(&first, "Original");
    write_docx_fixture(&second, "Second");
    let mut app = NotOfficeApp::default();
    let ctx = egui::Context::default();
    app.request_open_path(first);
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    let first_id = ctx.memory(|memory| memory.focused()).unwrap();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::F, command), |ctx| app.show(ctx));
    assert_eq!(ctx.memory(|memory| memory.focused()), app.search_input_id);
    app.search_query = "Original".into();
    let generation = app.docx_editor_generation;
    app.request_open_path(second);
    assert_ne!(app.docx_editor_generation, generation);
    let input = egui::RawInput {
        events: vec![egui::Event::Text("X".into())],
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| app.show(ctx));
    assert_eq!(loaded_docx_text(&app), "XSecond");
    assert_eq!(
        app.search_query, "Original",
        "old toolbar field must not consume this event"
    );
    assert_ne!(ctx.memory(|memory| memory.focused()).unwrap(), first_id);
    app.undo();
    assert_eq!(loaded_docx_text(&app), "Second");
}

#[test]
fn full_ui_autofocus_is_one_shot_and_respects_find_replace_and_modal_choices() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("focus.docx");
    write_docx_fixture(&source, "Original");
    let ctx = egui::Context::default();
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    app.pending_action = Some(PendingAction::Close);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("Ignored".into())],
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert!(app.docx_focus_pending);
    assert_eq!(loaded_docx_text(&app), "Original");
    app.cancel_pending_action();
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    assert!(!app.docx_focus_pending);
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::H, command), |ctx| app.show(ctx));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("FindText".into())],
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert_eq!(app.search_query, "FindText");
    assert_eq!(loaded_docx_text(&app), "Original");
    let replacement_id = app.replacement_input_id.unwrap();
    ctx.memory_mut(|memory| memory.request_focus(replacement_id));
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("ReplaceText".into())],
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert_eq!(app.replacement_text, "ReplaceText");
    assert_eq!(loaded_docx_text(&app), "Original");
    assert!(!app.is_dirty());
}

#[test]
fn full_ui_independent_windows_type_without_a_click_and_do_not_share_editor_state() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("shared.docx");
    write_docx_fixture(&source, "Original");
    let mut first = NotOfficeApp::default();
    let mut second = NotOfficeApp::default();
    let first_ctx = egui::Context::default();
    let second_ctx = egui::Context::default();
    first.request_open_path(source.clone());
    second.request_open_path(source);
    let _ = first_ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("A".into())],
            ..Default::default()
        },
        |ctx| first.show(ctx),
    );
    let _ = second_ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("B".into())],
            ..Default::default()
        },
        |ctx| second.show(ctx),
    );
    assert_eq!(loaded_docx_text(&first), "AOriginal");
    assert_eq!(loaded_docx_text(&second), "BOriginal");
    first.undo();
    assert_eq!(loaded_docx_text(&first), "Original");
    assert_eq!(loaded_docx_text(&second), "BOriginal");
}

#[test]
fn native_sized_window_keeps_initial_editor_visible_focused_and_typable_after_idle_frames() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("replace.docx");
    write_docx_xml_fixture(
        &source,
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>кот</w:t></w:r><w:r><w:t> кот</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>ЁЖИК</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
    );
    let ctx = egui::Context::default();
    install_windows_fonts(&ctx);
    let mut app = NotOfficeApp {
        zoom: 2.0,
        fonts_initialized: true,
        ..Default::default()
    };
    let input = |frame: usize| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        time: Some(frame as f64 / 60.0),
        ..Default::default()
    };
    let _ = ctx.run(input(0), |ctx| app.show(ctx));
    app.request_open_path(source);
    let editor_id = docx_editor_id(app.docx_editor_generation, 0);
    for frame in 1..=60 {
        let output = ctx.run(input(frame), |ctx| app.show(ctx));
        if frame == 60 {
            assert_preview_word_ink_is_not_clipped(&output, "кот");
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(editor_id),
                "native-sized view must retain editor focus"
            );
        }
    }
    let mut typing = input(61);
    typing.events = vec![egui::Event::Text("ТЕСТ7".into())];
    let _ = ctx.run(typing, |ctx| app.show(ctx));
    assert!(loaded_docx_text(&app).contains("ТЕСТ7"));
}

#[test]
fn initial_caret_and_text_stay_on_screen_in_a_short_window_at_four_hundred_percent() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("large-zoom.docx");
    write_docx_fixture(&source, "Visible");
    let ctx = egui::Context::default();
    let mut app = NotOfficeApp {
        zoom: 4.0,
        print_settings: print::PageSettings::default(),
        ..Default::default()
    };
    app.request_open_path(source);
    for frame in 0..60 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            },
            |ctx| app.show(ctx),
        );
        if frame == 59 {
            assert_preview_word_ink_is_not_clipped(&output, "Visible");
        }
    }
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(docx_editor_id(app.docx_editor_generation, 0))
    );
}

#[test]
fn find_tab_moves_directly_to_replace_and_shift_tab_returns_without_editing_docx() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("tab.docx");
    write_docx_fixture(&source, "Original");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    let ctx = egui::Context::default();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::H, command), |ctx| app.show(ctx));
    assert_eq!(ctx.memory(|m| m.focused()), app.search_input_id);
    let _ = ctx.run(search_key(egui::Key::Tab, egui::Modifiers::NONE), |ctx| {
        app.show(ctx)
    });
    assert_eq!(ctx.memory(|m| m.focused()), app.replacement_input_id);
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Text("Replacement".into())],
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert_eq!(app.replacement_text, "Replacement");
    let _ = ctx.run(search_key(egui::Key::Tab, egui::Modifiers::SHIFT), |ctx| {
        app.show(ctx)
    });
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    assert_eq!(ctx.memory(|m| m.focused()), app.search_input_id);
    assert_eq!(loaded_docx_text(&app), "Original");
    assert!(!app.is_dirty());
}

#[test]
fn find_always_shows_replacement_input_without_inserting_a_tab() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("tab-visible.docx");
    write_docx_fixture(&source, "Original");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    let ctx = egui::Context::default();
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(search_key(egui::Key::F, command), |ctx| app.show(ctx));
    assert!(app.replace_open);
    assert!(app.replacement_input_id.is_some());
    assert!(app.search_query.is_empty() && app.replacement_text.is_empty());
    assert_eq!(loaded_docx_text(&app), "Original");
    assert!(!app.is_dirty());
}

#[test]
fn delete_all_is_blocked_for_toolbar_fields_and_modal_prompts() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("guarded.docx");
    write_docx_fixture(&source, "Keep me");
    let mut app = NotOfficeApp::default();
    app.request_open_path(source);
    let ctx = egui::Context::default();
    for replacement_field in [false, true] {
        let id = egui::Id::new(("toolbar-field", replacement_field));
        if replacement_field {
            app.replacement_input_id = Some(id);
        } else {
            app.search_input_id = Some(id);
        }
        ctx.memory_mut(|m| m.request_focus(id));
        app.select_all_docx = true;
        let _ = ctx.run(
            search_key(egui::Key::Delete, egui::Modifiers::NONE),
            |ctx| {
                app.handle_docx_selection_shortcuts(ctx);
            },
        );
        assert_eq!(loaded_docx_text(&app), "Keep me");
        assert!(app.undo_stack.is_empty());
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
    app.pending_action = Some(PendingAction::Close);
    let _ = ctx.run(
        search_key(egui::Key::Delete, egui::Modifiers::NONE),
        |ctx| app.handle_docx_selection_shortcuts(ctx),
    );
    assert_eq!(loaded_docx_text(&app), "Keep me");
    assert!(!app.is_dirty());
}

#[test]
fn failed_pdf_open_keeps_unsaved_docx_and_pdf_replacement_uses_existing_dirty_guard() {
    let folder = tempfile::tempdir().unwrap();
    let docx = folder.path().join("source.docx");
    let pdf = folder.path().join("broken.pdf");
    write_docx_fixture(&docx, "Original");
    std::fs::write(&pdf, b"%PDF-1.7\nbroken").unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(docx.clone());
    edit_first_text(&mut app, "Keep my edits");
    app.request_open_path(pdf.clone());
    assert_eq!(app.pending_action, Some(PendingAction::Open(pdf)));
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert!(app.pdf.is_none());
    assert!(app.is_dirty());
    assert_eq!(loaded_docx_text(&app), "Keep my edits");
    assert_eq!(app.opened_path.as_deref(), Some(docx.as_path()));
    assert!(app.status.contains("загрузка PDF"));
}

#[cfg(windows)]
#[test]
fn pdf_opens_read_only_and_switching_back_to_docx_retires_the_pdf_view() {
    let folder = tempfile::tempdir().unwrap();
    let pdf = folder.path().join("test.pdf");
    let docx = folder.path().join("test.docx");
    pdf_view::write_test_pdf(&pdf);
    write_docx_fixture(&docx, "Original");
    let mut app = NotOfficeApp::default();
    app.request_open_path(pdf);
    assert!(app.pdf.is_some());
    assert!(app.has_document());
    assert!(!app.is_dirty());
    assert!(app.docx.is_none());
    assert!(app.plain_text().is_none());
    assert!(!app.save_document());
    app.print_document();
    assert!(app.print_receiver.is_none());
    assert!(app.pdf_print_open);
    assert_eq!(app.pdf_print_settings.last, 2);
    app.pdf_print_open = false;
    app.request_open_path(docx);
    assert!(app.pdf.is_none());
    assert!(app.docx.is_some());
}

#[test]
fn spreadsheet_column_names_continue_after_z() {
    assert_eq!(spreadsheet_column_name(0), "A");
    assert_eq!(spreadsheet_column_name(25), "Z");
    assert_eq!(spreadsheet_column_name(26), "AA");
}

#[test]
fn request_save_explains_blank_text_instead_of_opening_save_as_immediately() {
    let mut app = NotOfficeApp::blank_text();
    assert!(!app.request_save());
    assert!(app.save_prompt_open);
    assert!(!app.save_prompt_copy);
    assert!(app.save_prompt_reason.contains("не сохранён"));
    app.save_prompt_open = false;
}

#[test]
fn blank_txt_is_ready_for_immediate_typing_save_and_unsaved_close_guard() {
    let mut app = NotOfficeApp::blank_text();
    assert!(app.has_document());
    assert!(!app.is_dirty());
    assert!(app.window_title().contains("Untitled.txt"));
    let ctx = egui::Context::default();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(
        input(vec![egui::Event::Text("Сразу без клика".into())]),
        |ctx| app.show(ctx),
    );
    assert_eq!(app.text.as_ref().unwrap().text, "Сразу без клика");
    for _ in 0..20 {
        let _ = ctx.run(input(vec![]), |ctx| app.show(ctx));
    }
    assert_eq!(ctx.memory(|m| m.focused()), app.text_input_id);
    assert!(!app.request_close());
    app.cancel_pending_action();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("typed.txt");
    assert!(app.save_document_to(path.clone()));
    assert!(!app.is_dirty());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "Сразу без клика");
}
#[test]
fn text_open_replacement_failure_and_switching_formats_preserve_active_data() {
    let folder = tempfile::tempdir().unwrap();
    let md = folder.path().join("note.md");
    std::fs::write(&md, "# Markdown").unwrap();
    let bad = folder.path().join("bad.rtf");
    std::fs::write(&bad, b"not rtf").unwrap();
    let docx = folder.path().join("test.docx");
    write_docx_fixture(&docx, "Original");
    let mut app = NotOfficeApp::blank_text();
    app.request_open_path(md.clone());
    assert_eq!(app.plain_text().as_deref(), Some("# Markdown"));
    app.request_open_path(bad);
    assert_eq!(app.plain_text().as_deref(), Some("# Markdown"));
    app.request_open_path(docx);
    assert!(app.text.is_none());
    assert!(app.docx.is_some());
    app.request_open_path(md);
    assert!(app.docx.is_none());
    assert!(app.text.is_some());
}

#[test]
fn text_recovery_restore_save_copy_and_cleanup_cover_untitled_txt_md_and_rtf() {
    use liteoffice::text_document::{TextDocument, TextKind};
    for kind in [
        None,
        Some(TextKind::Txt),
        Some(TextKind::Markdown),
        Some(TextKind::Rtf),
    ] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let source = kind.map(|kind| folder.path().join(format!("original.{}", kind.extension())));
        if let Some(source) = &source {
            std::fs::write(
                source,
                if kind == Some(TextKind::Rtf) {
                    "{\\rtf1 Original}"
                } else {
                    "Original"
                },
            )
            .unwrap();
        }
        let before = source.as_ref().map(|p| std::fs::read(p).unwrap());
        let mut app = NotOfficeApp::blank_text();
        if let Some(source) = &source {
            app.request_open_path(source.clone());
        }
        app.initialize_recovery(root.clone());
        app.text.as_mut().unwrap().text = "Несохранённое 🙂".into();
        app.sync_recovery(true);
        assert!(app.recovery_session.as_ref().unwrap().is_text());
        assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
        drop(app);
        let mut restored = NotOfficeApp::blank_text();
        restored.initialize_recovery(root.clone());
        assert_eq!(restored.recovery_candidates.len(), 1);
        assert!(restored.restore_first_recovery());
        assert_eq!(restored.text.as_ref().unwrap().text, "Несохранённое 🙂");
        assert_eq!(
            restored.text.as_ref().unwrap().kind,
            kind.unwrap_or(TextKind::Txt)
        );
        assert!(restored.is_dirty());
        assert!(restored.recovered_document);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| restored.show(ctx),
            );
        }
        assert_eq!(ctx.memory(|m| m.focused()), restored.text_input_id);
        assert!(!restored.request_close());
        restored.cancel_pending_action();
        assert!(!restored.save_document_to(root.join("bad.txt")));
        assert!(restored
            .recovery_session
            .as_ref()
            .unwrap()
            .restore_text()
            .is_ok());
        if let Some(source) = &source {
            assert!(!restored.save_document_to(source.clone()));
        }
        let copy = folder.path().join(format!(
            "restored.{}",
            kind.unwrap_or(TextKind::Txt).extension()
        ));
        assert!(restored.save_document_to(copy.clone()));
        assert!(!restored.is_dirty());
        assert!(!restored.recovered_document);
        assert!(restored.recovery_session.is_none());
        assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
        assert_eq!(TextDocument::open(&copy).unwrap().text, "Несохранённое 🙂");
        if let Some(source) = source {
            assert_eq!(std::fs::read(source).unwrap(), before.unwrap());
        }
    }
}

#[test]
fn text_recovery_error_pending_empty_edits_and_dont_save_close_keep_correct_state() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("recovery");
    let mut app = NotOfficeApp::blank_text();
    app.initialize_recovery(root.clone());
    app.text.as_mut().unwrap().text = "First".into();
    app.sync_recovery(true);
    app.text.as_mut().unwrap().text = "Latest".into();
    app.sync_recovery(false);
    assert!(app.recovery_notice.starts_with("Ожидание восстановления"));
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore_text()
            .unwrap()
            .0
            .text,
        "First"
    );
    let session = app.recovery_session.as_ref().unwrap();
    let blocked = session.directory().join("pending-00000000000000000002.tmp");
    std::fs::create_dir(&blocked).unwrap();
    app.sync_recovery(true);
    assert!(app.recovery_notice.starts_with("ОШИБКА записи"));
    assert_eq!(app.text.as_ref().unwrap().text, "Latest");
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore_text()
            .unwrap()
            .0
            .text,
        "First"
    );
    std::fs::remove_dir(blocked).unwrap();
    app.text.as_mut().unwrap().text.clear();
    app.sync_recovery(true);
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore_text()
            .unwrap()
            .0
            .text,
        ""
    );
    drop(app);
    let mut restarted = NotOfficeApp::blank_text();
    restarted.initialize_recovery(root.clone());
    assert!(restarted.restore_first_recovery());
    assert!(restarted.is_dirty());
    assert_eq!(restarted.text.as_ref().unwrap().text, "");
    assert!(!restarted.request_close());
    restarted.cancel_pending_action();
    assert!(restarted.recovery_session.is_some());
    assert!(!restarted.request_close());
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        restarted.resume_pending_action(ctx)
    });
    assert!(restarted.close_approved);
    assert!(restarted.recovery_session.is_none());
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
}

#[test]
fn clean_text_close_after_undo_does_not_offer_a_stale_checkpoint() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("recovery");
    let mut app = NotOfficeApp::blank_text();
    app.initialize_recovery(root.clone());
    app.text.as_mut().unwrap().text = "Undo this edit".into();
    app.sync_recovery(true);
    app.text.as_mut().unwrap().text.clear();
    assert!(!app.is_dirty());
    assert!(app.request_close());
    drop(app);
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
}

#[test]
fn workbook_crash_recovery_restores_edited_cells_and_requires_copy() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("book.xlsx");
    std::fs::write(&source, b"protected original").unwrap();
    let root = folder.path().join("recovery");
    let mut app = NotOfficeApp {
        opened_path: Some(source.clone()),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                cells: vec![liteoffice::xlsx::SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "before".into(),
                }],
                row_count: 1,
                column_count: 1,
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    app.initialize_recovery(root.clone());
    app.spreadsheet.as_mut().unwrap().sheets[0].cells[0].value = "recovered value".into();
    app.sync_recovery(true);
    assert!(app.recovery_session.as_ref().unwrap().is_workbook());
    drop(app);

    let mut restored = NotOfficeApp::default();
    restored.initialize_recovery(root);
    assert_eq!(restored.recovery_candidates.len(), 1);
    assert!(restored.restore_first_recovery());
    assert!(restored.recovered_document);
    assert_eq!(
        restored.spreadsheet.as_ref().unwrap().sheets[0].cells[0].value,
        "recovered value"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"protected original");

    let copy = folder.path().join("restored.xlsx");
    assert!(restored.save_workbook_to(copy.clone()));
    assert!(!restored.is_dirty());
    assert_eq!(
        liteoffice::xlsx::XlsxPackage::open(&copy).unwrap().sheets[0].cells[0].value,
        "recovered value"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"protected original");
}

#[test]
fn workbook_undo_redo_restores_values_and_formatting() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("book.xlsx");
    std::fs::write(&source, b"protected original").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                cells: vec![liteoffice::xlsx::SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "before".into(),
                }],
                row_count: 1,
                column_count: 1,
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    let before = app.workbook_snapshot();
    app.spreadsheet.as_mut().unwrap().sheets[0].cells[0].value = "after".into();
    app.spreadsheet.as_mut().unwrap().formats.insert(
        (0, 0, 0),
        CellFormat {
            size: 18,
            bold: true,
            fill: None,
        },
    );
    app.record_workbook_edit(before);
    assert_eq!(app.workbook_undo.len(), 1);
    assert!(app.can_undo());
    app.undo();
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cells[0].value,
        "before"
    );
    assert!(app.spreadsheet.as_ref().unwrap().formats.is_empty());
    assert!(app.can_redo());
    app.redo();
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cells[0].value,
        "after"
    );
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().formats[&(0, 0, 0)],
        CellFormat {
            size: 18,
            bold: true,
            fill: None
        }
    );
}

#[test]
fn paste_range_is_one_undo_values_only_and_protects_read_only_source() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("paste-source.xlsx");
    let original = b"protected original";
    std::fs::write(&source, original).unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source.clone()),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Данные".into(),
                cells: vec![liteoffice::xlsx::SpreadsheetCell {
                    row: 2,
                    column: 1,
                    value: "старое".into(),
                }],
                row_count: 3,
                column_count: 2,
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    app.spreadsheet.as_mut().unwrap().selected_cell = Some((2, 1));
    let undo_before = app.workbook_undo.len();
    let (rows, columns) = app
        .paste_range_text("A\tB\r\n=1+1\t\r\nC\t\tD\r\n")
        .unwrap();
    assert_eq!((rows, columns), (3, 3));
    assert_eq!(app.workbook_undo.len(), undo_before + 1);
    for (row, column, expected) in [
        (2, 1, "A"),
        (2, 2, "B"),
        (2, 3, ""),
        (3, 1, "=1+1"),
        (3, 2, ""),
        (3, 3, ""),
        (4, 1, "C"),
        (4, 2, ""),
        (4, 3, "D"),
    ] {
        assert_eq!(
            app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(row, column),
            expected,
            "{row},{column}"
        );
    }
    assert_eq!(std::fs::read(&source).unwrap(), original);
    app.undo();
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(2, 1),
        "старое"
    );
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(3, 1),
        ""
    );

    let mut read_only = NotOfficeApp {
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel::default(),
        ])),
        ..Default::default()
    };
    assert_eq!(
        read_only.paste_range_text("X\tY"),
        Err(liteoffice::range_paste::RangePasteError::ReadOnly)
    );
    assert!(read_only.workbook_undo.is_empty());
    assert_eq!(
        read_only.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 0),
        ""
    );

    let too_many = "x\t".repeat(liteoffice::range_paste::MAX_PASTE_COLUMNS + 1);
    assert!(app.paste_range_text(&too_many).is_err());
    assert_eq!(app.workbook_undo.len(), undo_before);
}

#[test]
fn workbook_replace_all_is_one_undo_transaction() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("book.xlsx");
    std::fs::write(&source, b"protected original").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                cells: vec![
                    liteoffice::xlsx::SpreadsheetCell {
                        row: 0,
                        column: 0,
                        value: "foo and foo".into(),
                    },
                    liteoffice::xlsx::SpreadsheetCell {
                        row: 1,
                        column: 0,
                        value: "FOO".into(),
                    },
                ],
                row_count: 2,
                column_count: 1,
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    app.search_query = "foo".into();
    app.replacement_text = "bar".into();
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 3);
    app.replace_spreadsheet(true);
    let values = |app: &NotOfficeApp| {
        app.spreadsheet.as_ref().unwrap().sheets[0]
            .cells
            .iter()
            .map(|cell| cell.value.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        values(&app),
        vec!["bar and bar".to_owned(), "bar".to_owned()]
    );
    assert_eq!(app.workbook_undo.len(), 1);
    app.undo();
    assert_eq!(
        values(&app),
        vec!["foo and foo".to_owned(), "FOO".to_owned()]
    );
    app.redo();
    assert_eq!(
        values(&app),
        vec!["bar and bar".to_owned(), "bar".to_owned()]
    );
}

#[test]
fn text_replace_all_is_one_undo_transaction() {
    let mut app = NotOfficeApp::blank_text();
    app.text.as_mut().unwrap().text = "foo and foo".into();
    app.search_query = "foo".into();
    app.replacement_text = "bar".into();
    app.refresh_search();
    assert_eq!(app.search_results.hits.len(), 2);
    app.replace_text(true);
    assert_eq!(app.text.as_ref().unwrap().text, "bar and bar");
    assert_eq!(app.text_undo.len(), 1);
    app.undo();
    assert_eq!(app.text.as_ref().unwrap().text, "foo and foo");
    app.redo();
    assert_eq!(app.text.as_ref().unwrap().text, "bar and bar");
}

#[cfg(windows)]
#[test]
fn docx_pdf_export_writes_a_valid_pdf() {
    let folder = tempfile::tempdir().unwrap();
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("evidence/print-pdf/print-regression.docx");
    let package = DocxPackage::open(&input).unwrap();
    let settings = print::PageSettings::default();
    let blocks = docx_print_blocks(&package.model, &package.images, settings);
    let manifest = print::spool_manifest(&blocks, settings).unwrap();
    let output = folder.path().join("document.pdf");
    assert_eq!(
        print::run_windows_print_helper(&manifest, Some(&output)).unwrap(),
        print::PrintOutcome::Printed
    );
    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() > 1000);
}

#[cfg(windows)]
#[test]
fn spreadsheet_pdf_export_paginates_and_writes_a_valid_pdf() {
    let folder = tempfile::tempdir().unwrap();
    let sheets = vec![liteoffice::xlsx::SpreadsheetModel {
        sheet_name: "Data".into(),
        cells: (0..60)
            .map(|row| liteoffice::xlsx::SpreadsheetCell {
                row,
                column: 0,
                value: format!("Row {row}"),
            })
            .collect(),
        row_count: 60,
        column_count: 1,
    }];
    let book = SpreadsheetDocument::from_sheets(sheets);
    let settings = print::PageSettings::default();
    let blocks = spreadsheet_print_blocks(&book, settings);
    assert!(blocks
        .iter()
        .any(|block| matches!(block, print::PrintBlock::PageBreak)));
    assert_eq!(
        blocks
            .iter()
            .filter(|block| matches!(block, print::PrintBlock::TableRow(_)))
            .count(),
        60
    );
    let manifest = print::spool_manifest(&blocks, settings).unwrap();
    let output = folder.path().join("workbook.pdf");
    assert_eq!(
        print::run_windows_print_helper(&manifest, Some(&output)).unwrap(),
        print::PrintOutcome::Printed
    );
    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() > 1000);
}
