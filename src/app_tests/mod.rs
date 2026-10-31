use super::*;
use liteoffice::docx::{DocumentBlock, DocumentModel, Paragraph, TableRow, TextFragment};
use std::fs::File;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn paragraph(text: &str) -> Paragraph {
    Paragraph {
        fragments: vec![TextFragment {
            id: 0,
            text: text.into(),
            run_id: 0,
            font_size_half_points: None,
            format: Default::default(),
        }],
        ..Paragraph::default()
    }
}

fn document(paragraphs: Vec<Paragraph>) -> DocumentModel {
    DocumentModel {
        blocks: (0..paragraphs.len())
            .map(DocumentBlock::Paragraph)
            .collect(),
        paragraphs,
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn theme_switches_are_local_and_do_not_touch_document_history_or_print_settings() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("theme-source.docx");
    write_docx_fixture(&path, "Исходный текст");
    let package = DocxPackage::open(&path).unwrap();
    let model = package.model.clone();
    let mut first = NotOfficeApp::default();
    first.theme = Theme::Light;
    first.docx = Some(package);
    first.opened_path = Some(path.clone());
    first.saved_docx_model = Some(model.clone());
    first.print_settings.margin_mm = 19.0;
    first.zoom = 1.25;
    let dirty = first.is_dirty();
    let undo = first.undo_stack.len();
    let pages = first.print_settings;
    let original_bytes = std::fs::read(&path).unwrap();

    first.set_theme(Theme::DarkBlue);
    assert_eq!(first.theme, Theme::DarkBlue);
    assert_eq!(first.docx.as_ref().unwrap().model, model);
    assert_eq!(first.is_dirty(), dirty);
    assert_eq!(first.undo_stack.len(), undo);
    assert_eq!(first.print_settings, pages);
    assert!((first.zoom - 1.25).abs() < f32::EPSILON);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

    let mut second = NotOfficeApp::default();
    second.theme = Theme::Parchment;
    assert_eq!(first.theme, Theme::DarkBlue);
    assert_eq!(second.theme, Theme::Parchment);
    first.set_theme(Theme::Light);
    assert_eq!(second.theme, Theme::Parchment);
}

#[test]
fn ctrl_wheel_zoom_uses_factor_direction_and_existing_bounds() {
    let zoomed_in = ctrl_wheel_document_zoom(1.25, true, true, false, 1.10).unwrap();
    let zoomed_out = ctrl_wheel_document_zoom(1.25, true, true, false, 0.90).unwrap();
    assert!(zoomed_in > 1.25);
    assert!(zoomed_out < 1.25);
    assert_eq!(
        ctrl_wheel_document_zoom(MAX_VIEW_ZOOM, true, true, false, 1.5),
        Some(MAX_VIEW_ZOOM)
    );
    assert_eq!(
        ctrl_wheel_document_zoom(MIN_VIEW_ZOOM, true, true, false, 0.5),
        Some(MIN_VIEW_ZOOM)
    );
}

#[test]
fn ctrl_s_requests_docx_save_but_not_toolbar_text_save() {
    assert!(ctrl_s_save_requested(true, true, true, false));
    assert!(!ctrl_s_save_requested(false, true, true, false));
    assert!(!ctrl_s_save_requested(true, false, true, false));
    assert!(!ctrl_s_save_requested(true, true, false, false));
    assert!(!ctrl_s_save_requested(true, true, true, true));
}

#[test]
fn ctrl_wheel_zoom_does_not_handle_plain_scroll_or_input_widgets() {
    assert_eq!(
        ctrl_wheel_document_zoom(1.25, false, true, false, 1.1),
        None
    );
    assert_eq!(
        ctrl_wheel_document_zoom(1.25, true, false, false, 1.1),
        None
    );
    assert_eq!(ctrl_wheel_document_zoom(1.25, true, true, true, 1.1), None);
    assert_eq!(ctrl_wheel_document_zoom(1.25, true, true, false, 1.0), None);
}

#[test]
fn ctrl_wheel_zoom_values_remain_independent_per_window() {
    let first_window_zoom = ctrl_wheel_document_zoom(1.0, true, true, false, 1.2).unwrap();
    let second_window_zoom = ctrl_wheel_document_zoom(2.0, true, true, false, 0.8).unwrap();
    assert_eq!(first_window_zoom, 1.2);
    assert_eq!(second_window_zoom, 1.6);
}

fn write_docx_fixture(path: &Path, text: &str) {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
    );
    write_docx_xml_fixture(path, &xml);
}

