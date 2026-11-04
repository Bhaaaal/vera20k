//! VeraX custom trails: presentation history for `--features verax`.
//!
//! One owner keeps every trail's samples keyed by (owner kind, stable id,
//! catalog index). Sampling reads the simulation's committed anchor each
//! composite and never runs inside the simulation: no tick, save, replay or
//! RNG state changes. Aging uses wall-clock milliseconds, so a trail keeps the
//! length its `FadeDuration=` asked for whatever the display rate is.
//!
//! Each segment becomes one `Mix` surface line per pixel row of its `Width=`.
//! A segment's colour interpolates from `Color=` at the head toward `Color2=`
//! at the tail, and its strength fades with the older sample's age.

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
    /// order. `hidden` gates a segment whose newer end is concealed.
    pub(crate) fn draws(
        &self,
        now_ms: u64,
        camera: [i32; 2],
        detail_level: u8,
        mut hidden: impl FnMut([i32; 2]) -> bool,
    ) -> Vec<SurfaceDraw> {
        let mut ordered: Vec<(i32, usize, SurfaceDraw)> = Vec::new();
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
            for slot in 0..count.saturating_sub(1) {
                let newer = trail.samples[slot + 1];
                let older = trail.samples[slot];
                let age_ms = now_ms.saturating_sub(newer.at_ms);
                let faded = (i64::from(alpha) * age_ms as i64 / fade as i64) as i32;
                let strength = (i32::from(alpha) - faded).max(0);
                if strength < MIN_STRENGTH {
                    continue;
                }
                let older_age = now_ms.saturating_sub(older.at_ms) as f64 / fade as f64;
                let rgb = if detailed {
                    gradient_color(&style, older_age.clamp(0.0, 1.0))
                } else {
                    style.color
                };
                let [from, to] = [older.coord, newer.coord].map(|coord| project(coord, camera));
                if hidden(from) || hidden(to) {
                    continue;
                }
                for row in 0..width {
                    let [start, end] = match width {
                        1 => [from, to],
                        _ => offset_row(from, to, row, width),
                    };
                    ordered.push((
                        style.z_order,
                        sequence,
                        SurfaceDraw::Line(SurfaceLine {
                            from: start,
                            to: end,
                            z_adjust: [from_z(older.coord[2]), from_z(newer.coord[2])],
                            blend: SurfaceLineBlend::Mix { rgb, strength },
                        }),
                    ));
                }
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

/// Shift one pixel row of a thick line along the segment's normal.
fn offset_row(from: [i32; 2], to: [i32; 2], row: u8, width: u8) -> [[i32; 2]; 2] {
    let dx = f64::from(to[0] - from[0]);
    let dy = f64::from(to[1] - from[1]);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.5 {
        return [from, to];
    }
    let offset = f64::from(row) - f64::from(width - 1) / 2.0;
    let [ox, oy] = [
        (-dy / length * offset).round() as i32,
        (dx / length * offset).round() as i32,
    ];
    [
        [from[0].wrapping_add(ox), from[1].wrapping_add(oy)],
        [to[0].wrapping_add(ox), to[1].wrapping_add(oy)],
    ]
}

#[cfg(test)]
#[path = "vera_trails_tests.rs"]
mod tests;
