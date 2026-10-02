//! Live checks against the real Philips set. Behind `live-runtime-tests`
//! because they need the TV powered and on the network.
//!
//! These stay read-only on purpose. The set's JointSPACE server dies
//! permanently under request bursts — only a mains power cycle revives it —
//! so an automated suite has no business writing to it, and even the reads
//! here lean on `TvManager`'s own request gate for spacing.
#![cfg(feature = "live-runtime-tests")]

mod common;

use std::sync::OnceLock;

use maison_backend::{
    broadlink::BroadlinkManager,
    tv::{TvManager, TvPower},
};

/// One shared manager, so every test goes through the *same* request gate.
/// Building one per test would give each its own gate and let the suite burst
/// the very server this module exists to protect.
static TV: OnceLock<TvManager> = OnceLock::new();

/// The living-room set and its blaster; each path can be overridden.
fn manager() -> &'static TvManager {
    TV.get_or_init(|| {
        let broadlink = BroadlinkManager::new(
            &common::env_path("BROADLINK_CODES_JSON_PATH", "broadlink-codes.json"),
            &common::env_path("CLIMATE_STATE_JSON_PATH", "climate-state.json"),
        )
        .expect("Broadlink manager should build");
        TvManager::new(&common::env_path("TV_JSON_PATH", "tv.json"), broadlink)
            .expect("TV manager should build")
    })
}

#[tokio::test]
async fn reports_a_power_state() {
    let _guard = common::serial().lock().await;
    let power = manager().power().await;
    // Any of the three is a valid answer; what matters is that the probe
    // resolves rather than hanging or panicking.
    assert!(matches!(
        power,
        TvPower::On | TvPower::Standby | TvPower::DeepStandby
    ));
}

#[tokio::test]
async fn status_is_consistent_with_power() {
    let _guard = common::serial().lock().await;
    let status = manager().status().await;
    assert!(status.configured, "tv.json should carry a host");
    if status.power == TvPower::On {
        let volume = status.volume.expect("a powered set reports its volume");
        assert!(volume.max > volume.min);
        assert!(volume.current <= volume.max);
    } else {
        // Volume and Ambilight are only read when the panel is on.
        assert!(status.volume.is_none());
    }
}

#[tokio::test]
async fn ambilight_topology_matches_a_three_sided_set() {
    let _guard = common::serial().lock().await;
    let manager = manager();
    if manager.power().await != TvPower::On {
        eprintln!("skipping: TV is not on");
        return;
    }
    let topology = manager
        .ambilight_topology()
        .await
        .expect("Ambilight topology should read");
    // 55PUS6753: 4 left, 8 top, 4 right, no bottom strip.
    assert_eq!(topology.get("bottom").and_then(|v| v.as_u64()), Some(0));
    assert!(topology.get("top").and_then(|v| v.as_u64()).unwrap_or(0) > 0);
}
