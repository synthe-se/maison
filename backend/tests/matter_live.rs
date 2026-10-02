//! Live round trip against a Matter window covering. Behind
//! `live-runtime-tests` because it needs a device with an open commissioning
//! window:
//!
//! ```text
//! MATTER_LIVE_CODE=34970112332 MATTER_TEST_ROOTS=1 \
//!   cargo test --features live-runtime-tests --test matter_live -- --nocapture
//! ```
//!
//! `MATTER_TEST_ROOTS=1` is for a simulated device (matter.js / chip example,
//! CSA test DAC); leave it unset for a real, certified switch. The device is
//! decommissioned at the end, so it can be paired again afterwards.
#![cfg(feature = "live-runtime-tests")]

mod common;

use std::{env, time::Duration};

use maison_backend::matter::{CoverCommand, CoverView, MatterManager};

async fn settle(manager: &MatterManager, id: &str) -> CoverView {
    // A simulated cover jumps to its target; a real motor takes its travel
    // time, so poll until it reports itself stopped.
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let view = manager.get(id).await.expect("status read");
        if view.motion != Some(maison_backend::matter::CoverMotion::Opening)
            && view.motion != Some(maison_backend::matter::CoverMotion::Closing)
            && view.open_percent == view.target_open_percent
        {
            return view;
        }
    }
    manager.get(id).await.expect("status read")
}

#[tokio::test]
async fn commission_drive_and_remove() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let Ok(code) = env::var("MATTER_LIVE_CODE") else {
        eprintln!("MATTER_LIVE_CODE unset; skipping");
        return;
    };
    let test_roots = env::var("MATTER_TEST_ROOTS").is_ok_and(|v| v == "1");
    let state_dir = common::temp_root("maison-matter-live");
    let trust_dir = common::workspace_root().join("matter-trust");
    let manager =
        MatterManager::with_paths(&state_dir, &trust_dir, test_roots).expect("manager builds");

    let cover = manager
        .commission(&code, "Volet test")
        .await
        .expect("commissioning succeeds");
    println!("commissioned: {cover:?}");
    assert!(cover.online);
    assert_eq!(manager.list().await.len(), 1);

    manager
        .command(&cover.id, CoverCommand::Close)
        .await
        .expect("close");
    let closed = settle(&manager, &cover.id).await;
    println!("after close: {closed:?}");
    assert_eq!(closed.open_percent, Some(0));

    manager
        .command(&cover.id, CoverCommand::OpenPercent(40))
        .await
        .expect("go to 40 %");
    let partial = settle(&manager, &cover.id).await;
    println!("after 40 %: {partial:?}");
    assert_eq!(partial.open_percent, Some(40));

    manager
        .command(&cover.id, CoverCommand::Open)
        .await
        .expect("open");
    let open = settle(&manager, &cover.id).await;
    println!("after open: {open:?}");
    assert_eq!(open.open_percent, Some(100));

    manager
        .command(&cover.id, CoverCommand::Stop)
        .await
        .expect("stop");

    let renamed = manager
        .rename(&cover.id, "Volet salon")
        .await
        .expect("rename");
    assert_eq!(renamed.name, "Volet salon");

    // State survives a restart of the manager (fabric + covers on disk).
    drop(manager);
    let manager =
        MatterManager::with_paths(&state_dir, &trust_dir, test_roots).expect("manager reloads");
    let reloaded = manager.get(&cover.id).await.expect("reloaded status");
    assert!(reloaded.online, "CASE session re-established after reload");
    assert_eq!(reloaded.name, "Volet salon");

    manager.remove(&cover.id).await.expect("remove");
    assert!(manager.list().await.is_empty());
    let _ = std::fs::remove_dir_all(&state_dir);
}
