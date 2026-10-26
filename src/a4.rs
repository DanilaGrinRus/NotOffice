/// Physical dimensions of ISO 216 A4 in millimetres.
pub const A4_WIDTH_MM: f32 = 210.0;
pub const A4_HEIGHT_MM: f32 = 297.0;

/// A simple physical page definition.  The editor deliberately keeps layout
/// measurements independent from the monitor's DPI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_left_mm: f32,
    pub margin_right_mm: f32,
    pub margin_top_mm: f32,
    pub margin_bottom_mm: f32,
}

impl Default for PageGeometry {
    fn default() -> Self {
        Self {
            width_mm: A4_WIDTH_MM,
            height_mm: A4_HEIGHT_MM,
            margin_left_mm: 20.0,
            margin_right_mm: 20.0,
            margin_top_mm: 20.0,
            margin_bottom_mm: 20.0,
        }
    }
}

impl PageGeometry {
    pub fn mm_to_points(mm: f32) -> f32 {
        mm * 72.0 / 25.4
    }

    pub fn page_size_points(self) -> (f32, f32) {
        (
            Self::mm_to_points(self.width_mm),
            Self::mm_to_points(self.height_mm),
        )
    }

    pub fn content_width_points(self) -> f32 {
        Self::mm_to_points(self.width_mm - self.margin_left_mm - self.margin_right_mm)
    }

    pub fn content_height_points(self) -> f32 {
        Self::mm_to_points(self.height_mm - self.margin_top_mm - self.margin_bottom_mm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_210_by_297_mm() {
        let page = PageGeometry::default();
        assert_eq!((page.width_mm, page.height_mm), (210.0, 297.0));
        assert!((page.page_size_points().0 - 595.2756).abs() < 0.01);
    }
}
