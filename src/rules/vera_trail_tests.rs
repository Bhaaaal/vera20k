//! VeraX trail catalog regressions: ART reading, clamps and list resolution.

use super::{MAX_TRAILS, MAX_WIDTH, VeraTrailCatalog, VeraTrailStyle};
use crate::rules::ini_parser::IniFile;

fn catalog(art: &str) -> VeraTrailCatalog {
    VeraTrailCatalog::from_ini(&IniFile::from_str(art))
}

#[test]
fn catalog_reads_every_style_key_and_clamps_its_ranges() {
    let catalog = catalog(
        "[VeraTrailTypes]\n0=WING\n1=NOZZLE\n\
         [WING]\nFLH=60,-120,40\nColor=255,64,0\nColor2=255,0,0\n\
         Width=99\nFadeDuration=900\nSegmentLength=40\nAlpha=180\nZOrder=-3\n\
         [NOZZLE]\nColor=0,128,255\nWidth=0\nAlpha=300\nFadeDuration=0\n",
    );
    let wing = catalog.style("wing").expect("case-insensitive entry");
    assert_eq!(wing.flh, [60, -120, 40]);
    assert_eq!(wing.color, [255, 64, 0]);
    assert_eq!(wing.color2, Some([255, 0, 0]));
    assert_eq!(wing.width, MAX_WIDTH);
    assert_eq!(wing.fade_ms, 900);
    assert_eq!(wing.segment_length, 40);
    assert_eq!(wing.alpha, 180);
    assert_eq!(wing.z_order, -3);

    let nozzle = catalog.style("NOZZLE").expect("exact entry");
    assert_eq!(nozzle.flh, [0; 3], "absent FLH keeps the zero default");
    assert_eq!(nozzle.color, [0, 128, 255]);
    assert_eq!(nozzle.color2, None, "absent Color2 leaves the trail flat");
    assert_eq!(nozzle.width, 1, "width clamps up to one pixel");
    assert_eq!(nozzle.alpha, 255, "alpha clamps to 0..=255");
    assert_eq!(nozzle.fade_ms, 1, "a zero fade still keeps one sample time");
    assert_eq!(nozzle.segment_length, 0);
    assert_eq!(nozzle.z_order, 0);
}

#[test]
fn malformed_colours_keep_the_previous_value() {
    let catalog = catalog(
        "[VeraTrailTypes]\n0=BROKEN\n[WING]\nColor=10,20,30\n\
         [BROKEN]\nColor=1,2\nColor2=not_a_colour\n",
    );
    let style = catalog.style("BROKEN").expect("entry");
    // A partial triple keeps the reader's default; a non-numeric tail keeps
    // the value it inherits from Color (native ReadColorRGB only scan-fails
    // on non-numeric text).
    assert_eq!(style.color, VeraTrailStyle::default().color);
    assert_eq!(style.color2, Some(style.color));
}

#[test]
fn style_lists_resolve_names_case_insensitively_skip_unknowns_and_cap() {
    let mut art = String::from("[VeraTrailTypes]\n");
    for index in 0..(MAX_TRAILS + 1) {
        art.push_str(&format!("{index}=T{index}\n"));
    }
    for index in 0..(MAX_TRAILS + 1) {
        art.push_str(&format!("[T{index}]\nColor={index},0,0\n"));
    }
    let catalog = catalog(&art);

    let names = (0..=MAX_TRAILS)
        .map(|index| format!("t{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let list = format!("{names},MISSING, T0");
    let styles = catalog.resolve(&list);
    assert_eq!(styles.len(), MAX_TRAILS, "the list stops at the hard cap");
    for (index, style) in styles.iter().enumerate() {
        assert_eq!(style.color, [index as u8, 0, 0], "catalog order is kept");
    }

    assert!(catalog.resolve("MISSING,ALSO_MISSING").is_empty());
    assert!(catalog.resolve("").is_empty());
    assert!(VeraTrailCatalog::default().resolve("ANY").is_empty());
}

#[test]
fn a_missing_catalog_section_yields_no_entries() {
    let catalog = catalog("[WING]\nColor=255,0,0\n");
    assert!(catalog.is_empty());
    assert!(catalog.style("WING").is_none());
}

#[test]
fn the_art_registry_resolves_a_type_section_list() {
    use crate::rules::art_data::ArtRegistry;

    let art = IniFile::from_str(
        "[VeraTrailTypes]\n0=WING\n[WING]\nColor=10,20,30\nWidth=2\n\
         [FLAC]\nVeraTrail.Types=wing\n[PLAIN]\n",
    );
    let registry = ArtRegistry::from_ini(&art);
    let styles = registry.vera_trail_styles("FLAC");
    assert_eq!(styles.len(), 1);
    assert_eq!(styles[0].color, [10, 20, 30]);
    assert_eq!(styles[0].width, 2);

    assert!(registry.vera_trail_styles("PLAIN").is_empty());
    assert!(registry.vera_trail_styles("MISSING").is_empty());
}

#[test]
fn the_intruder_example_art_fixture_resolves_three_trails() {
    use crate::rules::art_data::ArtRegistry;

    let art = IniFile::from_str(crate::test_fixture::text(
        "tests/fixtures/ini/verax_trails_art_example.ini",
    ));
    let registry = ArtRegistry::from_ini(&art);
    let styles = registry.vera_trail_styles("FALC");
    assert_eq!(styles.len(), 3, "two wing tips and one exhaust");
    assert_eq!(styles[0].flh, [0, -90, 10]);
    assert_eq!(styles[0].color, [255, 255, 255]);
    assert_eq!(styles[0].color2, Some([96, 160, 255]));
    assert_eq!(styles[0].z_order, 1);
    assert_eq!(styles[1].flh, [0, 90, 10]);
    assert_eq!(styles[2].flh, [-80, 0, 5]);
    assert_eq!(styles[2].z_order, 0, "the exhaust draws behind the wings");
}
