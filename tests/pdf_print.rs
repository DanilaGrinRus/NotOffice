#![cfg(windows)]
use liteoffice::pdf::PdfSession;
use liteoffice::pdf_print::{print_pdf_to, PdfPrintSettings};
use std::path::Path;

fn fixture(path: &Path) {
    let blue = "0 0 1 rg 14 20 110 170 re f\n";
    let red = "1 0 0 rg 36 36 288 144 re f\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /OpenAction << /S /JavaScript /JS (NEVER_EXECUTE) >> >>"
            .to_owned(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 144 216] /Contents 5 0 R >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 360 216] /Contents 6 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{blue}endstream", blue.len()),
        format!("<< /Length {} >>\nstream\n{red}endstream", red.len()),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    std::fs::write(path, bytes).unwrap();
}

fn red_width(path: &Path, png_path: &Path) -> u32 {
    let session = PdfSession::open(path).unwrap();
    assert_eq!(session.pages.len(), 1);
    assert!(
        session.pages[0][0] > session.pages[0][1],
        "selected landscape page must remain landscape"
    );
    session.request_page(0).unwrap();
    let bytes = session
        .responses
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .1
        .unwrap();
    std::fs::write(png_path, &bytes).unwrap();
    let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let mut min = image.width();
    let mut max = 0;
    let mut min_y = image.height();
    let mut max_y = 0;
    let mut blue = 0;
    for (x, y, p) in image.enumerate_pixels() {
        if p[0] > 160 && p[1] < 100 && p[2] < 100 {
            min = min.min(x);
            max = max.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        if p[2] > 160 && p[0] < 100 && p[1] < 100 {
            blue += 1;
        }
    }
    assert_eq!(
        blue, 0,
        "page 1 blue rectangle must not print when only page 2 is selected"
    );
    assert!(max > min, "red page 2 content must be visible");
    assert!(
        ((min + max) as f32 / 2.0 - image.width() as f32 / 2.0).abs() < image.width() as f32 * 0.03,
        "content must be horizontally centered"
    );
    assert!(
        ((min_y + max_y) as f32 / 2.0 - image.height() as f32 / 2.0).abs()
            < image.height() as f32 * 0.03,
        "content must be vertically centered"
    );
    max - min
}

#[test]
fn print_snapshot_and_bad_range_fail_without_opening_a_printer_dialog() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("source.pdf");
    fixture(&path);
    let session = PdfSession::open(&path).unwrap();
    assert_eq!(session.pages.len(), 2);
    let snapshot = session.snapshot();
    std::fs::write(&path, b"external replacement").unwrap();
    assert_eq!(
        PdfSession::from_snapshot(snapshot.clone())
            .unwrap()
            .pages
            .len(),
        2
    );
    assert!(print_pdf_to(
        snapshot,
        PdfPrintSettings {
            first: 3,
            last: 3,
            percent: None
        },
        None
    )
    .is_err());
}

#[test]
#[ignore = "explicit new input-PDF print-to-PDF check only; does not run old print/recovery regression"]
fn export_and_verify_input_pdf_range_orientation_and_scale() {
    let folder =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/pdf-print-rc11-final");
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("source.pdf");
    fixture(&source);
    let original = std::fs::read(&source).unwrap();
    let session = PdfSession::open(&source).unwrap();
    let fit = folder.join("page-2-fit.pdf");
    let half = folder.join("page-2-50pct.pdf");
    for (path, percent) in [(&fit, None), (&half, Some(50))] {
        assert_eq!(
            print_pdf_to(
                session.snapshot(),
                PdfPrintSettings {
                    first: 2,
                    last: 2,
                    percent
                },
                Some(path)
            )
            .unwrap(),
            liteoffice::print::PrintOutcome::Printed
        );
    }
    let fit_width = red_width(&fit, &folder.join("page-2-fit.png"));
    let half_width = red_width(&half, &folder.join("page-2-50pct.png"));
    assert!(
        fit_width > half_width * 2,
        "Fit should occupy more width than 50% actual source size"
    );
    assert_eq!(std::fs::read(source).unwrap(), original);
    println!(
        "Created {}; red widths: Fit={fit_width}, 50%={half_width}",
        folder.display()
    );
}
