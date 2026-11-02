//! Test-mode HUD: a window-fixed readout in the match screen's bottom-left
//! corner, shown from launch without a keypress.
//!
//! The block is drawn on the final window surface in window pixels through
//! `FpsOverlay::encode_bottom_left`, after every camera and zoom pass, so the
//! overlay keeps its size, place and legibility at every zoom and at every
//! Options whole-frame zoom. Its text may report the zoom; the overlay itself
//! never scales with it.
//!
//! Captures never include it (`frame::render_frame` skips it while a shell or
//! tactical capture session owns the frame), and a build without
//! `dev-overlays` does not compile it at all.
//!
//! The readout uses the built-in 5x7 face, which has digits, `-`, `:`, `/` and
//! capitals A..Y — no `Z` and no period — so a zoom reads as an integer
//! percentage instead of a decimal factor.

/// Whether the HUD starts shown. The test mode is on from launch.
pub(crate) const DEFAULT_VISIBLE: bool = true;

/// Left inset and line height come from the shared overlay encoder.
pub(crate) struct TestHud {
    visible: bool,
}

impl Default for TestHud {
    fn default() -> Self {
        Self {
            visible: DEFAULT_VISIBLE,
        }
    }
}

impl TestHud {
    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// Show or hide the block (the Test Mode switch, and Ctrl+Shift+F6).
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Ctrl+Shift+F6: show or hide the block; returns whether it is now shown.
    pub(crate) fn toggle(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }
}

/// The HUD's lines, one entry per row, in the order they read top-down.
///
/// `zoom` is the match camera's live `zoom_level` (1.0 is native scale).
pub(crate) fn lines(zoom: f32) -> Vec<String> {
    vec![zoom_line(zoom)]
}

/// `SCALE 125 PCT` for a 1.25x camera; `SCALE -- PCT` before there is a view.
pub(crate) fn zoom_line(zoom: f32) -> String {
    if !zoom.is_finite() || zoom <= 0.0 {
        return "SCALE -- PCT".to_owned();
    }
    format!("SCALE {} PCT", (zoom * 100.0).round() as i64)
}

/// Draw the block for this frame, when it is shown.
pub(crate) fn encode(
    state: &crate::app::state::AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) {
    if !state.renderer.test_hud.is_visible() {
        return;
    }
    let text = lines(state.match_state.input.zoom_level).join("\n");
    state.renderer.fps_overlay.encode_bottom_left(
        &state.renderer.gpu,
        &state.renderer.batch_renderer,
        encoder,
        destination,
        &text,
        "window_test_hud",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_mode_hud_is_shown_from_launch_and_can_be_toggled() {
        let mut hud = TestHud::default();
        assert!(hud.is_visible(), "the test HUD starts shown");
        assert!(!hud.toggle());
        assert!(!hud.is_visible());
        assert!(hud.toggle());
        assert!(hud.is_visible());
    }

    #[test]
    fn the_zoom_line_reads_the_camera_scale_as_a_percentage() {
        for (zoom, expected) in [
            (1.0, "SCALE 100 PCT"),
            (1.25, "SCALE 125 PCT"),
            (2.0, "SCALE 200 PCT"),
            (3.0, "SCALE 300 PCT"),
            (0.8, "SCALE 80 PCT"),
            (0.25, "SCALE 25 PCT"),
        ] {
            assert_eq!(zoom_line(zoom), expected);
        }
    }

    #[test]
    fn the_zoom_line_never_formats_a_value_without_a_view() {
        for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(zoom_line(zoom), "SCALE -- PCT");
        }
    }

    /// The built-in 5x7 face is uppercase-only and has neither `Z` nor `.`;
    /// a line that spells a word it cannot draw would render as gap glyphs.
    #[test]
    fn every_hud_character_exists_in_the_built_in_font() {
        let supported = crate::render::bit_font::fallback_5x7_glyphs();
        for zoom in [0.25, 0.8, 1.0, 1.25, 2.0, 3.0, f32::NAN] {
            for line in lines(zoom) {
                for ch in line.chars() {
                    assert!(
                        supported.iter().any(|(glyph, _)| *glyph == ch),
                        "{line:?} needs {ch:?}, which the built-in font lacks"
                    );
                }
            }
        }
    }
}