fn write_docx_xml_fixture(path: &Path, xml: &str) {
    let file = File::create(path).unwrap();
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("word/document.xml", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(xml.as_bytes()).unwrap();
    archive.finish().unwrap();
}

fn write_print_fixture(path: &Path) {
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><w:body>
<w:p><w:r><w:t>Кириллица: Ёжик. PAGE-ONE</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblBorders><w:top w:val="single"/><w:bottom w:val="single"/><w:left w:val="single"/><w:right w:val="single"/><w:insideH w:val="single"/><w:insideV w:val="single"/></w:tblBorders></w:tblPr><w:tr><w:tc><w:p><w:r><w:t>Левая</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Правая</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1828800" cy="914400"/><a:graphic><a:blip r:embed="picture"/></a:graphic></wp:inline></w:drawing><w:br w:type="page"/></w:r></w:p>
<w:p><w:r><w:t>СТРАНИЦА 2. PAGE-TWO</w:t></w:r></w:p>
</w:body></w:document>"#;
    let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="picture" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/picture.png"/></Relationships>"#;
    let types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut archive = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in [
        ("word/document.xml", xml.as_bytes()),
        ("word/_rels/document.xml.rels", rels.as_slice()),
        ("[Content_Types].xml", types.as_slice()),
        ("_rels/.rels", root_rels.as_slice()),
        (
            "word/media/picture.png",
            include_bytes!("officeart_test.png").as_slice(),
        ),
        (
            "word/media/unused.png",
            include_bytes!("officeart_test.png").as_slice(),
        ),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();
}

#[test]
fn docx_fixture_preview_and_print_keep_the_same_image_page_and_size_at_all_zooms() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("print.docx");
    write_print_fixture(&path);
    let package = DocxPackage::open(&path).unwrap();
    assert_eq!(
        package.images.len(),
        1,
        "unreferenced media must not be rendered"
    );
    for landscape in [false, true] {
        for margin_mm in [10.0, 25.0, 40.0] {
            let settings = print::PageSettings {
                landscape,
                margin_mm,
                image_scale: print::ImageScale::OriginalSize,
                ..Default::default()
            };
            let (flow, ranges) = docx_preview_plan(&package.model, &package.images, settings, 1.0);
            assert_eq!(ranges.len(), 2);
            for zoom in [0.5, 1.0, 2.0, 4.0] {
                let (_, zoom_ranges) =
                    docx_preview_plan(&package.model, &package.images, settings, zoom);
                assert_eq!(ranges, zoom_ranges, "view zoom changed pagination");
            }
            assert!(ranges[0].clone().any(|index| matches!(
                flow[index],
                FlowBlock::Image {
                    size_pt: [144.0, 72.0],
                    ..
                }
            )));
            let blocks = docx_print_blocks(&package.model, &package.images, settings);
            let break_index = blocks
                .iter()
                .position(|block| matches!(block, print::PrintBlock::PageBreak))
                .unwrap();
            let image_index = blocks
                .iter()
                .position(|block| {
                    matches!(
                        block,
                        print::PrintBlock::SizedImage {
                            width_pt: 144.0,
                            height_pt: 72.0,
                            ..
                        }
                    )
                })
                .unwrap();
            assert!(
                image_index < break_index,
                "image moved past the source page break"
            );
            assert!(blocks.iter().any(|block| matches!(block, print::PrintBlock::TableRow(cells) if cells == &["Левая", "Правая"])));
        }
    }
}

#[test]
fn screen_preview_paints_the_docx_image_and_scales_it_without_editing_the_model() {
    let ctx = egui::Context::default();
    let texture = decode_document_image(
        &ctx,
        include_bytes!("officeart_test.png"),
        "preview-test".into(),
    )
    .unwrap();
    for zoom in [0.5, 1.0, 2.0] {
        let mut rect = egui::Rect::NOTHING;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = render_docx_image(ui, Some(&texture), [144.0, 72.0], zoom).rect;
            });
        });
        assert_eq!(rect.size(), egui::vec2(144.0, 72.0) * zoom);
        let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
        assert!(meshes.iter().any(|primitive| matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture.id() && !mesh.vertices.is_empty())));
    }
    assert!(decode_document_image(&ctx, b"broken image", "broken".into()).is_none());
}

