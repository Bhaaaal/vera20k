//! Diagnostics owner (F12): process debug overlays plus the hidden
//! shell/tactical capture harnesses. Match-lifetime diagnostic replay lives
//! in `app::match_diagnostics`.

pub(crate) mod debug_overlays;
pub(crate) mod fps_overlay;
pub(crate) mod shell_capture;
pub(crate) mod state;
pub(crate) mod tactical_capture;
/// Test-mode HUD: a window-fixed readout in the match screen's bottom-left.
#[cfg(feature = "dev-overlays")]
pub(crate) mod test_hud;
/// Test-mode record of a refused placement: area, footprint and gates.
#[cfg(feature = "dev-overlays")]
pub(crate) mod placement_dump;
