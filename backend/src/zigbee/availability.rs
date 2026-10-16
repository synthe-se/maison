//! Is each lamp still there? A lamp that reports by itself (attribute reporting, asked at
//! its interview) says so at least every few minutes; one that does not is only noticed
//! when a ping goes unanswered. A lamp silent past its timeout is pinged (an On/Off read);
//! one that comes back gets the light it was asked for.
//!
//! Never blocking: the check advances one step per call (every chore of the event loop),
//! timed against stored deadlines, and the event loop serves commands between steps.

use std::time::Duration;

use tokio::time::{timeout, Instant};
use tracing::{debug, info, warn};

use super::{
    callbacks::drain_callbacks,
    context::EzspContext,
    device::{DiscoveredDevice, Hex, Reporting, ZigbeeDeviceType},
    driver::EZSP_COMMAND_TIMEOUT,
    zcl::{
        build_brightness_command_payload, build_color_temperature_command_payload,
        build_color_xy_command_payload, COLOR_CONTROL_CLUSTER_ID, LEVEL_CONTROL_CLUSTER_ID,
        ON_OFF_CLUSTER_ID, REPORT_MAX_INTERVAL_S,
    },
};
use crate::util::capped_backoff;

/// A lamp that does not report, silent this long, is pinged. Short: the network is quiet,
/// a ping is the only way to notice a wall switch.
const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(60);
/// A lamp that reports, silent this long (its longest report interval and a margin), is
/// pinged: its reports stopped.
const REPORTED_TIMEOUT: Duration = Duration::from_secs(REPORT_MAX_INTERVAL_S as u64 + 60);
/// Between two pings of one lamp.
const PING_RETRY_DELAY: Duration = Duration::from_secs(3);
/// Between two lamps.
const COOL_DOWN: Duration = Duration::from_secs(2);
/// A lamp that keeps failing is checked less often: its timeout is ×1.5 after a failed
/// cycle, then doubles, up to ×12.
const FIRST_BACKOFF_FACTOR: f64 = 1.5;
const MAX_BACKOFF_FACTOR: u32 = 12;
/// Budget of one restore sweep. Past it the sweep pauses until the next change; it does not
/// tear the pipeline down (after a power cut many lamps come back at once, and a healthy but
/// busy link may need longer).
const RESTORE_SWEEP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Availability {
    state: AvailabilityState,
    /// Lamps due for a ping, the first being pinged.
    queue: Vec<AvailabilityTarget>,
}

#[derive(Debug, Clone)]
struct AvailabilityTarget {
    node_id: u16,
    endpoint: u8,
    was_reachable: bool,
    eui64: String,
}

#[derive(Debug, Default)]
enum AvailabilityState {
    /// No ping out: the next step picks the next lamp due.
    #[default]
    Idle,
    WaitingForResponse(Ping),
    CoolDown {
        resume_at: Instant,
    },
}

#[derive(Debug)]
struct Ping {
    target: AvailabilityTarget,
    attempt: u32,
    max_attempts: u32,
    /// Heard after this, the lamp answered.
    before_ping: Instant,
    check_at: Instant,
}

/// One step of the check; a lamp that changed state gets its desired light back and the
/// devices are shared.
pub async fn check_availability(context: &mut EzspContext) {
    // the only blocking work is one read request, bounded like any command
    match timeout(EZSP_COMMAND_TIMEOUT, tick_device_availability(context)).await {
        Ok(true) => {}
        Ok(false) => return,
        Err(_) => {
            warn!("an availability step exceeded the command timeout — it goes on next chore");
            return;
        }
    }
    if timeout(RESTORE_SWEEP_TIMEOUT, restore_desired_state(context))
        .await
        .is_err()
    {
        warn!("desired-state restore sweep exceeded its budget — will resume next tick");
    }
    context.publish();
}

