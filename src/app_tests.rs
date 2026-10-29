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
fn unified_find_switches_sheets_and_exposes_far_cell_without_editing_workbook() {
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
    assert_eq!(app.search_results.hits.len(), 2);
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
    assert_eq!(app.spreadsheet.as_ref().unwrap().selected_sheet, 0);
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
        "Done Find",
        "toolbar Ctrl+Z must not undo the document"
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
    let _ = ctx.run(search_key(egui::Key::Z, command), |ctx| {
        app.handle_edit_shortcuts(ctx)
    });
    assert_eq!(loaded_docx_text(&app), "Find Find");
    let _ = ctx.run(search_key(egui::Key::Y, command), |ctx| {
        app.handle_edit_shortcuts(ctx)
    });
    assert_eq!(loaded_docx_text(&app), "Done Find");
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
            bold: true
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
