//! VeraX trail history regressions: sampling, fading, ordering, width and the
//! head-to-tail colour gradient.

use super::{OwnerKind, VeraTrails};
use crate::render::surface_line::{SurfaceDraw, SurfaceLineBlend};
use crate::rules::vera_trail::VeraTrailStyle;

fn style() -> VeraTrailStyle {
    VeraTrailStyle {
        flh: [0; 3],
        color: [255, 0, 0],
        color2: None,
        width: 1,
        fade_ms: 1000,
        segment_length: 0,
        alpha: 255,
        z_order: 0,
    }
}

fn feed(trails: &mut VeraTrails, coord: [i32; 3], now_ms: u64, style: &VeraTrailStyle) {
    trails.begin_frame();
    trails.sample(OwnerKind::Entity, 7, 0, style, coord, now_ms);
    trails.end_frame(now_ms);
}

fn lines(draws: &[SurfaceDraw]) -> Vec<[i32; 2]> {
    draws
        .iter()
        .map(|draw| match draw {
            SurfaceDraw::Line(line) => line.from,
            _ => panic!("a VeraX trail only emits lines"),
        })
        .collect()
}

#[test]
fn samples_append_on_movement_and_respect_segment_length() {
    let mut trails = VeraTrails::default();
    feed(&mut trails, [0, 0, 0], 0, &style());
    feed(&mut trails, [0, 0, 0], 16, &style());
    assert_eq!(trails.sample_len(OwnerKind::Entity, 7, 0), 1);

    let mut gated = style();
    gated.segment_length = 512;
    let mut trails = VeraTrails::default();
    feed(&mut trails, [0, 0, 0], 0, &gated);
    feed(&mut trails, [256, 0, 0], 16, &gated);
    assert_eq!(
        trails.sample_len(OwnerKind::Entity, 7, 0),
        1,
        "not far enough"
    );
    feed(&mut trails, [1024, 0, 0], 32, &gated);
    assert_eq!(
        trails.sample_len(OwnerKind::Entity, 7, 0),
        2,
        "past the gate"
    );
}

#[test]
fn a_stationary_owner_fades_out_and_retires() {
    let mut trails = VeraTrails::default();
    feed(&mut trails, [0, 0, 0], 0, &style());
    feed(&mut trails, [256, 0, 0], 32, &style());
    assert_eq!(trails.sample_len(OwnerKind::Entity, 7, 0), 2);

    // The owner stops moving; past the fade every sample ages out even
    // though the owner is still there.
    feed(&mut trails, [256, 0, 0], 1500, &style());
    assert_eq!(trails.sample_len(OwnerKind::Entity, 7, 0), 0);
    assert_eq!(trails.draws(1500, [0, 0], 2, |_| false).len(), 0);
}

#[test]
fn a_removed_owner_leaves_its_trail_to_fade() {
    let mut trails = VeraTrails::default();
    trails.begin_frame();
    trails.sample(OwnerKind::Entity, 9, 0, &style(), [0, 0, 0], 0);
    trails.sample(OwnerKind::Entity, 9, 0, &style(), [512, 0, 0], 100);
    trails.end_frame(100);

    // No samples this frame: the owner is gone.
    trails.begin_frame();
    trails.end_frame(200);
    assert!(trails.draws(200, [0, 0], 2, |_| false).len() >= 1);
    assert_eq!(trails.trail_count(), 1);

    trails.begin_frame();
    trails.end_frame(5000);
    assert_eq!(trails.trail_count(), 0);
}

#[test]
fn width_draws_one_line_per_row_and_detail_zero_narrows_it() {
    let mut wide = style();
    wide.width = 3;
    let mut trails = VeraTrails::default();
    feed(&mut trails, [0, 0, 0], 0, &wide);
    feed(&mut trails, [512, 0, 0], 100, &wide);
    let draws = trails.draws(100, [0, 0], 2, |_| false);
    assert_eq!(draws.len(), 3);

    let rows = lines(&draws);
    assert!(
        rows[0][1] < rows[1][1] && rows[1][1] < rows[2][1],
        "rows stack along the segment normal"
    );
    assert_eq!(trails.draws(100, [0, 0], 0, |_| false).len(), 1);
}

#[test]
fn z_order_sorts_draws_independently_of_the_catalog_list() {
    let mut back = style();
    back.z_order = 5;
    let mut front = style();
    front.z_order = -2;
    let mut trails = VeraTrails::default();
    trails.begin_frame();
    trails.sample(OwnerKind::Entity, 3, 0, &back, [0, 0, 0], 0);
    trails.sample(OwnerKind::Entity, 3, 0, &back, [512, 0, 0], 100);
    trails.sample(OwnerKind::Entity, 3, 1, &front, [0, 1024, 0], 0);
    trails.sample(OwnerKind::Entity, 3, 1, &front, [512, 1024, 0], 100);
    trails.end_frame(100);

    // The front trail alone must produce the same first line, proving the
    // ordering comes from `ZOrder=`, not from the order in the list.
    let mut only_front = VeraTrails::default();
    only_front.begin_frame();
    only_front.sample(OwnerKind::Entity, 3, 1, &front, [0, 1024, 0], 0);
    only_front.sample(OwnerKind::Entity, 3, 1, &front, [512, 1024, 0], 100);
    only_front.end_frame(100);

    let draws = trails.draws(100, [0, 0], 2, |_| false);
    let reference = only_front.draws(100, [0, 0], 2, |_| false);
    assert_eq!(draws.len(), 2, "a single segment per trail");
    match (&draws[0], &reference[0]) {
        (SurfaceDraw::Line(first), SurfaceDraw::Line(expected)) => assert_eq!(first, expected),
        _ => panic!("lines only"),
    }
}

#[test]
fn colour2_grades_into_the_tail_colour() {
    let mut graded = style();
    graded.color2 = Some([0, 0, 255]);
    let mut trails = VeraTrails::default();
    feed(&mut trails, [0, 0, 0], 0, &graded);
    feed(&mut trails, [512, 0, 0], 100, &graded);
    // At 1000 ms the tail sample is fully faded: only the fresher head remains
    // coloured by its own age, so ask near the tail instead.
    let draws = trails.draws(900, [0, 0], 2, |_| false);
    let SurfaceDraw::Line(line) = draws[0] else {
        panic!("line")
    };
    let SurfaceLineBlend::Mix { rgb, .. } = line.blend else {
        panic!("mix")
    };
    assert!(
        rgb[0] < 60 && rgb[2] > 200,
        "the older sample is mostly Color2, got {rgb:?}"
    );
}
