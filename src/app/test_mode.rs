//! The Skirmish screen's Test Mode switch, for builds that carry the dev
//! overlays but not the browser dashboard.
//!
//! `dev-overlays` compiles the engine's offline trainer (`sim::cheats`); a
//! `debug-tools` build reaches the same owner through `app::debug_tools`, whose
//! dashboard admission already honours the switch, so this module exists only
//! without that feature (`#[cfg(all(dev-overlays, not(debug-tools)))]` at its
//! declaration).
//!
//! It writes nothing of its own: the trainer keeps the flags, the simulation
//! applies them at its master-frame boundary, and `sandbox_full_visibility`
//! stays the app's own presentation switch (`simulation.debug_reveal_map()`
//! answers it). A network or replay session is refused by the trainer itself,
//! so a switch left on cannot desync a match or a recording.

use crate::app::state::AppState;
use crate::sim::cheats::{CheatFlags, DebugSessionContext};

/// Per-frame admission, called once after the runtime frame's input service.
pub(crate) fn service(state: &mut AppState) {
    let test_mode = state.match_state.test_mode;
    // Only an offline match admits the trainer; the simulated session answers
    // the rest (a network match gets Network and every write is refused).
    let session_mode = state.match_state.session_mode();
    let offline = matches!(
        session_mode,
        crate::app::match_runtime::session::SessionMode::Campaign
            | crate::app::match_runtime::session::SessionMode::Skirmish
    );
    if state.match_state.sim_runtime.is_none() {
        return;
    }
    let context = if !test_mode {
        DebugSessionContext::Disabled
    } else if offline {
        DebugSessionContext::Offline
    } else {
        DebugSessionContext::Network
    };
    let Some(runtime) = state.match_state.sim_runtime.as_mut() else {
        return;
    };
    runtime.simulation.set_debug_session_context(context);
    if !test_mode || !runtime.simulation.trainer_available() {
        state.match_state.sandbox_full_visibility = runtime.simulation.debug_reveal_map();
        return;
    }
    #[cfg(feature = "dev-overlays")]
    if !state.renderer.test_hud.is_visible() {
        state.renderer.test_hud.set_visible(true);
    }
    let wanted = CheatFlags {
        infinite_money: true,
        money_target: crate::sim::cheats::MAX_MONEY_TARGET,
        instant_build: true,
        super_weapon_ready: true,
        reveal_map: true,
        ..CheatFlags::default()
    };
    if runtime.simulation.cheat_flags() != wanted {
        log::info!("Test Mode: trainer flags applied (money, instant build, reveal)");
        if let Err(error) = runtime.simulation.set_cheat_flags(wanted) {
            log::warn!("Test Mode: trainer refused the flags: {error}");
        }
    }
    runtime
        .simulation
        .apply_debug_tools(&runtime.resources.rules);
    state.match_state.sandbox_full_visibility = runtime.simulation.debug_reveal_map();
}
