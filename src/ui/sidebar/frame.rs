//! The sidebar's place and size in a match frame.
//!
//! Like gamemd's `g_SidebarSurface`, the sidebar lays out and draws in its own
//! native pixels, and so does the command bar along the frame's bottom. Both
//! lay out on the frame at the player's Sidebar size (Options): the frame in
//! sidebar pixels, rounded up. The frame shows the sidebar's column flush with
//! its top and right edges, and the bar's strip flush with its bottom edge and
//! the column, each scaled by the Sidebar size; the battlefield takes the
//! rest. At Normal it is all the frame's own pixels.
//!
//! Layout space is that frame in sidebar pixels: the column's left edge at
//! `layout width - 168`, the strip's top at `layout height - 32`. At Normal it
//! is the frame space itself, so only Large and XL map points and rects
//! between the two.

use super::Rect;

/// The sidebar column's native width (`6A5090`), in sidebar pixels.
pub const SIDEBAR_WIDTH_PX: u32 = 168;

/// The CNC3-style widened sidebar column, in sidebar pixels. The live in-game
/// frame uses this width; captures keep the native [`SIDEBAR_WIDTH_PX`] for
/// gamemd comparison. `SidebarChromeLayoutSpec::modern_three_column` lays the
/// three cameo columns out against this width.
pub const MODERN_SIDEBAR_WIDTH_PX: u32 = 220;

/// The modern panel's fixed height, in sidebar pixels: the top chrome
/// (`cameo_grid_top` = 227), four 50-pixel cameo rows, and the footer/scroll
/// strip below them. The panel is top-anchored; the battlefield fills the rest
/// of the frame, including below the panel.
pub const MODERN_PANEL_HEIGHT_PX: u32 = 472;

/// Stock YR reserves the right sidebar and a fixed 32-pixel bottom strip, the
/// command bar's, when it builds the tactical view rectangle. In sidebar
/// pixels.
pub const BOTTOM_STRIP_HEIGHT_PX: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarFrame {
    frame_width: u32,
    frame_height: u32,
    /// Frame pixels per 100 sidebar pixels: 100 at Normal.
    percent: u32,
    /// The sidebar column's width in sidebar pixels. Native frames use
    /// [`SIDEBAR_WIDTH_PX`]; the modern in-game frame uses
    /// [`MODERN_SIDEBAR_WIDTH_PX`].
    sidebar_width: u32,
    /// The panel's height in sidebar pixels. `None` is the native full-height
    /// column; `Some(h)` is the compact top-anchored modern panel, below which
    /// the battlefield fills the rest of the frame.
    panel_height: Option<u32>,
}

impl SidebarFrame {
    pub fn new(frame_width: u32, frame_height: u32, percent: u32) -> Self {
        Self::with_sidebar_width(frame_width, frame_height, percent, SIDEBAR_WIDTH_PX)
    }

    /// The same frame with an explicit sidebar column width (e.g. the widened
    /// modern column), still a full-height column.
    pub fn with_sidebar_width(
        frame_width: u32,
        frame_height: u32,
        percent: u32,
        sidebar_width: u32,
    ) -> Self {
        Self {
            frame_width,
            frame_height,
            percent: percent.max(100),
            sidebar_width: sidebar_width.max(1),
            panel_height: None,
        }
    }

    /// The live in-game frame: the widened column as a compact top-anchored
    /// panel, with the battlefield filling the rest of the frame.
    pub fn modern(frame_width: u32, frame_height: u32, percent: u32) -> Self {
        Self {
            frame_width,
            frame_height,
            percent: percent.max(100),
            sidebar_width: MODERN_SIDEBAR_WIDTH_PX,
            panel_height: Some(MODERN_PANEL_HEIGHT_PX),
        }
    }

    /// The native size, Normal: what captures and native comparisons use.
    pub fn native(frame_width: u32, frame_height: u32) -> Self {
        Self::new(frame_width, frame_height, 100)
    }

    /// The sidebar column's width in sidebar pixels.
    pub fn sidebar_width(self) -> u32 {
        self.sidebar_width
    }

    /// Whether the sidebar is a compact top-anchored panel (rather than the
    /// native full-height column).
    pub fn is_compact_panel(self) -> bool {
        self.panel_height.is_some()
    }

    /// The panel's height in frame pixels: the full frame height for the
    /// native column, the fixed panel height otherwise.
    pub fn panel_height_in_frame(self) -> u32 {
        match self.panel_height {
            Some(h) => h * self.percent / 100,
            None => self.frame_height,
        }
    }

