//! VeraX custom trails: presentation history for `--features verax`.
//!
//! One owner keeps every trail's samples keyed by (owner kind, stable id,
//! catalog index). Sampling reads the simulation's committed anchor each
//! composite and never runs inside the simulation: no tick, save, replay or
//! RNG state changes. Aging uses wall-clock milliseconds, so a trail keeps the
//! length its `FadeDuration=` asked for whatever the display rate is.
//!
//! A segment fills one `Mix` surface run per pixel row that its `Width=` band
//! covers, so the width stays continuous instead of becoming a set of parallel
//! single-pixel lines. A segment's colour interpolates from `Color=` at the
//! head toward `Color2=` at the tail, and its strength fades with the older
//! sample's age.

use std::collections::HashMap;
use std::collections::VecDeque;

use crate::render::surface_line::{SurfaceDraw, SurfaceLine, SurfaceLineBlend};
use crate::rules::vera_trail::VeraTrailStyle;

/// Safety cap on retained samples per trail. A 4000 ms fade sampled at 60 Hz
/// needs 240; the cap only guards a pathological display rate.
const MAX_SAMPLES: usize = 256;
/// Segments weaker than this are not drawn, matching the retail trail's A gate.
const MIN_STRENGTH: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum OwnerKind {
    Entity,
    Projectile,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    coord: [i32; 3],
    at_ms: u64,
}

struct Trail {
    style: VeraTrailStyle,
    samples: VecDeque<Sample>,
    /// The owner asked for this trail in the current frame.
    seen: bool,
    /// The owner still exists (its removal only starts the fade).
    attached: bool,
}

impl Trail {
    /// Whether `coord` is far enough from the newest sample to append one.
    fn wants_sample(&self, coord: [i32; 3]) -> bool {
        let Some(newest) = self.samples.back() else {
            return true;
        };
        if coord == newest.coord {
            return false;
        }
        if self.style.segment_length <= 0 {
            return true;
        }
        let dx = i64::from(coord[0] - newest.coord[0]);
        let dy = i64::from(coord[1] - newest.coord[1]);
        let dz = i64::from(coord[2] - newest.coord[2]);
        let reach = i64::from(self.style.segment_length);
        dx * dx + dy * dy + dz * dz >= reach * reach
    }
}

/// All custom trails of one client. Cleared on load and on a new match.
#[derive(Default)]
pub(crate) struct VeraTrails {
    trails: HashMap<(OwnerKind, u64, u8), Trail>,
    /// Key insertion order; ties keep the order the catalog produced.
    order: Vec<(OwnerKind, u64, u8)>,
}

impl VeraTrails {
    /// Mark every trail unseen; the frame's samples mark the live ones.
    pub(crate) fn begin_frame(&mut self) {
        for trail in self.trails.values_mut() {
            trail.seen = false;
        }
    }

    /// Record this frame's anchor for one owner trail.
    ///
    /// The trail is created on first sight from the type's style; a coordinate
    /// that did not move (or moved less than `SegmentLength=`) appends nothing,
    /// so a stationary owner keeps no stacked points.
    pub(crate) fn sample(
        &mut self,
        kind: OwnerKind,
        owner: u64,
        index: u8,
        style: &VeraTrailStyle,
        coord: [i32; 3],
        now_ms: u64,
    ) {
        let key = (kind, owner, index);
        if !self.trails.contains_key(&key) {
            self.order.push(key);
            self.trails.insert(
                key,
                Trail {
                    style: *style,
                    samples: VecDeque::new(),
                    seen: true,
                    attached: true,
                },
            );
        }
        let Some(trail) = self.trails.get_mut(&key) else {
            return;
        };
        trail.seen = true;
        trail.attached = true;
        trail.style = *style;
        if trail.wants_sample(coord) {
            trail.samples.push_back(Sample {
                coord,
                at_ms: now_ms,
            });
            while trail.samples.len() > MAX_SAMPLES {
                trail.samples.pop_front();
            }
        }
    }

