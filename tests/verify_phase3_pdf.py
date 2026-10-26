"""Inspect real Microsoft Print to PDF output from windows_pdf_fixture_regression.

Requires PyMuPDF. Usage: python tests/verify_phase3_pdf.py EVIDENCE_DIRECTORY [--render]
"""

import argparse
import json
from pathlib import Path

import pymupdf


def verify(folder: Path, render: bool) -> None:
    results = []
    for name, width_mm, height_mm, margin_mm, numbers in (
        ("portrait-10mm.pdf", 210, 297, 10, True),
        ("landscape-25mm.pdf", 297, 210, 25, True),
        ("portrait-no-numbers.pdf", 210, 297, 10, False),
    ):
        with pymupdf.open(folder / name) as document:
            assert len(document) == 2, (name, "unexpected pagination", len(document))
            for page_index, page in enumerate(document):
                assert abs(page.rect.width * 25.4 / 72 - width_mm) < 0.3, name
                assert abs(page.rect.height * 25.4 / 72 - height_mm) < 0.3, name
                text = page.get_text()
                marker = "PAGE-ONE" if page_index == 0 else "PAGE-TWO"
                assert marker in text, (name, page_index, "lost page text")
                assert (f"Page {page_index + 1}" in text) == numbers, (name, "footer setting")
                if render:
                    page.get_pixmap(matrix=pymupdf.Matrix(1.5, 1.5), alpha=False).save(
                        folder / f"{Path(name).stem}-page-{page_index + 1}.png"
                    )

            first, second = document[0], document[1]
            assert "Кириллица: Ёжик." in first.get_text(), (name, "Cyrillic changed")
            assert "Левая" in first.get_text() and "Правая" in first.get_text(), name
            assert len(first.get_drawings()) >= 2, (name, "missing table borders")
            images = first.get_image_info()
            assert len(images) == 1, (name, "referenced image missing or unused image printed")
            assert not second.get_image_info(), (name, "image crossed explicit page break")
            image_rect = pymupdf.Rect(images[0]["bbox"])
            assert abs(image_rect.width - 144) < 1 and abs(image_rect.height - 72) < 1, (
                name, "image size differs from preview", image_rect
            )
            assert image_rect.y0 > first.search_for("Левая")[0].y1, (name, "wrong image anchor")
            text_rect = first.search_for("Кириллица:")[0]
            left_mm = text_rect.x0 * 25.4 / 72
            top_mm = text_rect.y0 * 25.4 / 72
            assert abs(left_mm - margin_mm) < 1, (name, "wrong left margin", left_mm)
            assert abs(top_mm - margin_mm) < 1, (name, "wrong top margin", top_mm)
            raster = first.get_pixmap(clip=image_rect, alpha=False)
            assert min(raster.samples) < 200, (name, "blank image")
            results.append({"file": name, "pages": 2, "left_mm": round(left_mm, 2),
                            "top_mm": round(top_mm, 2), "images": 1,
                            "table_paths": len(first.get_drawings()), "numbers": numbers})
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--render", action="store_true")
    arguments = parser.parse_args()
    verify(arguments.directory.resolve(strict=True), arguments.render)