#[test]
#[ignore = "read-only check of a local DOC selected by LITEOFFICE_DOC_PREVIEW_TEST_INPUT"]
fn local_legacy_doc_preview_image_regression() {
    let path =
        std::env::var_os("LITEOFFICE_DOC_PREVIEW_TEST_INPUT").expect("select a local test DOC");
    let package = LegacyDocPackage::open(Path::new(&path)).unwrap();
    assert!(!package.images.is_empty());
    let ctx = egui::Context::default();
    let textures = decode_doc_gallery(&ctx, &package);
    assert_eq!(textures.len(), package.images.len());
    assert!(
        textures.iter().all(Option::is_some),
        "at least one recovered raster cannot be decoded for preview"
    );
    let mut anchors = vec![None; textures.len()];
    let mut widths = vec![85.0; textures.len()];
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(3000.0, 30000.0),
        )),
        ..Default::default()
    };
    let output = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_legacy_doc(
                ui,
                &package,
                &textures,
                &mut anchors,
                &mut widths,
                &LegacyViewOptions {
                    zoom: 0.5,
                    page_settings: print::PageSettings::default(),
                    query: "",
                    current_hit: None,
                    scroll_to_hit: false,
                },
            );
        });
    });
    let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
    for texture in textures.iter().flatten() {
        assert!(meshes.iter().any(|primitive| matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture.id() && !mesh.vertices.is_empty())), "recovered image was omitted from the gallery");
    }
    println!(
        "DOC_PREVIEW: {} recovered images decoded and painted",
        textures.len()
    );
}

#[cfg(windows)]
#[test]
#[ignore = "requires Microsoft Print to PDF; writes DOCX/PDF evidence under manual_phase3_artifacts"]
fn windows_pdf_fixture_regression() {
    use std::os::windows::process::CommandExt;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("manual_phase3_artifacts")
        .join(format!("automated-{seed}"));
    std::fs::create_dir_all(&folder).unwrap();
    let input = folder.join("print-regression.docx");
    write_print_fixture(&input);
    let original_input = std::fs::read(&input).unwrap();
    let package = DocxPackage::open(&input).unwrap();
    for (name, landscape, margin_mm, numbers) in [
        ("portrait-10mm.pdf", false, 10.0, true),
        ("landscape-25mm.pdf", true, 25.0, true),
        ("portrait-no-numbers.pdf", false, 10.0, false),
    ] {
        let settings = print::PageSettings {
            landscape,
            margin_mm,
            print_page_numbers: numbers,
            image_scale: print::ImageScale::OriginalSize,
        };
        let blocks = docx_print_blocks(&package.model, &package.images, settings);
        let manifest_path = folder.join(format!("{name}.manifest"));
        std::fs::write(
            &manifest_path,
            print::spool_manifest(&blocks, settings).unwrap(),
        )
        .unwrap();
        let output_path = folder.join(name);
        let manifest = std::fs::read_to_string(&manifest_path).unwrap();
        assert_eq!(
            print::run_windows_print_helper(&manifest, Some(&output_path)).unwrap(),
            print::PrintOutcome::Printed,
        );
        assert!(std::fs::metadata(&output_path).unwrap().len() > 1000);
    }
    let verification = std::process::Command::new("python")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/verify_phase3_pdf.py"))
        .arg(&folder)
        .arg("--render")
        .creation_flags(0x08000000)
        .output()
        .expect("PDF verification needs Python with PyMuPDF installed");
    assert!(
        verification.status.success(),
        "{} {}",
        String::from_utf8_lossy(&verification.stdout),
        String::from_utf8_lossy(&verification.stderr)
    );
    assert_eq!(
        std::fs::read(&input).unwrap(),
        original_input,
        "printing mutated the source DOCX"
    );
    println!("{}", String::from_utf8_lossy(&verification.stdout));
    println!("PDF_EVIDENCE={}", folder.display());
}

