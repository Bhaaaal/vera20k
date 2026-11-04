//! VeraX custom trails (`--features verax`): ART `[VeraTrailTypes]` catalog.
//!
//! A catalog entry names one trail's style. A type's ART section lists the
//! trails it draws with `VeraTrail.Types=NAME,NAME`; the order in the list is
//! only a configuration order, because drawing sorts by `ZOrder`. This is
//! VERA20K's own extension: a build without `verax` reads none of these keys,
//! and a stock build's ART leaves every list unset.
//!
//! Anchors use the native GetFLH body frame (`forward`/`lateral`/`height`), so
//! a wing-tip or exhaust offset follows the hull's facing and Fly pose. A
//! projectile has no body frame: its list ignores `FLH` and anchors on the
//! bullet coordinate itself, like retail `UseLineTrail`.

use crate::rules::ini_parser::{IniFile, IniSection};

/// At most this many trails draw for one type, whatever its list says.
pub const MAX_TRAILS: usize = 12;
/// Widest line the parallel-row expansion accepts, in logical pixels.
pub const MAX_WIDTH: u8 = 8;
/// Longest fade the ring keeps samples for, in milliseconds.
const MAX_FADE_MS: i32 = 4000;
/// Largest sampling spacing, in leptons.
const MAX_SEGMENT_LENGTH: i32 = 2000;
/// `VeraTrail.Types=` text buffer; the names are short but the list may grow.
pub(crate) const LIST_CAPACITY: usize = 0x200;

/// One catalog entry: everything the presentation needs to draw a trail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VeraTrailStyle {
    /// Body-space `GetFLH` offset: forward, lateral, height, in leptons.
    /// Read as `FLH=`; ignored for a projectile.
    pub flh: [i32; 3],
    /// Head colour (`Color=`).
    pub color: [u8; 3],
    /// Tail colour (`Color2=`); `None` keeps `color` along the whole trail.
    pub color2: Option<[u8; 3]>,
    /// Line width in logical pixels at zoom 1, `1..=MAX_WIDTH`.
    pub width: u8,
    /// Milliseconds a sample takes to fade from full strength to zero.
    pub fade_ms: i32,
    /// Smallest anchor movement, in leptons, that appends a sample; `0`
    /// appends one on any movement, like retail LineTrail.
    pub segment_length: i32,
    /// Head opacity, `0..=255`.
    pub alpha: u8,
    /// Draw order: lower values draw first (behind higher ones).
    pub z_order: i32,
}

impl Default for VeraTrailStyle {
    fn default() -> Self {
        Self {
            flh: [0; 3],
            color: [255, 255, 255],
            color2: None,
            width: 2,
            fade_ms: 600,
            segment_length: 0,
            alpha: 255,
            z_order: 0,
        }
    }
}

/// `[VeraTrailTypes]` plus every named section's style.
#[derive(Clone, Debug, Default)]
pub struct VeraTrailCatalog {
    /// Uppercase entry name -> style, in catalog order.
    entries: Vec<(String, VeraTrailStyle)>,
}

impl VeraTrailCatalog {
    /// Read the catalog from the ART document's `[VeraTrailTypes]` list.
    ///
    /// A missing list or missing entry section is skipped, not fatal: one typo
    /// must not silently disable every other trail in the file.
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut catalog = Self::default();
        let Some(list) = ini.section("VeraTrailTypes") else {
            return catalog;
        };
        for key in list.physical_keys() {
            // Enumerable-style list readers resolve names through their own
            // section; the name keeps its authored bytes but is matched
            // case-insensitively.
            let Some(name) = list.read_name(key, 0x40) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let Some(section) = ini.section(name) else {
                continue;
            };
            catalog
                .entries
                .push((name.to_ascii_uppercase(), read_style(section)));
        }
        catalog
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolve one `VeraTrail.Types=` text into drawable styles.
    ///
    /// Names match case-insensitively; unknown or empty names are skipped.
    /// The list is capped at [`MAX_TRAILS`] resolved trails.
    pub fn resolve(&self, list: &str) -> Vec<VeraTrailStyle> {
        let mut styles = Vec::new();
        for name in list.split(',').map(str::trim) {
            if name.is_empty() {
                continue;
            }
            if styles.len() >= MAX_TRAILS {
                break;
            }
            if let Some((_, style)) = self
                .entries
                .iter()
                .find(|(entry, _)| entry.eq_ignore_ascii_case(name))
            {
                styles.push(*style);
            }
        }
        styles
    }

    /// The style a catalog entry named `name` supplies, if any.
    pub fn style(&self, name: &str) -> Option<VeraTrailStyle> {
        self.entries
            .iter()
            .find(|(entry, _)| entry.eq_ignore_ascii_case(name))
            .map(|(_, style)| *style)
    }
}

fn read_style(section: &IniSection) -> VeraTrailStyle {
    let defaults = VeraTrailStyle::default();
    let color = section.read_color_rgb("Color", defaults.color, None);
    let color2 = section
        .is_present("Color2")
        .then(|| section.read_color_rgb("Color2", color, None));
    VeraTrailStyle {
        flh: section.read_coord3_value("FLH").unwrap_or(defaults.flh),
        color,
        color2,
        width: clamp_width(section.read_int("Width", i32::from(defaults.width))),
        fade_ms: section
            .read_int("FadeDuration", defaults.fade_ms)
            .clamp(1, MAX_FADE_MS),
        segment_length: section
            .read_int("SegmentLength", defaults.segment_length)
            .clamp(0, MAX_SEGMENT_LENGTH),
        alpha: section
            .read_int("Alpha", i32::from(defaults.alpha))
            .clamp(0, 255) as u8,
        z_order: section.read_int("ZOrder", defaults.z_order),
    }
}

fn clamp_width(value: i32) -> u8 {
    value.clamp(1, i32::from(MAX_WIDTH)) as u8
}

#[cfg(test)]
#[path = "vera_trail_tests.rs"]
mod tests;
