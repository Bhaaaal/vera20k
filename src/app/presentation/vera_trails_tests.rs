//! VeraX trail history regressions: sampling, fading, ordering, the `Width=`
//! band and the head-to-tail colour gradient.

use super::{OwnerKind, VeraTrails};
use crate::render::surface_line::{SurfaceDraw, SurfaceLineBlend};
use crate::rules::vera_trail::VeraTrailStyle;

/// The tactical logical-pixel rect the runs are clipped to.
const CLIP: [i32; 4] = [0, 0, 640, 480];
/// Projection origin: 2048 leptons east is 240 logical pixels right and 120
/// down, plus the shared 15-row bias.
const START: [i32; 3] = [2048, 0, 0];

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

/// One trail holding `coords`, sampled 100 ms apart, aged to `now_ms`.
fn sampled(style: &VeraTrailStyle, coords: &[[i32; 3]], now_ms: u64) -> VeraTrails {
    let mut trails = VeraTrails::default();
    trails.begin_frame();
    for (step, coord) in coords.iter().enumerate() {
        trails.sample(OwnerKind::Entity, 7, 0, style, *coord, step as u64 * 100);
    }
    trails.end_frame(now_ms);
    trails
}

fn draws(trails: &VeraTrails, now_ms: u64, detail_level: u8) -> Vec<SurfaceDraw> {
    trails.draws(now_ms, [0, 0], CLIP, detail_level, |_| false)
}

/// The endpoints of every drawn line, in draw order.
fn lines(draws: &[SurfaceDraw]) -> Vec<([i32; 2], [i32; 2])> {
    draws
        .iter()
        .map(|draw| match draw {
            SurfaceDraw::Line(line) => (line.from, line.to),
            _ => panic!("a VeraX trail only emits lines"),
        })
        .collect()
}

