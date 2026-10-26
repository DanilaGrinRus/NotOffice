//! Native Windows PDF raster reader, not the deferred print/recovery regression.
#![cfg(windows)]
use liteoffice::pdf::PdfSession;
use std::fs;
use std::path::Path;

fn fixture(active: bool, marker_command: &Path) -> Vec<u8> {
    // JavaScript, Launch and form-open actions are present but must remain data.
    let action = if active {
        " /OpenAction 8 0 R /AA << /WC 9 0 R >> /Names << /JavaScript << /Names [(startup) 8 0 R] >> >>"
    } else {
        ""
    };
    let stream1 =
        "0.1 0.5 0.8 rg 30 50 240 250 re f\nBT /F1 16 Tf 40 330 Td (PDF raster page one) Tj ET\n";
    let stream2 =
        "0.8 0.4 0.2 rg 30 50 240 250 re f\nBT /F1 16 Tf 40 330 Td (PDF raster page two) Tj ET\n";
    let launch_path = marker_command
        .to_string_lossy()
        .replace('\\', "/")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R{action} >>"),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{stream1}endstream", stream1.len()),
        format!("<< /Length {} >>\nstream\n{stream2}endstream", stream2.len()),
        format!("<< /S /JavaScript /JS (app.launchURL\\(\\\"file:///{launch_path}\\\", true\\);) >>"),
        format!("<< /S /Launch /F ({launch_path}) >>"),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        pdf.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    pdf
}

fn render(session: &PdfSession, index: usize) -> Vec<u8> {
    session.request_page(index).unwrap();
    let (page, bytes) = session
        .responses
        .recv_timeout(std::time::Duration::from_secs(15))
        .unwrap();
    assert_eq!(page, index);
    bytes.unwrap()
}

fn verify(folder: &Path) {
    let marker = folder.join("ACTION_EXECUTED.txt");
    let command = folder.join("inert-action.cmd");
    // A harmless local sentinel, only ever referenced as untrusted PDF data.
    fs::write(
        &command,
        format!("@echo off\r\necho ERROR>\"{}\"\r\n", marker.display()),
    )
    .unwrap();
    let clean_path = folder.join("plain.pdf");
    let active_path = folder.join("inert-actions.pdf");
    let clean = fixture(false, &command);
    let active = fixture(true, &command);
    fs::write(&clean_path, &clean).unwrap();
    fs::write(&active_path, &active).unwrap();
    let clean_view = PdfSession::open(&clean_path).unwrap();
    let active_view = PdfSession::open(&active_path).unwrap();
    assert_eq!(active_view.pages.len(), 2);
    assert_eq!(clean_view.pages, active_view.pages);
    assert!(active_view.request_page(2).is_err());
    for index in 0..2 {
        let baseline = render(&clean_view, index);
        let actual = render(&active_view, index);
        let left = image::load_from_memory(&baseline).unwrap().into_rgba8();
        let right = image::load_from_memory(&actual).unwrap().into_rgba8();
        assert_eq!(
            left, right,
            "active PDF actions must not change raster output"
        );
        assert!(right
            .pixels()
            .any(|pixel| pixel[0] < 100 || pixel[1] < 100 || pixel[2] < 100));
        fs::write(
            folder.join(format!("native-page-{}.png", index + 1)),
            actual,
        )
        .unwrap();
    }
    drop(active_view);
    drop(clean_view);
    assert!(!marker.exists(), "embedded action was executed");
    assert_eq!(fs::read(clean_path).unwrap(), clean);
    assert_eq!(fs::read(active_path).unwrap(), active);
    fs::write(folder.join("broken.pdf"), b"%PDF-1.7\nbroken").unwrap();
    assert!(PdfSession::open(&folder.join("broken.pdf")).is_err());
}

#[test]
fn native_pdf_pages_render_without_dispatching_document_actions_or_mutating_source() {
    let folder = tempfile::tempdir().unwrap();
    verify(folder.path());
}

#[test]
fn native_pdf_launch_openaction_is_not_dispatched() {
    let folder = tempfile::tempdir().unwrap();
    let marker = folder.path().join("LAUNCH_EXECUTED.txt");
    let command = folder.path().join("inert-launch.cmd");
    fs::write(
        &command,
        format!("@echo off\r\necho ERROR>\"{}\"\r\n", marker.display()),
    )
    .unwrap();
    let mut bytes = fixture(true, &command);
    let tag = b"/OpenAction 8 0 R";
    let offset = bytes
        .windows(tag.len())
        .position(|part| part == tag)
        .unwrap();
    // Fixed-width change keeps xref offsets valid: directly select Launch,
    // rather than only the JavaScript action or a will-close action.
    bytes[offset + b"/OpenAction ".len()] = b'9';
    let path = folder.path().join("launch.pdf");
    fs::write(&path, &bytes).unwrap();
    let session = PdfSession::open(&path).unwrap();
    assert!(!render(&session, 0).is_empty());
    drop(session);
    assert!(!marker.exists(), "PDF Launch OpenAction was executed");
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
#[ignore = "export only new PDF-reader safety fixtures, no printing"]
fn export_pdf_reader_evidence() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/pdf-rc4");
    fs::create_dir(&folder).expect("never overwrite manual files");
    verify(&folder);
    println!("PDF_READER_EVIDENCE={}", folder.display());
}

#[test]
#[ignore = "read-only PDF-reader QA of an existing mixed PDF; never prints"]
fn render_existing_mixed_pdf_for_reader_qa() {
    let path = std::path::PathBuf::from(
        std::env::var_os("LITEOFFICE_PDF_VIEW_INPUT").expect("select a PDF"),
    );
    let original = fs::read(&path).unwrap();
    let session = PdfSession::open(&path).unwrap();
    assert_eq!(session.pages.len(), 2);
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/pdf-rc4");
    fs::create_dir_all(&folder).unwrap();
    for index in 0..2 {
        fs::write(
            folder.join(format!("mixed-native-page-{}.png", index + 1)),
            render(&session, index),
        )
        .unwrap();
    }
    assert_eq!(fs::read(path).unwrap(), original);
}
