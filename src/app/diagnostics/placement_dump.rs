//! Test Mode's record of a refused building placement.
//!
//! The skirmish screen's Test Mode switch admits the trainer and full map
//! vision; this adds the record a placement mismatch needs. When the local
//! gate refuses a click, the whole attempt goes to `logs/ra2.log`: the area the
//! overlay would draw, the footprint the click would put down, each gate that
//! refused it, and an ASCII map of both.
//!
//! `sim::production::build_area_report` owns the report; this module only
//! decides when a match in Test Mode writes one.

use crate::app::AppState;

/// Log one refused placement attempt. A match without Test Mode writes nothing.
pub(crate) fn record(
    state: &AppState,
    owner: &str,
    type_id: &str,
    cursor: (u16, u16),
    anchor: (u16, u16),
) {
    if !state.match_state.test_mode {
        return;
    }
    let (Some(runtime), Some(rules)) = (state.match_state.sim_runtime.as_ref(), state.rules())
    else {
        return;
    };
    let Some(report) = crate::sim::production::build_area_report(
        &runtime.simulation,
        rules,
        owner,
        type_id,
        cursor,
        anchor,
        state.overlay_registry(),
    ) else {
        return;
    };
    log::warn!("{report}");
}