/// How long `device` may stay silent before it is pinged: its heartbeat (a lamp that
/// reports sends one at least every [`REPORT_MAX_INTERVAL_S`]), then ×1.5, ×3, ×6, ×12 at
/// most after failed cycles.
fn silence_allowed(device: &DiscoveredDevice) -> Duration {
    let heartbeat = if device.reporting == Reporting::Configured {
        REPORTED_TIMEOUT
    } else {
        AVAILABILITY_TIMEOUT
    };
    if device.failed_ping_cycles == 0 {
        return heartbeat;
    }
    capped_backoff(
        heartbeat.mul_f64(FIRST_BACKOFF_FACTOR),
        device.failed_ping_cycles,
        heartbeat * MAX_BACKOFF_FACTOR,
    )
}

/// Idle → ping the next lamp due → wait for its answer (retrying once for a lamp that was
/// reachable) → cool down → idle. Says whether a lamp changed state.
async fn tick_device_availability(context: &mut EzspContext) -> bool {
    // answers that arrived update `last_seen` first
    if let Err(error) = drain_callbacks(context).await {
        warn!(%error, "callback during the availability check");
    }
    let now = Instant::now();
    match std::mem::take(&mut context.availability.state) {
        AvailabilityState::Idle => start_next_ping(context, now).await,
        AvailabilityState::WaitingForResponse(ping) => check_ping(context, ping, now).await,
        AvailabilityState::CoolDown { resume_at } => {
            if now < resume_at {
                context.availability.state = AvailabilityState::CoolDown { resume_at };
            }
            false
        }
    }
}

async fn start_next_ping(context: &mut EzspContext, now: Instant) -> bool {
    if context.availability.queue.is_empty() {
        let due = context
            .joined_devices
            .iter()
            .filter_map(|device| due_for_ping(device, now))
            .collect();
        context.availability.queue = due;
    }
    let Some(target) = context.availability.queue.first().cloned() else {
        return false;
    };
    let max_attempts = if target.was_reachable { 2 } else { 1 };
    debug!(node_id = %Hex(target.node_id), eui64 = %target.eui64, was_reachable = target.was_reachable,
        attempts = max_attempts, "availability ping started");
    send_ping(context, target, 1, max_attempts).await
}

/// A lamp (interviewed, with On/Off) silent past its timeout ([`silence_allowed`]).
fn due_for_ping(device: &DiscoveredDevice, now: Instant) -> Option<AvailabilityTarget> {
    let endpoint = device.info.endpoint?;
    let lamp = device.interview_completed
        && device.info.has_input(ON_OFF_CLUSTER_ID)
        && device.info.device_type != ZigbeeDeviceType::Remote;
    let limit = silence_allowed(device);
    let silent = device
        .last_seen
        .is_none_or(|seen| now.duration_since(seen) > limit);
    (lamp && silent).then(|| AvailabilityTarget {
        node_id: device.node_id,
        endpoint,
        was_reachable: device.reachable,
        eui64: device.info.eui64.clone(),
    })
}

async fn send_ping(
    context: &mut EzspContext,
    target: AvailabilityTarget,
    attempt: u32,
    max_attempts: u32,
) -> bool {
    let before_ping = Instant::now();
    match context
        .send_read_attributes(
            target.node_id,
            target.endpoint,
            ON_OFF_CLUSTER_ID,
            &[0x0000],
        )
        .await
    {
        Ok(()) => {
            debug!(
                node_id = %Hex(target.node_id),
                attempt, "availability ping sent"
            );
            let check_at = Instant::now() + PING_RETRY_DELAY;
            context.availability.state = AvailabilityState::WaitingForResponse(Ping {
                target,
                attempt,
                max_attempts,
                before_ping,
                check_at,
            });
            false
        }
        Err(error) => {
            warn!(node_id = %Hex(target.node_id), attempt, %error, "availability ping send failed — skipping device");
            give_up(context, &target, "after an EZSP send failure")
        }
    }
}