    /// Whether a frame pixel lies inside the compact panel. Always false for
    /// the native full-height column (the sidebar surface handles that case).
    pub fn panel_contains(self, x: f32, y: f32) -> bool {
        self.is_compact_panel()
            && x >= self.frame_left()
            && x < self.frame_width as f32
            && y >= 0.0
            && y < self.panel_height_in_frame() as f32
    }

    /// Frame pixels per sidebar pixel.
    pub fn scale(self) -> f32 {
        self.percent as f32 / 100.0
    }

    /// Whether every sidebar pixel is a whole block of frame pixels.
    pub fn is_sharp(self) -> bool {
        self.percent.is_multiple_of(100)
    }

    /// The column's width in frame pixels: `sidebar_width` at Normal.
    pub fn width_in_frame(self) -> u32 {
        self.sidebar_width * self.percent / 100
    }

    /// The bottom strip's height in frame pixels: 32 at Normal.
    pub fn strip_height_in_frame(self) -> u32 {
        BOTTOM_STRIP_HEIGHT_PX * self.percent / 100
    }

    /// The battlefield: the frame less the column and the strip. A compact
    /// panel leaves the full frame width for the battlefield (the panel only
    /// covers the top-right); the native column reserves its width. A
    /// degenerate frame keeps one pixel, so its scissor never drops every
    /// tactical draw.
    pub fn tactical_size(self) -> (u32, u32) {
        let width = if self.is_compact_panel() {
            self.frame_width
        } else {
            self.frame_width.saturating_sub(self.width_in_frame())
        };
        (
            width.max(1),
            self.frame_height
                .saturating_sub(self.strip_height_in_frame())
                .max(1),
        )
    }

    /// The frame in sidebar pixels, rounded up so the sidebar and the bar
    /// reach the frame's edges.
    pub fn layout_size(self) -> (u32, u32) {
        (
            (self.frame_width * 100).div_ceil(self.percent).max(1),
            (self.frame_height * 100).div_ceil(self.percent).max(1),
        )
    }

    /// [`Self::layout_size`] as the screen the native layout (`6A5130`) runs
    /// on.
    pub fn layout_screen(self) -> (f32, f32) {
        let (width, height) = self.layout_size();
        (width as f32, height as f32)
    }

    /// Layout-space x of the column's left edge.
    pub fn layout_left(self) -> f32 {
        self.layout_size().0 as f32 - self.sidebar_width as f32
    }

    /// Layout-space y of the strip's top edge.
    pub fn strip_layout_top(self) -> f32 {
        self.layout_size().1 as f32 - BOTTOM_STRIP_HEIGHT_PX as f32
    }

    /// The sidebar's surface in its own pixels: the column's width and the
    /// layout's height (the fixed panel height for the compact modern panel).
    pub fn surface_size(self) -> (u32, u32) {
        (
            self.sidebar_width,
            self.panel_height.unwrap_or_else(|| self.layout_size().1),
        )
    }

    /// The bar's surface in its own pixels: the layout's width left of the
    /// column, and the strip's native height. The compact panel leaves the
    /// full layout width for the bar.
    pub fn bar_surface_size(self) -> (u32, u32) {
        let width = if self.is_compact_panel() {
            self.layout_size().0
        } else {
            self.layout_size().0.saturating_sub(self.sidebar_width)
        };
        (width.max(1), BOTTOM_STRIP_HEIGHT_PX)
    }

    /// The column's left edge in frame pixels; left of the frame when the
    /// frame is narrower than the column.
    pub fn frame_left(self) -> f32 {
        self.frame_width as f32 - self.width_in_frame() as f32
    }

    /// The column's part of the frame, in frame pixels: where the sidebar's
    /// surface is shown. A compact panel is top-anchored at its fixed height.
    pub fn column(self) -> (u32, u32, u32, u32) {
        let x = self.frame_width.saturating_sub(self.width_in_frame());
        (x, 0, self.frame_width - x, self.panel_height_in_frame())
    }

    /// The strip's part of the frame, in frame pixels: where the bar's surface
    /// is shown. The compact panel leaves the full frame width for the bar;
    /// the native column leaves it left of the column.
    pub fn strip(self) -> (u32, u32, u32, u32) {
        let y = self
            .frame_height
            .saturating_sub(self.strip_height_in_frame());
        let width = if self.is_compact_panel() {
            self.frame_width
        } else {
            self.column().0
        };
        (0, y, width, self.frame_height - y)
    }

    /// The frame pixel where the bar's surface pixel (0, 0) lands; above the
    /// frame when the frame is shorter than the strip.
    pub fn bar_origin(self) -> (f32, f32) {
        (
            self.x_to_frame(0.0),
            self.frame_height as f32 - self.strip_height_in_frame() as f32,
        )
    }