fn loaded_docx_text(app: &NotOfficeApp) -> String {
    app.docx
        .as_ref()
        .and_then(|package| package.model.paragraphs.first())
        .and_then(|paragraph| paragraph.fragments.first())
        .map(|fragment| fragment.text.clone())
        .expect("a DOCX paragraph should be active")
}

fn edit_first_text(app: &mut NotOfficeApp, text: &str) {
    app.docx
        .as_mut()
        .unwrap()
        .model
        .fragment_mut(0)
        .unwrap()
        .text = text.into();
}

#[test]
fn crash_restart_restores_dirty_copy_without_overwriting_source_and_save_clears_checkpoint() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("original.docx");
    let root = folder.path().join("recovery");
    write_docx_fixture(&path, "Original");
    let original_bytes = std::fs::read(&path).unwrap();
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root.clone());
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "Recovered Ёжик");
    app.sync_recovery(true);
    assert!(app.recovery_session.is_some());
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    drop(app);
    let mut restarted = NotOfficeApp::default();
    restarted.initialize_recovery(root.clone());
    assert_eq!(restarted.recovery_candidates.len(), 1);
    assert!(restarted.restore_first_recovery());
    assert!(restarted.is_dirty());
    assert!(restarted.recovered_document);
    assert_eq!(loaded_docx_text(&restarted), "Recovered Ёжик");
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert!(!restarted.save_document_to(root.join("bad.docx")));
    let copy = folder.path().join("recovered-copy.docx");
    assert!(restarted.save_document_to(copy.clone()));
    assert!(!restarted.is_dirty());
    assert!(!restarted.recovered_document);
    assert_eq!(std::fs::read(path).unwrap(), original_bytes);
    drop(restarted);
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
    assert_eq!(
        DocxPackage::open(copy)
            .unwrap()
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Recovered Ёжик"
    );
}

#[test]
fn failed_save_and_cancel_keep_recovery_but_explicit_discard_close_removes_it() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("source.docx");
    let root = folder.path().join("recovery");
    write_docx_fixture(&path, "Original");
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root.clone());
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "Unsaved");
    app.sync_recovery(true);
    let session_dir = app
        .recovery_session
        .as_ref()
        .unwrap()
        .directory()
        .to_path_buf();
    assert!(!app.request_close());
    app.cancel_pending_action();
    assert!(session_dir.read_dir().unwrap().count() >= 3);
    assert!(!app.save_document_to(folder.path().join("missing").join("copy.docx")));
    assert!(app.recovery_session.as_ref().unwrap().restore().is_ok());
    assert!(app.is_dirty());
    assert!(!app.request_close());
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert!(app.close_approved);
    drop(app);
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
    assert_eq!(
        DocxPackage::open(path)
            .unwrap()
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Original"
    );
}

#[test]
fn postpone_and_invalid_recovery_keep_files_until_explicit_rejection() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("recovery");
    let session = liteoffice::recovery::Session::create(&root, Path::new("lost.docx")).unwrap();
    let invalid = session
        .directory()
        .join("snapshot-00000000000000000001.docx");
    std::fs::write(&invalid, b"broken").unwrap();
    drop(session);
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root.clone());
    assert!(!app.restore_first_recovery());
    assert!(invalid.exists());
    assert_eq!(app.recovery_candidates.len(), 1);
    app.recovery_candidates.clear(); // Later: release claim, no deletion
    app.initialize_recovery(root.clone());
    assert_eq!(app.recovery_candidates.len(), 1);
    assert!(app.discard_first_recovery());
    assert!(!invalid.exists());
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
}