async fn check_ping(context: &mut EzspContext, ping: Ping, now: Instant) -> bool {
    if now < ping.check_at {
        context.availability.state = AvailabilityState::WaitingForResponse(ping);
        return false;
    }
    let node_id = ping.target.node_id;
    let answered = context
        .device(node_id)
        .and_then(|device| device.last_seen)
        .is_some_and(|seen| seen > ping.before_ping);
    if answered {
        debug!(
            node_id = %Hex(node_id),
            attempt = ping.attempt,
            "availability ping got response — device is online"
        );
        done_with(context, node_id);
        return true;
    }
    if ping.attempt < ping.max_attempts {
        // unreachable at once for quick feedback; the retry confirms it before the backoff grows
        let changed = mark_lost(context, &ping.target, "after first failed ping (retrying)");
        return send_ping(context, ping.target, ping.attempt + 1, ping.max_attempts).await
            || changed;
    }
    give_up(context, &ping.target, "after failed availability pings")
}

/// Unreachable, one more failed cycle (its next check comes later), on to the next lamp.
fn give_up(context: &mut EzspContext, target: &AvailabilityTarget, why: &str) -> bool {
    let changed = mark_lost(context, target, why);
    if let Some(device) = context.device_mut(target.node_id) {
        device.failed_ping_cycles += 1;
    }
    done_with(context, target.node_id);
    changed
}

/// Says whether that is news (it was reachable).
fn mark_lost(context: &mut EzspContext, target: &AvailabilityTarget, why: &str) -> bool {
    let lost = context
        .device_mut(target.node_id)
        .is_some_and(DiscoveredDevice::lost);
    if lost {
        info!(node_id = %Hex(target.node_id), eui64 = %target.eui64, why, "device marked unreachable");
    }
    lost
}

/// Off the queue; a pause before the next lamp, if there is one.
fn done_with(context: &mut EzspContext, node_id: u16) {
    let availability = &mut context.availability;
    availability
        .queue
        .retain(|target| target.node_id != node_id);
    availability.state = if availability.queue.is_empty() {
        AvailabilityState::Idle
    } else {
        AvailabilityState::CoolDown {
            resume_at: Instant::now() + COOL_DOWN,
        }
    };
}