    /// The frame pixel where layout pixel (0, 0) lands for a surface laid out
    /// on the whole layout, an active-game shell: its column on the
    /// sidebar's. Left of the frame when the layout rounded up.
    /// [`Self::point_to_layout`] is its inverse.
    pub fn layout_origin(self) -> (f32, f32) {
        (self.x_to_frame(0.0), 0.0)
    }

    /// Layout x in frame pixels: the column and the bar are flush with the
    /// frame's right edge.
    fn x_to_frame(self, x: f32) -> f32 {
        self.frame_left() + (x - self.layout_left()) * self.scale()
    }

    /// A rect of the sidebar's, in layout space, in frame pixels.
    pub fn rect_to_frame(self, rect: Rect) -> Rect {
        let scale = self.scale();
        Rect {
            x: self.x_to_frame(rect.x),
            y: rect.y * scale,
            w: rect.w * scale,
            h: rect.h * scale,
        }
    }

    /// A rect of the bar's, in layout space, in frame pixels.
    pub fn bar_rect_to_frame(self, rect: Rect) -> Rect {
        let scale = self.scale();
        Rect {
            x: self.x_to_frame(rect.x),
            y: self.bar_origin().1 + (rect.y - self.strip_layout_top()) * scale,
            w: rect.w * scale,
            h: rect.h * scale,
        }
    }

    /// A frame point in the sidebar's layout space.
    pub fn point_to_layout(self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.scale();
        (
            self.layout_left() + (x - self.frame_left()) / scale,
            y / scale,
        )
    }