#[test]
fn recovered_images_tables_fonts_and_print_plan_survive_a_second_crash() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("mixed.docx");
    let root = folder.path().join("recovery");
    write_print_fixture(&source);
    let original = std::fs::read(&source).unwrap();
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root.clone());
    app.request_open_path(source.clone());
    edit_first_text(&mut app, "Несохранённый документ Ё");
    app.docx.as_mut().unwrap().model.set_all_run_font_sizes(30);
    let expected_model = app.docx.as_ref().unwrap().model.clone();
    let package = app.docx.as_ref().unwrap();
    let expected_print = docx_print_blocks(
        &package.model,
        &package.images,
        print::PageSettings::default(),
    );
    app.sync_recovery(true);
    drop(app);
    for _ in 0..2 {
        let mut restored = NotOfficeApp::default();
        restored.initialize_recovery(root.clone());
        assert!(restored.restore_first_recovery());
        assert!(restored.recovered_document);
        assert!(restored.is_dirty());
        assert_eq!(restored.docx.as_ref().unwrap().model, expected_model);
        assert_eq!(
            docx_print_blocks(
                &restored.docx.as_ref().unwrap().model,
                &restored.docx.as_ref().unwrap().images,
                print::PageSettings::default()
            ),
            expected_print
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
        // Dropping before Save is a second interrupted session. The same
        // durable recovery must be offered again, not consumed by Restore.
    }
}

#[test]
fn recovery_timer_tracks_latest_edit_and_save_error_is_visible_without_losing_model() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    let root = folder.path().join("recovery");
    write_docx_fixture(&source, "Original");
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root);
    app.request_open_path(source);
    edit_first_text(&mut app, "First");
    app.sync_recovery(false);
    edit_first_text(&mut app, "Second");
    app.sync_recovery(false); // throttled; not claimed as committed yet
    assert!(app.recovery_notice.starts_with("Ожидание восстановления"));
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore()
            .unwrap()
            .0
            .model
            .fragment(0)
            .unwrap()
            .text,
        "First"
    );
    app.recovery_last_attempt = Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
    app.sync_recovery(false);
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore()
            .unwrap()
            .0
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Second"
    );
    let session = app.recovery_session.as_ref().unwrap();
    std::fs::create_dir(session.directory().join("pending-00000000000000000003.tmp")).unwrap();
    edit_first_text(&mut app, "Only in memory");
    app.sync_recovery(true);
    assert!(app.recovery_notice.contains("ОШИБКА"));
    assert!(app.is_dirty());
    assert_eq!(loaded_docx_text(&app), "Only in memory");
    assert_eq!(
        app.recovery_session
            .as_ref()
            .unwrap()
            .restore()
            .unwrap()
            .0
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Second"
    );
}

#[test]
fn failed_replacement_keeps_recovery_and_successful_discard_open_retires_it() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    let broken = folder.path().join("broken.docx");
    let replacement = folder.path().join("replacement.docx");
    let root = folder.path().join("recovery");
    write_docx_fixture(&source, "Original");
    write_docx_fixture(&replacement, "Replacement");
    std::fs::write(&broken, b"broken ZIP").unwrap();
    let mut app = NotOfficeApp::default();
    app.initialize_recovery(root.clone());
    app.request_open_path(source);
    edit_first_text(&mut app, "Unsaved");
    app.sync_recovery(true);
    app.request_open_path(broken);
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert_eq!(loaded_docx_text(&app), "Unsaved");
    assert!(app.recovery_session.as_ref().unwrap().restore().is_ok());
    app.request_open_path(replacement);
    let _ = ctx.run(Default::default(), |ctx| app.resume_pending_action(ctx));
    assert_eq!(loaded_docx_text(&app), "Replacement");
    assert!(app.recovery_session.is_none());
    assert!(liteoffice::recovery::discover(&root).unwrap().0.is_empty());
}

#[test]
fn normal_save_refreshes_baseline_and_can_be_repeated_without_a_dialog() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("normal.docx");
    write_docx_fixture(&path, "Initial");
    let mut app = NotOfficeApp::default();
    app.request_open_path(path.clone());
    for text in ["Saved first", "Saved second"] {
        edit_first_text(&mut app, text);
        assert!(app.is_dirty());
        assert!(app.save_document());
        assert!(!app.is_dirty());
        assert_eq!(
            DocxPackage::open(&path)
                .unwrap()
                .model
                .fragment(0)
                .unwrap()
                .text,
            text
        );
        assert_eq!(app.opened_path.as_ref(), Some(&path));
    }
}