    /// Drop samples older than each trail's fade and retire faded owners.
    pub(crate) fn end_frame(&mut self, now_ms: u64) {
        for key in &self.order {
            let Some(trail) = self.trails.get_mut(key) else {
                continue;
            };
            if !trail.seen {
                trail.attached = false;
            }
            // A fade of at least one millisecond keeps the division finite.
            let fade = u64::try_from(trail.style.fade_ms).unwrap_or(1).max(1);
            while trail
                .samples
                .front()
                .is_some_and(|sample| now_ms.saturating_sub(sample.at_ms) > fade)
            {
                trail.samples.pop_front();
            }
        }
        self.order
            .retain(|key| self.trails.get(key).is_some_and(Trail::is_live));
        self.trails.retain(|_, trail| trail.is_live());
    }

    /// Turn this frame's samples into ordered surface-line draws.
    ///
    /// Trails draw behind higher `ZOrder=` values; equal values keep catalog
    /// order. `hidden` gates a segment whose newer end is concealed, and `clip`
    /// bounds the runs a width band may produce.
    pub(crate) fn draws(
        &self,
        now_ms: u64,
        camera: [i32; 2],
        clip: [i32; 4],
        detail_level: u8,
        mut hidden: impl FnMut([i32; 2]) -> bool,
    ) -> Vec<SurfaceDraw> {
        let mut ordered: Vec<(i32, usize, SurfaceDraw)> = Vec::new();
        // The runs of one segment and of the segment before it: the shared
        // endpoint of two samples is covered by both bands, and only the older
        // segment may Mix those pixels, so no pixel takes the same trail twice.
        let mut previous: Vec<(i32, i32, i32)> = Vec::new();
        let mut current: Vec<(i32, i32, i32)> = Vec::new();
        for (sequence, key) in self.order.iter().enumerate() {
            let Some(trail) = self.trails.get(key) else {
                continue;
            };
            let style = trail.style;
            // Detail level 0 keeps the trail but narrows it and skips the
            // colour gradient, like the retail detail switch simplifies art.
            let detailed = detail_level != 0;
            let width = u8::max(1, if detailed { style.width } else { 1 });
            let alpha = if detailed {
                style.alpha
            } else {
                style.alpha.min(160)
            };
            if alpha == 0 {
                continue;
            }
            let fade = u64::try_from(style.fade_ms).unwrap_or(1).max(1);
            let count = trail.samples.len();
            previous.clear();
            for slot in 0..count.saturating_sub(1) {
                current.clear();
                'segment: {
                    let newer = trail.samples[slot + 1];
                    let older = trail.samples[slot];
                    let age_ms = now_ms.saturating_sub(newer.at_ms);
                    let faded = (i64::from(alpha) * age_ms as i64 / fade as i64) as i32;
                    let strength = (i32::from(alpha) - faded).max(0);
                    if strength < MIN_STRENGTH {
                        break 'segment;
                    }
                    let older_age = now_ms.saturating_sub(older.at_ms) as f64 / fade as f64;
                    let rgb = if detailed {
                        gradient_color(&style, older_age.clamp(0.0, 1.0))
                    } else {
                        style.color
                    };
                    let [from, to] = [older.coord, newer.coord].map(|coord| project(coord, camera));
                    if hidden(from) || hidden(to) {
                        break 'segment;
                    }
                    let z_adjust = [from_z(older.coord[2]), from_z(newer.coord[2])];
                    let blend = SurfaceLineBlend::Mix { rgb, strength };
                    if width == 1 {
                        // One logical pixel keeps the native single-pixel walk.
                        ordered.push((
                            style.z_order,
                            sequence,
                            SurfaceDraw::Line(SurfaceLine {
                                from,
                                to,
                                z_adjust,
                                blend,
                            }),
                        ));
                        break 'segment;
                    }
                    band_rows(
                        from,
                        to,
                        width,
                        z_adjust,
                        blend,
                        clip,
                        &previous,
                        &mut current,
                        |line| {
                            ordered.push((style.z_order, sequence, SurfaceDraw::Line(line)));
                        },
                    );
                }
                std::mem::swap(&mut previous, &mut current);
            }
        }
        ordered.sort_by_key(|(z_order, sequence, _)| (*z_order, *sequence));
        ordered.into_iter().map(|(_, _, draw)| draw).collect()
    }

    /// Drop every trail (load, new match, type removal).
    pub(crate) fn clear(&mut self) {
        self.trails.clear();
        self.order.clear();
    }

    #[cfg(test)]
    pub(crate) fn trail_count(&self) -> usize {
        self.trails.len()
    }

    #[cfg(test)]
    pub(crate) fn sample_len(&self, kind: OwnerKind, owner: u64, index: u8) -> usize {
        self.trails
            .get(&(kind, owner, index))
            .map_or(0, |trail| trail.samples.len())
    }
}

