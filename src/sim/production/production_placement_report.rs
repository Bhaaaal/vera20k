//! Test-mode record of a refused building placement.
//!
//! VERA's placement overlay and its admission share one decision, but a player
//! can still meet sites where the light green area looks usable and the click is
//! refused. This report answers "why" for one attempt: the area the overlay
//! draws, the footprint the click would put down, each gate that refused it, and
//! an ASCII map of both. It never decides anything — the caller logs the text.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::map::resolved_terrain::NativeCellQuery;
use crate::rules::locomotor_type::SpeedType;
use crate::rules::object_type::ObjectType;
use crate::rules::overlay_types::OverlayTypeRegistry;
use crate::rules::ruleset::RuleSet;
use crate::sim::build_site::{
    GroundAdmission, building_speed_type, cell_holds_structure, ground_admission,
};
use crate::sim::intern::InternedId;
use crate::sim::world::Simulation;

use super::{base_provider_radius, build_area_cells, is_within_build_area};

/// Cells of the map window the report prints around the attempted site.
const WINDOW: i32 = 12;

/// The report for one refused placement, or `None` when the type or terrain is
/// missing.
#[allow(clippy::too_many_arguments)]
pub fn build_area_report(
    sim: &Simulation,
    rules: &RuleSet,
    owner: &str,
    type_id: &str,
    cursor: (u16, u16),
    anchor: (u16, u16),
    overlay_registry: Option<&OverlayTypeRegistry>,
) -> Option<String> {
    let obj = rules.object(type_id)?;
    let terrain = sim.resolved_terrain.as_ref()?;
    let cells = NativeCellQuery::isolated(terrain);
    let speed = building_speed_type(obj);
    let owner_id = sim.interner.get(owner);
    let area = build_area_cells(sim, rules, owner, obj, overlay_registry);
    let allowed: HashSet<(u16, u16)> = area.allowed.iter().map(|cell| cell.coord).collect();
    let blocked: HashSet<(u16, u16)> = area.blocked.iter().map(|cell| cell.coord).collect();
    let footprint: HashSet<(u16, u16)> = obj
        .foundation
        .cell_offsets()
        .into_iter()
        .map(|(dx, dy)| {
            (
                anchor.0.wrapping_add_signed(dx),
                anchor.1.wrapping_add_signed(dy),
            )
        })
        .collect();
    let (width, height) = obj.foundation.dimensions();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "placement refused: owner={owner} type={type_id} cursor=({},{}) anchor=({},{}) \
         foundation={width}x{height}",
        cursor.0, cursor.1, anchor.0, anchor.1,
    );
    let providers: Vec<String> = sim
        .substrate
        .entities
        .values()
        .filter_map(|entity| {
            let radius = base_provider_radius(sim, rules, owner, entity)?;
            Some(format!(
                "{}@({},{}) radius={radius} reach={}",
                sim.interner.resolve(entity.type_ref()),
                entity.position.rx(),
                entity.position.ry(),
                radius.saturating_add(obj.place_extra_range),
            ))
        })
        .collect();
    let _ = writeln!(
        out,
        "  in_build_area={} place_extra_range={} providers=[{}]",
        is_within_build_area(sim, rules, owner, obj, anchor.0, anchor.1),
        obj.place_extra_range,
        providers.join(", "),
    );
    let viewer = sim.session.current_house();
    for (dx, dy) in obj.foundation.cell_offsets() {
        let coord = (
            anchor.0.wrapping_add_signed(dx),
            anchor.1.wrapping_add_signed(dy),
        );
        let identity = cells.lookup((coord.0 as i16, coord.1 as i16));
        let ground = ground_admission(
            sim,
            rules,
            overlay_registry,
            identity,
            speed,
            Some(obj),
            owner_id,
            &cells,
        );
        let shroud = viewer.is_some_and(|viewer| {
            crate::sim::vision::cell_is_shrouded(&sim.fog, &cells, viewer, coord)
        });
        let _ = writeln!(
            out,
            "  cell ({},{}): area={} ground={} structure={} shroud={shroud}",
            coord.0,
            coord.1,
            area_state(&allowed, &blocked, coord),
            ground_name(ground),
            if cell_holds_structure(sim, &cells, identity) {
                "yes"
            } else {
                "no"
            },
        );
    }
    let bounds = allowed.iter().fold(None, |bounds, &(x, y)| match bounds {
        None => Some((x, y, x, y)),
        Some((x0, y0, x1, y1)) => Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y))),
    });
    match bounds {
        Some((x0, y0, x1, y1)) => {
            let _ = writeln!(
                out,
                "  area: green={} grey={} green_bounds=({x0},{y0})..({x1},{y1})",
                allowed.len(),
                blocked.len(),
            );
        }
        None => {
            let _ = writeln!(out, "  area: green=0 grey={}", blocked.len());
        }
    }
    let _ = writeln!(
        out,
        "  map: x right, y down | G=green #=grey ~=water .=outside area \
         o=footprint cell green X=footprint cell refused"
    );
    for y in anchor.1 as i32 - WINDOW..=anchor.1 as i32 + WINDOW {
        let mut row = String::new();
        for x in anchor.0 as i32 - WINDOW..=anchor.0 as i32 + WINDOW {
            row.push(map_char(
                sim,
                rules,
                overlay_registry,
                &cells,
                speed,
                obj,
                owner_id,
                (&allowed, &blocked, &footprint),
                (x, y),
            ));
        }
        let _ = writeln!(out, "  {row}");
    }
    Some(out)
}

type AreaState<'a> = (
    &'a HashSet<(u16, u16)>,
    &'a HashSet<(u16, u16)>,
    &'a HashSet<(u16, u16)>,
);

/// The glyph one map cell gets. A footprint cell wins over the area state, so a
/// refused site is visible inside its own footprint.
#[allow(clippy::too_many_arguments)]
fn map_char(
    sim: &Simulation,
    rules: &RuleSet,
    overlay_registry: Option<&OverlayTypeRegistry>,
    cells: &NativeCellQuery<'_>,
    speed: Option<SpeedType>,
    obj: &ObjectType,
    owner_id: Option<InternedId>,
    area: AreaState<'_>,
    (x, y): (i32, i32),
) -> char {
    let terrain = cells.terrain();
    if x < 0 || y < 0 || x >= i32::from(terrain.width()) || y >= i32::from(terrain.height()) {
        return ' ';
    }
    let coord = (x as u16, y as u16);
    let (allowed, blocked, footprint) = area;
    if footprint.contains(&coord) {
        return if allowed.contains(&coord) { 'o' } else { 'X' };
    }
    if allowed.contains(&coord) {
        return 'G';
    }
    if blocked.contains(&coord) {
        return '#';
    }
    let ground = ground_admission(
        sim,
        rules,
        overlay_registry,
        cells.lookup((x as i16, y as i16)),
        speed,
        Some(obj),
        owner_id,
        cells,
    );
    if ground == GroundAdmission::Water {
        '~'
    } else {
        '.'
    }
}

fn area_state(
    allowed: &HashSet<(u16, u16)>,
    blocked: &HashSet<(u16, u16)>,
    coord: (u16, u16),
) -> &'static str {
    if allowed.contains(&coord) {
        "green"
    } else if blocked.contains(&coord) {
        "grey"
    } else {
        "outside"
    }
}

fn ground_name(ground: GroundAdmission) -> &'static str {
    match ground {
        GroundAdmission::Allowed => "Allowed",
        GroundAdmission::Water => "Water",
        GroundAdmission::Blocked => "Blocked",
    }
}