#[test]
fn converted_doc_undo_then_redo_restores_the_edit() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("original.doc");
    std::fs::write(&source, b"source must stay unchanged").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        legacy_doc: Some(LegacyDocPackage {
            paragraphs: vec!["Кот".into()],
            ..Default::default()
        }),
        ..Default::default()
    };
    app.edit_legacy_as_docx();
    for text in ["Первая правка", "Вторая правка"] {
        let before = app.docx.as_ref().unwrap().model.clone();
        edit_first_text(&mut app, text);
        app.record_docx_edit(before);
    }
    assert!(app.can_undo());
    app.undo();
    assert_eq!(loaded_docx_text(&app), "Первая правка");
    assert!(app.can_redo());
    app.redo();
    assert_eq!(loaded_docx_text(&app), "Вторая правка");
    assert!(!app.can_redo());
}

#[test]
fn legacy_conversion_uses_existing_edit_guards_and_independent_docx_save_baselines() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("original.doc");
    std::fs::write(&source, b"source must stay unchanged").unwrap();
    let make_app = || NotOfficeApp {
        opened_path: Some(source.clone()),
        legacy_doc: Some(LegacyDocPackage {
            paragraphs: vec!["Кот".into()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut first = make_app();
    let mut second = make_app();
    first.edit_legacy_as_docx();
    second.edit_legacy_as_docx();
    assert!(first.is_dirty());
    assert!(first.docx_focus_pending);
    assert!(!first.request_close());
    first.cancel_pending_action();
    edit_first_text(&mut first, "Первое окно");
    assert_eq!(loaded_docx_text(&second), "Кот");
    assert!(!first.save_document_to(source.clone()));
    assert!(first.is_dirty());
    let target = folder.path().join("copy.docx");
    assert!(first.save_document_to(target.clone()));
    assert!(!first.is_dirty());
    assert_eq!(first.opened_path.as_ref(), Some(&target));
    edit_first_text(&mut first, "Новое изменение первого окна");
    assert!(first.save_document());
    second.open_path(target.clone());
    edit_first_text(&mut second, "Второе окно");
    edit_first_text(&mut first, "Внешнее изменение");
    assert!(first.save_document());
    assert!(!second.save_document_to(target));
    assert!(second.conflict_destination.is_some());
    assert_eq!(loaded_docx_text(&second), "Второе окно");
    assert_eq!(
        std::fs::read(source).unwrap(),
        b"source must stay unchanged"
    );
}

#[test]
fn workbook_edits_close_guards_save_baselines_and_two_window_conflicts_are_independent() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("original.xls");
    std::fs::write(&source, b"protected input").unwrap();
    let mut first = NotOfficeApp {
        opened_path: Some(source.clone()),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                ..Default::default()
            },
        ])),
        ..Default::default()
    };
    first.edit_workbook_copy();
    assert!(first.is_dirty());
    assert!(!first.request_close());
    first.cancel_pending_action();
    first
        .spreadsheet
        .as_mut()
        .unwrap()
        .set_cell_text(0, 0, "Первое окно".into());
    assert!(!first.save_document_to(source.clone()));
    let target = folder.path().join("result.xlsx");
    assert!(first.save_document_to(target.clone()));
    assert!(!first.is_dirty());
    let mut second = NotOfficeApp::default();
    second.open_path(target.clone());
    second.edit_workbook_copy();
    let second_target = folder.path().join("second.xlsx");
    assert!(second.save_document_to(second_target.clone()));
    // Open the same exported target as another editable session, then
    // exercise independent state and external-edit conflict detection.
    let package = XlsxPackage::open(&second_target).unwrap();
    let mut competitor =
        liteoffice::workbook_edit::WorkbookEditor::new(source.clone(), &package.sheets);
    competitor.save(&target, &package.sheets).unwrap();
    first
        .spreadsheet
        .as_mut()
        .unwrap()
        .set_cell_text(0, 0, "Правки первого".into());
    assert_eq!(
        second.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 0),
        "Первое окно"
    );
    assert!(!first.save_document());
    assert!(first.conflict_destination.is_some());
    assert!(first.is_dirty());
    assert_eq!(std::fs::read(source).unwrap(), b"protected input");
}