/// Sends their desired brightness, temperature and colour to the lamps back in reach (a
/// lamp switched back on at the wall starts at its factory defaults). Its on/off is not
/// touched: a lamp powered back on is on.
async fn restore_desired_state(context: &mut EzspContext) {
    let targets = context
        .joined_devices
        .iter()
        .filter(|device| {
            device.reachable && !device.desired.applied && device.info.endpoint.is_some()
        })
        .filter(|device| !device.desired.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    for target in targets {
        // a failure leaves it unapplied: tried again on the next sweep
        if !restore(context, &target).await {
            continue;
        }
        if let Some(device) = context.device_mut(target.node_id) {
            device.desired.applied = true;
            info!(node_id = %Hex(target.node_id), eui64 = %device.info.eui64, "desired state fully restored after reconnect");
        }
        // answers update the device promptly
        if let Err(error) = drain_callbacks(context).await {
            warn!(%error, "callback during the desired-state restore");
        }
    }
}

/// Says whether every part went through.
async fn restore(context: &mut EzspContext, target: &DiscoveredDevice) -> bool {
    let (node_id, info, desired) = (target.node_id, &target.info, &target.desired);
    let Some(endpoint) = info.endpoint else {
        return false;
    };
    // never a brightness of 0: a lamp switched off (dimmer, wall) and switched back on must
    // not go off again
    if let Some(brightness) = desired
        .brightness
        .filter(|level| info.supports_brightness && target.light.is_on && *level > 0)
    {
        let sent = context.send_zcl(
            node_id,
            endpoint,
            LEVEL_CONTROL_CLUSTER_ID,
            "unicast restored brightness",
            |sequence| build_brightness_command_payload(brightness, sequence),
        );
        if !restored(sent.await, node_id, "brightness") {
            return false;
        }
        if let Some(device) = context.device_mut(node_id) {
            device.light.brightness = brightness;
            device.light.is_on = true;
        }
    }
    if let Some(temperature) = desired.temperature.filter(|_| info.supports_temperature) {
        let sent = context.send_zcl(
            node_id,
            endpoint,
            COLOR_CONTROL_CLUSTER_ID,
            "unicast restored colour temperature",
            |sequence| build_color_temperature_command_payload(temperature, sequence),
        );
        if !restored(sent.await, node_id, "colour temperature") {
            return false;
        }
        if let Some(device) = context.device_mut(node_id) {
            device.light.temperature = Some(temperature);
        }
    }
    if let Some((x, y)) = desired.color.filter(|_| info.supports_color) {
        let sent = context.send_zcl(
            node_id,
            endpoint,
            COLOR_CONTROL_CLUSTER_ID,
            "unicast restored colour xy",
            |sequence| build_color_xy_command_payload(x, y, sequence),
        );
        if !restored(sent.await, node_id, "colour XY") {
            return false;
        }
        if let Some(device) = context.device_mut(node_id) {
            device.light.color.color_x = Some(x);
            device.light.color.color_y = Some(y);
        }
    }
    true
}

fn restored(sent: Result<(), super::error::DriverError>, node_id: u16, what: &str) -> bool {
    match sent {
        Ok(()) => {
            info!(
                node_id = %Hex(node_id),
                what, "restored desired state after reconnect"
            );
            true
        }
        Err(error) => {
            warn!(node_id = %Hex(node_id), what, %error, "failed to restore desired state");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_lamp_is_checked_less_often() {
        let mut lamp = DiscoveredDevice::test_lamp();
        let allowed = |lamp: &DiscoveredDevice, cycles| {
            let mut lamp = lamp.clone();
            lamp.failed_ping_cycles = cycles;
            silence_allowed(&lamp).as_secs()
        };
        assert_eq!(
            [0, 1, 2, 3, 4, 400].map(|cycles| allowed(&lamp, cycles)),
            [60, 90, 180, 360, 720, 720],
            "×1.5, ×3, ×6, ×12 at most, no overflow"
        );
        // a lamp that reports says it lives every five minutes: pinged only past that
        lamp.reporting = Reporting::Configured;
        assert_eq!(allowed(&lamp, 0), 360);
        lamp.reporting = Reporting::Asked;
        assert_eq!(allowed(&lamp, 0), 60, "until it says yes, pinged as before");
    }

    #[test]
    fn a_reporting_lamp_heard_minutes_ago_is_not_pinged() {
        let now = Instant::now();
        let mut lamp = DiscoveredDevice::test_lamp();
        let Some(two_minutes_ago) = now.checked_sub(Duration::from_secs(120)) else {
            return;
        };
        lamp.last_seen = Some(two_minutes_ago);
        assert!(
            due_for_ping(&lamp, now).is_some(),
            "silent 2 min, no reports"
        );
        lamp.reporting = Reporting::Configured;
        assert!(due_for_ping(&lamp, now).is_none(), "its next report is due");
    }

    #[test]
    fn only_silent_interviewed_lamps_are_pinged() {
        let now = Instant::now();
        let mut lamp = DiscoveredDevice::test_lamp();
        lamp.last_seen = None;
        assert!(due_for_ping(&lamp, now)
            .is_some_and(|target| target.endpoint == 11 && target.was_reachable));

        lamp.last_seen = Some(now);
        assert!(due_for_ping(&lamp, now).is_none(), "heard just now");

        let mut remote = DiscoveredDevice::test_lamp();
        remote.info.device_type = ZigbeeDeviceType::Remote;
        assert!(
            due_for_ping(&remote, now).is_none(),
            "a sleepy remote is never pinged"
        );

        let mut uninterviewed = DiscoveredDevice::test_lamp();
        uninterviewed.info.endpoint = None;
        assert!(due_for_ping(&uninterviewed, now).is_none());
    }
}