/// `(x start, x end, row)` of every run.
fn runs(draws: &[SurfaceDraw]) -> Vec<(i32, i32, i32)> {
    lines(draws)
        .into_iter()
        .map(|(from, to)| {
            assert_eq!(from[1], to[1], "a width band fills a horizontal run");
            (from[0], to[0], from[1])
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
    assert_eq!(draws(&trails, 1500, 2).len(), 0);
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
    assert!(draws(&trails, 200, 2).len() >= 1);
    assert_eq!(trails.trail_count(), 1);

    trails.begin_frame();
    trails.end_frame(5000);
    assert_eq!(trails.trail_count(), 0);
}

#[test]
fn a_wide_vertical_trail_fills_one_run_of_its_width_per_row() {
    let mut wide = style();
    wide.width = 3;
    // 512 leptons east and south of START are (0, 60): straight down the screen.
    let down = [START, [2560, 512, 0]];
    let band = runs(&draws(&sampled(&wide, &down, 100), 100, 2));
    let rows: Vec<i32> = band.iter().map(|run| run.2).collect();
    assert_eq!(
        rows,
        (rows[0]..=rows[0] + 60).collect::<Vec<_>>(),
        "a vertical band fills one run per row"
    );
    assert!(
        band.iter().all(|run| run.1 - run.0 + 1 == 3),
        "every row is Width=3 columns wide: {band:?}"
    );

    let narrow = lines(&draws(&sampled(&wide, &down, 100), 100, 0));
    assert_eq!(
        narrow.len(),
        1,
        "detail level 0 narrows the band to one line"
    );
}

#[test]
fn a_wide_horizontal_trail_fills_the_row_its_center_line_covers() {
    let mut wide = style();
    wide.width = 6;
    // 512 leptons east and south are (120, 0): a horizontal screen segment.
    let path = [START, [2560, -512, 0]];
    let band = runs(&draws(&sampled(&wide, &path, 100), 100, 2));
    let single = lines(&draws(&sampled(&style(), &path, 100), 100, 2));
    let (start, end) = (single[0].0[0], single[0].1[0]);
    assert_eq!(end - start, 120, "the segment is 120 logical pixels long");

    assert_eq!(band.len(), 6, "Width=6 fills six rows, whatever its parity");
    let rows: Vec<i32> = band.iter().map(|run| run.2).collect();
    assert_eq!(rows, (rows[0]..=rows[0] + 5).collect::<Vec<_>>());
    assert!(
        rows.contains(&single[0].0[1]),
        "the band covers the center line's own row, got {rows:?}"
    );
    assert!(
        band.iter().all(|run| (run.0, run.1) == (start, end)),
        "every row spans the whole segment: {band:?}"
    );
}

#[test]
fn a_diagonal_width_band_is_one_continuous_run_per_row() {
    let mut wide = style();
    wide.width = 6;
    // 384 east and 128 south leptons are (30, 30): a 45° screen diagonal.
    let path = [START, [2432, 128, 0]];
    let band = runs(&draws(&sampled(&wide, &path, 100), 100, 2));

    let rows: Vec<i32> = band.iter().map(|run| run.2).collect();
    assert_eq!(
        rows,
        (rows[0]..=*rows.last().unwrap()).collect::<Vec<_>>(),
        "every row between the first and the last is filled"
    );
    // One row crosses width/cos(45°) = 6·√2 = 8.49 pixels of the band; only the
    // two end rows are shortened, by the flat caps.
    let widest = band.iter().map(|run| run.1 - run.0 + 1).max().unwrap();
    assert!(
        (8..=9).contains(&widest),
        "one row spans 8 or 9 pixels, got {widest}"
    );
    assert!(
        band.windows(2)
            .all(|pair| pair[1].0 <= pair[0].1 + 1 && pair[0].0 <= pair[1].1 + 1),
        "neighbouring rows overlap or touch: {band:?}"
    );
}

#[test]
fn width_one_keeps_the_native_single_line() {
    // 512 leptons east and south of START are (0, 60): straight down the screen.
    let down = [START, [2560, 512, 0]];
    let single = lines(&draws(&sampled(&style(), &down, 100), 100, 2));
    assert_eq!(single.len(), 1);
    let (from, to) = single[0];
    assert_eq!(
        (to[0] - from[0], to[1] - from[1]),
        (0, 60),
        "the segment's own endpoints, not a band run"
    );

    let mut wide = style();
    wide.width = 8;
    assert_eq!(
        lines(&draws(&sampled(&wide, &down, 100), 100, 0)),
        single,
        "low detail narrows a wide trail to that same line"
    );
}

#[test]
fn two_segments_leave_no_gap_at_their_shared_endpoint() {
    let mut wide = style();
    wide.width = 4;
    // East, then a right angle onto screen-south through the same sample.
    let path = [START, [2560, -512, 0], [3072, 0, 0]];
    let band = runs(&draws(&sampled(&wide, &path, 200), 200, 2));

    let mut by_row: std::collections::BTreeMap<i32, Vec<(i32, i32)>> = Default::default();
    for &(start, end, row) in &band {
        by_row.entry(row).or_default().push((start, end));
    }
    let rows: Vec<i32> = by_row.keys().copied().collect();
    assert_eq!(
        rows,
        (rows[0]..=*rows.last().unwrap()).collect::<Vec<_>>(),
        "the two bands leave no empty row across their joint: {band:?}"
    );
    assert!(
        by_row.values().any(|runs| runs.len() == 2),
        "the joint row carries both segments: {by_row:?}"
    );
    // The older segment keeps the pixels both bands cover, so a row never Mixes
    // one pixel twice and never leaves a column between the two runs.
    for (row, mut runs) in by_row {
        runs.sort_unstable();
        for pair in runs.windows(2) {
            assert!(
                pair[0].1 < pair[1].0,
                "row {row} covers {pair:?} twice in {runs:?}"
            );
            assert!(
                pair[1].0 <= pair[0].1 + 1,
                "row {row} leaves a column between {pair:?}"
            );
        }
    }
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

    let drawn = draws(&trails, 100, 2);
    let reference = draws(&only_front, 100, 2);
    assert_eq!(drawn.len(), 2, "a single segment per trail");
    match (&drawn[0], &reference[0]) {
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
    let drawn = draws(&trails, 900, 2);
    let SurfaceDraw::Line(line) = drawn[0] else {
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