impl Trail {
    fn is_live(&self) -> bool {
        self.attached || !self.samples.is_empty()
    }
}

fn project(coord: [i32; 3], camera: [i32; 2]) -> [i32; 2] {
    let (x, y) = crate::util::lepton::absolute_leptons_to_screen(coord[0], coord[1], coord[2]);
    [x as i32 - camera[0], y as i32 - camera[1]]
}

/// The shared depth adjustment a line store takes for one endpoint.
fn from_z(z_leptons: i32) -> i32 {
    (-2i32).wrapping_sub(crate::util::native_x87::adjust_for_z_standard(z_leptons))
}

fn gradient_color(style: &VeraTrailStyle, tail: f64) -> [u8; 3] {
    let Some(end) = style.color2 else {
        return style.color;
    };
    std::array::from_fn(|index| {
        let start = f64::from(style.color[index]);
        let end = f64::from(end[index]);
        (start + (end - start) * tail).round().clamp(0.0, 255.0) as u8
    })
}

/// An `x` interval and whether each end excludes its value.
#[derive(Clone, Copy)]
struct Span {
    low: f64,
    high: f64,
    open_low: bool,
    open_high: bool,
}

/// The `x` where `low ≤ a·x + c ≤ high`.
fn span(a: f64, c: f64, low: f64, open_low: bool, high: f64, open_high: bool) -> Option<Span> {
    if a > 0.0 {
        Some(Span {
            low: (low - c) / a,
            high: (high - c) / a,
            open_low,
            open_high,
        })
    } else if a < 0.0 {
        Some(Span {
            low: (high - c) / a,
            high: (low - c) / a,
            open_low: open_high,
            open_high: open_low,
        })
    } else if (low < c || (!open_low && low == c)) && (c < high || (!open_high && c == high)) {
        Some(Span {
            low: f64::NEG_INFINITY,
            high: f64::INFINITY,
            open_low: false,
            open_high: false,
        })
    } else {
        None
    }
}

/// The overlap of two intervals, empty when they only share an open end.
fn intersect(left: Span, right: Span) -> Option<Span> {
    let (low, open_low) = if right.low > left.low {
        (right.low, right.open_low)
    } else if right.low < left.low {
        (left.low, left.open_low)
    } else {
        (left.low, left.open_low || right.open_low)
    };
    let (high, open_high) = if right.high < left.high {
        (right.high, right.open_high)
    } else if right.high > left.high {
        (left.high, left.open_high)
    } else {
        (left.high, left.open_high || right.open_high)
    };
    (low < high || (low == high && !open_low && !open_high)).then_some(Span {
        low,
        high,
        open_low,
        open_high,
    })
}