#[test]
fn full_ui_workbook_literal_input_receives_typing_and_cell_selection_has_separate_undo_ids() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("test.xls");
    std::fs::write(&source, b"protected").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                ..Default::default()
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    // Grid's first sizing pass is disabled; retain pending focus until its
    // first interactive pass, rather than focusing an unavailable widget.
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    let id = app.spreadsheet.as_ref().unwrap().input_id;
    assert_eq!(ctx.memory(|m| m.focused()), id);
    let input = egui::RawInput {
        events: vec![egui::Event::Text("Ёжик".into())],
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| app.show(ctx));
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 0),
        "Ёжик"
    );
    app.spreadsheet.as_mut().unwrap().selected_cell = Some((0, 1));
    let _ = ctx.run(egui::RawInput::default(), |ctx| app.show(ctx));
    assert_ne!(app.spreadsheet.as_ref().unwrap().input_id, id);
}

#[test]
fn typing_a_formula_is_one_workbook_undo_transaction() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("formula-undo.xls");
    std::fs::write(&source, b"protected").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                ..Default::default()
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    let ctx = egui::Context::default();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    for _ in 0..4 {
        let _ = ctx.run(input(vec![]), |ctx| app.show(ctx));
    }
    let id = app.spreadsheet.as_ref().unwrap().input_id;
    assert_eq!(ctx.memory(|m| m.focused()), id);
    for character in ["=", "3", "*", "4"] {
        let _ = ctx.run(input(vec![egui::Event::Text(character.into())]), |ctx| {
            app.show(ctx)
        });
    }
    let enter = egui::Modifiers::NONE;
    let _ = ctx.run(
        input(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: enter,
        }]),
        |ctx| app.show(ctx),
    );
    assert_eq!(app.workbook_undo.len(), 1);
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 0),
        "12"
    );
    app.undo();
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 0),
        ""
    );
}

#[test]
fn range_selection_does_not_leave_an_active_working_cell() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("range-active.xls");
    std::fs::write(&source, b"protected").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                row_count: 2,
                column_count: 2,
                ..Default::default()
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    {
        let book = app.spreadsheet.as_mut().unwrap();
        book.selected_cell = Some((0, 0));
        book.selection.range_start = Some((0, 0));
        book.selection.range_end = Some((0, 1));
    }
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert_eq!(app.spreadsheet.as_ref().unwrap().selected_cell, None);
}

#[test]
fn inline_workbook_mouse_typing_tab_enter_and_save_reopen_keep_cell_values() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("original.xls");
    std::fs::write(&source, b"protected").unwrap();
    let mut app = NotOfficeApp {
        opened_path: Some(source.clone()),
        spreadsheet: Some(SpreadsheetDocument::from_sheets(vec![
            liteoffice::xlsx::SpreadsheetModel {
                sheet_name: "Data".into(),
                ..Default::default()
            },
        ])),
        ..Default::default()
    };
    app.edit_workbook_copy();
    let ctx = egui::Context::default();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    for _ in 0..5 {
        let _ = ctx.run(input(vec![]), |ctx| app.show(ctx));
    }
    let generation = app.docx_editor_generation;
    let b1 = egui::Id::new(("workbook-inline", generation, 0_usize, 0_usize, 1_usize));
    let rect = ctx.read_response(b1).unwrap().rect;
    assert!(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)).contains_rect(rect)
    );
    for pressed in [true, false] {
        let _ = ctx.run(
            input(vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]),
            |ctx| app.show(ctx),
        );
    }
    let _ = ctx.run(input(vec![]), |ctx| app.show(ctx));
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().selected_cell,
        Some((0, 1))
    );
    assert_eq!(ctx.memory(|m| m.focused()), Some(b1));
    let _ = ctx.run(input(vec![egui::Event::Text("Ёжик".into())]), |ctx| {
        app.show(ctx)
    });
    assert_eq!(
        app.spreadsheet.as_ref().unwrap().sheets[0].cell_value(0, 1),
        "Ёжик"
    );
    for (key, shift, expected) in [
        (egui::Key::Tab, false, (0, 2)),
        (egui::Key::Tab, true, (0, 1)),
        (egui::Key::Enter, false, (1, 1)),
        (egui::Key::Enter, true, (0, 1)),
    ] {
        let modifiers = if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        let mut key_input = input(vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        key_input.modifiers = modifiers;
        let _ = ctx.run(key_input, |ctx| app.show(ctx));
        let _ = ctx.run(input(vec![]), |ctx| app.show(ctx));
        assert_eq!(
            app.spreadsheet.as_ref().unwrap().selected_cell,
            Some(expected)
        );
        let id = egui::Id::new((
            "workbook-inline",
            generation,
            0_usize,
            expected.0,
            expected.1,
        ));
        assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    }
    let output = folder.path().join("inline.xlsx");
    assert!(app.save_document_to(output.clone()));
    assert_eq!(
        XlsxPackage::open(output).unwrap().sheets[0].cell_value(0, 1),
        "Ёжик"
    );
    assert_eq!(std::fs::read(source).unwrap(), b"protected");
}