    /// The layout pixel a whole-layout copy shows at frame pixel (x, y): the
    /// one under its centre, as the copy samples it at the whole sizes. Hit
    /// tests on whole pixels use it to land where the layout is drawn.
    pub fn pixel_to_layout(self, x: i32, y: i32) -> (i32, i32) {
        let (x, y) = self.point_to_layout(x as f32 + 0.5, y as f32 + 0.5);
        (x.floor() as i32, y.floor() as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn normal_is_the_frame_itself() {
        let frame = SidebarFrame::new(800, 600, 100);
        assert_eq!(frame.width_in_frame(), 168);
        assert_eq!(frame.column(), (632, 0, 168, 600));
        assert_eq!(frame.strip(), (0, 568, 632, 32));
        assert_eq!(frame.tactical_size(), (632, 568));
        assert_eq!(frame.surface_size(), (168, 600));
        assert_eq!(frame.bar_surface_size(), (632, 32));
        assert_eq!(frame.layout_screen(), (800.0, 600.0));
        assert_eq!(frame.bar_origin(), (0.0, 568.0));
        let cameo = rect(648.0, 49.0, 140.0, 108.0);
        assert_eq!(frame.rect_to_frame(cameo), cameo);
        let button = rect(32.0, 568.0, 52.0, 32.0);
        assert_eq!(frame.bar_rect_to_frame(button), button);
        assert_eq!(frame.point_to_layout(700.5, 99.5), (700.5, 99.5));
        assert_eq!(frame.layout_origin(), (0.0, 0.0));
        assert!(frame.is_sharp());
    }

    #[test]
    fn larger_sizes_scale_the_sidebar_and_bar_from_the_corner() {
        let large = SidebarFrame::new(1920, 1080, 150);
        assert_eq!(large.width_in_frame(), 252);
        assert_eq!(large.strip_height_in_frame(), 48);
        assert_eq!(large.column(), (1668, 0, 252, 1080));
        assert_eq!(large.strip(), (0, 1032, 1668, 48));
        assert_eq!(large.tactical_size(), (1668, 1032));
        assert_eq!(large.layout_screen(), (1280.0, 720.0));
        assert_eq!(large.surface_size(), (168, 720));
        assert_eq!(large.bar_surface_size(), (1112, 32));
        assert!(!large.is_sharp());
        // The whole layout column and strip become the frame's.
        assert_eq!(
            large.rect_to_frame(rect(1112.0, 0.0, 168.0, 720.0)),
            rect(1668.0, 0.0, 252.0, 1080.0)
        );
        assert_eq!(
            large.bar_rect_to_frame(rect(0.0, 688.0, 1112.0, 32.0)),
            rect(0.0, 1032.0, 1668.0, 48.0)
        );
        assert_eq!(large.point_to_layout(1818.0, 300.0), (1212.0, 200.0));
        assert_eq!(large.layout_origin(), (0.0, 0.0));

        // An odd frame rounds the layout up: the column reaches the bottom
        // edge, and the bar still sits on it.
        let xl = SidebarFrame::new(1921, 1081, 200);
        assert_eq!(xl.layout_size(), (961, 541));
        assert_eq!(xl.surface_size(), (168, 541));
        assert_eq!(xl.strip(), (0, 1017, 1585, 64));
        assert_eq!(xl.bar_origin(), (-1.0, 1017.0));
        // A whole-layout surface shares the column's corner, and the frame's
        // pixels map back onto its own.
        assert_eq!(xl.layout_origin(), (-1.0, 0.0));
        assert_eq!(xl.point_to_layout(-1.0, 0.0), (0.0, 0.0));
        assert_eq!(xl.point_to_layout(1920.0, 1080.0), (960.5, 540.0));
        // A frame shorter than the strip keeps a one-pixel battlefield.
        let tiny = SidebarFrame::native(100, 20);
        assert_eq!(tiny.tactical_size(), (1, 1));
        assert_eq!(tiny.strip(), (0, 0, 0, 20));
        assert_eq!(tiny.bar_origin(), (0.0, -12.0));
        assert_eq!(
            xl.bar_rect_to_frame(rect(10.0, 509.0, 52.0, 32.0)),
            rect(19.0, 1017.0, 104.0, 64.0)
        );
        assert!(xl.is_sharp());
    }

    #[test]
    fn a_whole_layout_surface_puts_its_column_on_the_sidebar() {
        for (width, height) in [(800, 600), (1366, 768), (1920, 1080), (1921, 1081)] {
            for percent in [100, 150, 200] {
                let frame = SidebarFrame::new(width, height, percent);
                let (x, y) = frame.layout_origin();
                assert_eq!(x + frame.layout_left() * frame.scale(), frame.frame_left());
                assert_eq!(y, 0.0);
                assert_eq!(frame.point_to_layout(x, y), (0.0, 0.0));
            }
        }
    }

    #[test]
    fn frame_pixels_hit_the_layout_pixel_the_copy_shows() {
        let normal = SidebarFrame::native(800, 600);
        assert_eq!(normal.pixel_to_layout(37, 41), (37, 41));
        // XL shows each layout pixel as a 2x2 block from the corner.
        let xl = SidebarFrame::new(1920, 1080, 200);
        assert_eq!(xl.pixel_to_layout(38, 40), (19, 20));
        assert_eq!(xl.pixel_to_layout(39, 41), (19, 20));
        assert_eq!(xl.pixel_to_layout(40, 42), (20, 21));
        // An odd frame starts the blocks one pixel left of the frame.
        let odd = SidebarFrame::new(1921, 1081, 200);
        assert_eq!(odd.pixel_to_layout(0, 0), (0, 0));
        assert_eq!(odd.pixel_to_layout(1, 1), (1, 0));
        assert_eq!(odd.pixel_to_layout(2, 2), (1, 1));
        // Large gives a seam pixel to the layout pixel under its centre.
        let large = SidebarFrame::new(1920, 1080, 150);
        assert_eq!(large.pixel_to_layout(0, 0), (0, 0));
        assert_eq!(large.pixel_to_layout(1, 1), (1, 1));
        assert_eq!(large.pixel_to_layout(2, 2), (1, 1));
        assert_eq!(large.pixel_to_layout(3, 3), (2, 2));
    }

    #[test]
    fn modern_sidebar_width_widens_the_column_and_shrinks_the_battlefield() {
        let frame = SidebarFrame::with_sidebar_width(800, 600, 100, MODERN_SIDEBAR_WIDTH_PX);
        assert_eq!(frame.sidebar_width(), 220);
        assert_eq!(frame.width_in_frame(), 220);
        assert_eq!(frame.column(), (580, 0, 220, 600));
        assert_eq!(frame.strip(), (0, 568, 580, 32));
        assert_eq!(frame.tactical_size(), (580, 568));
        assert_eq!(frame.surface_size(), (220, 600));
        assert_eq!(frame.bar_surface_size(), (580, 32));
        assert_eq!(frame.layout_left(), 580.0);
    }

    #[test]
    fn modern_compact_panel_is_top_anchored_and_returns_space_to_the_battlefield() {
        let frame = SidebarFrame::modern(800, 600, 100);
        assert!(frame.is_compact_panel());
        assert_eq!(frame.sidebar_width(), 220);
        assert_eq!(frame.panel_height_in_frame(), 472);
        // The panel is a top-anchored block; the battlefield is full width.
        assert_eq!(frame.column(), (580, 0, 220, 472));
        assert_eq!(frame.strip(), (0, 568, 800, 32));
        assert_eq!(frame.tactical_size(), (800, 568));
        assert_eq!(frame.surface_size(), (220, 472));
        assert_eq!(frame.bar_surface_size(), (800, 32));
        // The panel's left edge is unchanged.
        assert_eq!(frame.layout_left(), 580.0);
    }
}