/// Fill the `Width=` band of one segment as one horizontal run per pixel row.
///
/// `Width=` is a continuous logical-pixel width, not a row count. Every row
/// whose center the band covers becomes one horizontal `SurfaceLine`: the shared
/// `XSurface7BA610`/`DSurface4BEAC0` walk stores `dx + 1` pixels for an
/// axis-aligned line with equal Z adjustments, so a run keeps the native Mix,
/// ABuffer and depth behavior, including the depth sample the raster takes from
/// the row the pixel is stored on. The band is half-open (`-half < perp ≤ half`),
/// so an axis-aligned segment fills exactly `width` rows and an even width keeps
/// the middle row that rounded parallel offsets skip. Caps are flat, and
/// `uncovered_runs` settles the shared endpoint with the previous segment.
fn band_rows(
    from: [i32; 2],
    to: [i32; 2],
    width: u8,
    z_adjust: [i32; 2],
    blend: SurfaceLineBlend,
    clip: [i32; 4],
    covered: &[(i32, i32, i32)],
    emitted: &mut Vec<(i32, i32, i32)>,
    mut emit: impl FnMut(SurfaceLine),
) {
    let [dx, dy] = [f64::from(to[0] - from[0]), f64::from(to[1] - from[1])];
    let length = (dx * dx + dy * dy).sqrt();
    if !(length >= 0.5) || clip[2] <= 0 || clip[3] <= 0 {
        return;
    }
    let [fx, fy] = [f64::from(from[0]), f64::from(from[1])];
    let [za, zb] = [f64::from(z_adjust[0]), f64::from(z_adjust[1])];
    let squared = length * length;
    let half = f64::from(width) / 2.0;
    let band = half * length;
    // Butt caps and the band are convex, so each row keeps one `x` interval.
    let first = (fy.min(f64::from(to[1])) - half - 1.0).ceil() as i32;
    let last = (fy.max(f64::from(to[1])) + half + 1.0).floor() as i32;
    for row in first.max(0)..=last.min(clip[3] - 1) {
        let uy = f64::from(row) - fy;
        // along = d·(p - from), across = (-dy, dx)·(p - from).
        let along_c = dy * uy - dx * fx;
        let across_c = dy * fx + dx * uy;
        let along = span(dx, along_c, 0.0, false, squared, false);
        let across = span(-dy, across_c, -band, true, band, false);
        let Some(run) = along.and_then(|along| across.and_then(|across| intersect(along, across)))
        else {
            continue;
        };
        let start = (if run.open_low {
            run.low.floor() + 1.0
        } else {
            run.low.ceil()
        }) as i32;
        let end = (if run.open_high {
            run.high.ceil() - 1.0
        } else {
            run.high.floor()
        }) as i32;
        let (start, end) = (start.max(0), end.min(clip[2] - 1));
        if start > end {
            continue;
        }
        // The run's Z adjustment follows the center line, so the raster's own
        // per-pixel store keeps the depth the center line had there.
        let z_at = |x: i32| {
            let along = ((dx * f64::from(x) + along_c) / squared).clamp(0.0, 1.0);
            (za + (zb - za) * along).round() as i32
        };
        uncovered_runs(start, end, row, covered, |start, end| {
            let z_from = z_at(start);
            // A zero-length line has no walk at all: the raster counts pixels
            // along its dominant axis, and every axis is zero. A cap corner or
            // the joint with the previous segment can leave such a sliver; one
            // Z step is the raster's documented Z-dominant store of that single
            // pixel, which is the same Mix the run itself would have made.
            let z_adjust = if start == end {
                [z_from, z_from.wrapping_add(1)]
            } else {
                [z_from, z_at(end)]
            };
            emitted.push((row, start, end));
            emit(SurfaceLine {
                from: [start, row],
                to: [end, row],
                z_adjust,
                blend,
            });
        });
    }
}

/// Emit the parts of `start..=end` on `row` that `covered` leaves free.
///
/// `covered` holds one segment's runs sorted by row. Two adjacent segments meet
/// at a shared sample, where both bands cover the same pixels; the older
/// segment keeps them, so the newer one only fills what its predecessor left.
fn uncovered_runs(
    start: i32,
    end: i32,
    row: i32,
    covered: &[(i32, i32, i32)],
    mut emit: impl FnMut(i32, i32),
) {
    let first = covered.partition_point(|&(covered_row, _, _)| covered_row < row);
    let mut cursor = start;
    for &(covered_row, covered_start, covered_end) in &covered[first..] {
        if covered_row != row || covered_start > end {
            break;
        }
        if covered_end < cursor {
            continue;
        }
        if covered_start > cursor {
            emit(cursor, covered_start - 1);
        }
        cursor = covered_end.saturating_add(1);
        if cursor > end {
            return;
        }
    }
    if cursor <= end {
        emit(cursor, end);
    }
}

#[cfg(test)]
#[path = "vera_trails_tests.rs"]
mod tests;