#[test]
fn save_as_adopts_the_new_path_and_later_save_does_not_change_original() {
    let folder = tempfile::tempdir().unwrap();
    let original = folder.path().join("original.docx");
    let copy = folder.path().join("Копия отчёта.docx");
    write_docx_fixture(&original, "Initial");
    let original_bytes = std::fs::read(&original).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(original.clone());
    edit_first_text(&mut app, "Saved As");
    assert!(app.save_document_to(copy.clone()));
    assert_eq!(app.opened_path.as_ref(), Some(&copy));
    assert_eq!(app.docx.as_ref().unwrap().source_path(), copy);
    assert!(!app.is_dirty());
    edit_first_text(&mut app, "Next Save");
    assert!(app.save_document());
    assert_eq!(
        DocxPackage::open(&copy)
            .unwrap()
            .model
            .fragment(0)
            .unwrap()
            .text,
        "Next Save"
    );
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
}

#[test]
fn unsaved_close_requires_a_choice_and_cancel_keeps_edits() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("close.docx");
    write_docx_fixture(&path, "Initial");
    let mut app = NotOfficeApp::default();
    app.request_open_path(path);
    assert!(app.request_close());
    edit_first_text(&mut app, "Unsaved");
    assert!(!app.request_close());
    assert_eq!(app.pending_action, Some(PendingAction::Close));
    app.cancel_pending_action();
    assert!(!app.close_approved);
    assert!(app.pending_action.is_none());
    assert!(app.is_dirty());
    assert_eq!(loaded_docx_text(&app), "Unsaved");
    assert!(!app.request_close());
    assert!(app.save_document());
    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        app.resume_pending_action(ctx)
    });
    assert!(app.close_approved);
    assert!(output.viewport_output.values().any(|viewport| viewport
        .commands
        .iter()
        .any(|cmd| matches!(cmd, egui::ViewportCommand::Close))));
}

#[test]
fn failed_save_keeps_unsaved_document_and_does_not_approve_close_or_switch_source() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("failure.docx");
    write_docx_fixture(&path, "Initial");
    let original = std::fs::read(&path).unwrap();
    let mut app = NotOfficeApp::default();
    app.request_open_path(path.clone());
    edit_first_text(&mut app, "Unsaved after IO failure");
    assert!(!app.request_close());
    assert!(!app.save_document_to(folder.path().join("missing-parent/file.docx")));
    assert_eq!(app.pending_action, Some(PendingAction::Close));
    assert!(!app.close_approved);
    assert!(app.is_dirty());
    assert_eq!(app.opened_path.as_ref(), Some(&path));
    assert_eq!(loaded_docx_text(&app), "Unsaved after IO failure");
    assert_eq!(std::fs::read(path).unwrap(), original);
    assert!(app.status.contains("Не удалось сохранить"));
}

include!("app_tests_part_1.rs");
include!("app_tests_part_2.rs");
