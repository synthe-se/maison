//! The driver: one task, on its own thread, owns the dongle (serial → ASH → EZSP) and serves the commands queued
//! to it, and the callbacks as they come. It never dies: a panic is a restart after a pause,
//! a failing link a rebuild, a missing dongle a retry with backoff. [`Driver`] is the handle
//! the manager holds; it shares the devices through a watch channel, so the manager only
//! syncs what changed.

use std::{
    future::Future,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use tokio::{
    runtime,
    sync::{mpsc, oneshot, watch, RwLock},
    time::{interval, timeout, Instant, MissedTickBehavior},
};
use tracing::{debug, error, info, warn};

use super::{
    availability::check_availability,
    callbacks::on_callback,
    commands::{discover_devices, handle_command, DriverCommand},
    config::ZigbeeConfig,
    context::EzspContext,
    device::{DiscoveredDevice, ZigbeeDevice, ZigbeeDeviceInfo},
    discovery::retry_pending_interviews,
    error::{DriverError, Link},
    network::{bring_up, check_link, teardown_context, NetworkState},
};
use crate::{
    error::{AppError, Refusal},
    util::{capped_backoff, lock},
};

/// How often the chores run (link health, availability): callbacks and commands are served
/// as they come, never on this tick.
const CHORES_INTERVAL: Duration = Duration::from_secs(1);
/// Uninterviewed devices are asked again every this many chores.
const INTERVIEW_RETRY_CHORES: u32 = 10;
/// Per-EZSP-step timeout. Past it the pipeline is presumed dead and rebuilt: safe ONLY
/// because a desynchronised EZSP channel is never reused.
pub const EZSP_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// Touchlink scans run two 15-second radio passes plus per-device delays.
pub const TOUCHLINK_COMMAND_TIMEOUT: Duration = Duration::from_secs(90);
/// Longest a caller of [`Driver::send`] waits for the reply: past the worst per-command
/// timeout plus queueing, and with [`COMMAND_ENQUEUE_TIMEOUT`] under the 180 s HTTP
/// timeout, so callers see this error, not a gateway timeout.
const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_secs(120);
/// Longest a command may wait to enter the 32-slot queue: full that long, the driver is
/// wedged or saturated; failing fast beats stacking more work behind it.
const COMMAND_ENQUEUE_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(300);
/// Past this many failed attempts in a row the driver reports `Failed` (requests fail fast)
/// but KEEPS retrying: the radio must never need a process restart to come back.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;
/// Longest pause before restarting a driver that panicked.
const MAX_RESTART_DELAY: Duration = Duration::from_secs(60);
/// Commands the dongle failed in a row before the link is presumed broken.
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

const NO_SERIAL_PORT: &str = "No Zigbee serial port is set (ZIGBEE_SERIAL_PORT)";

/// Whether the driver takes commands. `Failed` says why in words fit for the client (the
/// detail, serial path included, is in the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverLifecycle {
    Starting,
    Ready,
    Failed(&'static str),
}

/// What the driver task and the handle share.
pub struct Shared {
    message: RwLock<Option<String>>,
    lifecycle: RwLock<DriverLifecycle>,
    network: RwLock<NetworkState>,
    devices: watch::Sender<Vec<ZigbeeDevice>>,
}

impl Shared {
    fn new(message: Option<String>) -> Self {
        Self {
            message: RwLock::new(message),
            lifecycle: RwLock::new(DriverLifecycle::Starting),
            network: RwLock::new(NetworkState::Unknown),
            devices: watch::Sender::new(Vec::new()),
        }
    }

    /// The status line shown with the lamps (none when all is well).
    pub async fn say(&self, message: Option<String>) {
        *self.message.write().await = message;
    }

    async fn set_lifecycle(&self, lifecycle: DriverLifecycle) {
        *self.lifecycle.write().await = lifecycle;
    }

    pub async fn set_network(&self, state: NetworkState) {
        *self.network.write().await = state;
        self.say(state.problem().map(str::to_string)).await;
    }

    /// The devices, announced to the manager only when they changed.
    pub fn publish(&self, devices: &[DiscoveredDevice]) {
        let snapshot = devices.iter().map(ZigbeeDevice::from).collect::<Vec<_>>();
        self.devices.send_if_modified(|current| {
            let changed = *current != snapshot;
            if changed {
                *current = snapshot;
            }
            changed
        });
    }
}

pub struct DriverRequest {
    pub command: DriverCommand,
    pub reply_tx: oneshot::Sender<Result<(), DriverError>>,
}

/// The manager's handle on the driver: the task starts on first use.
pub struct Driver {
    shared: Arc<Shared>,
    command_tx: mpsc::Sender<DriverRequest>,
    command_rx: StdMutex<Option<mpsc::Receiver<DriverRequest>>>,
    /// Held while the driver runs: it stops when this goes.
    task: StdMutex<Option<oneshot::Sender<()>>>,
    config: ZigbeeConfig,
    known_devices: Vec<(u16, ZigbeeDeviceInfo)>,
}

impl Driver {
    /// The driver for `config`, seeded with the devices kept from earlier runs.
    pub fn new(config: ZigbeeConfig, known_devices: Vec<(u16, ZigbeeDeviceInfo)>) -> Self {
        let message = match &config.serial_port {
            Some(_) => "The Zigbee radio is starting",
            None => NO_SERIAL_PORT,
        };
        let (command_tx, command_rx) = mpsc::channel(32);
        Self {
            shared: Arc::new(Shared::new(Some(message.to_string()))),
            command_tx,
            command_rx: StdMutex::new(Some(command_rx)),
            task: StdMutex::new(None),
            config,
            known_devices,
        }
    }

    pub async fn message(&self) -> Option<String> {
        self.shared.message.read().await.clone()
    }

    /// The devices as the driver last shared them; `changed()` says when they move.
    pub fn subscribe(&self) -> watch::Receiver<Vec<ZigbeeDevice>> {
        self.shared.devices.subscribe()
    }

    /// Starts the driver task, once; it runs the first discovery itself, when its network is
    /// up. Never waits: the save loop and every request call it, and must not queue behind a
    /// dead radio.
    pub fn start(&self) {
        let mut task = lock(&self.task);
        if task.is_some() {
            return;
        }
        let Some(command_rx) = lock(&self.command_rx).take() else {
            return;
        };
        let (config, known, shared) = (
            self.config.clone(),
            self.known_devices.clone(),
            Arc::clone(&self.shared),
        );
        let driver = supervise_driver(Arc::clone(&self.shared), command_rx, move |command_rx| {
            Box::pin(run_driver(
                config.clone(),
                known.clone(),
                Arc::clone(&shared),
                command_rx,
            ))
        });
        match on_own_thread(driver) {
            Ok(stop) => *task = Some(stop),
            Err(error) => error!(%error, "no thread for the Zigbee driver"),
        }
    }

    /// Runs `command` on the radio. Refused, coded, while the radio is not ready
    /// (`not_configured` without a serial port, `device_unreachable` else, the reason in the
    /// log); the driver's own refusals as [`DriverError::into_client`] says them.
    pub async fn send(&self, command: DriverCommand) -> Result<(), AppError> {
        self.start();
        let down = |reason: &'static str| -> AppError {
            if reason == NO_SERIAL_PORT {
                DriverError::NotConfigured(reason).into_client()
            } else {
                DriverError::Unavailable(reason).into_client()
            }
        };
        match *self.shared.lifecycle.read().await {
            DriverLifecycle::Starting => return Err(down("The Zigbee radio is still starting")),
            DriverLifecycle::Failed(reason) => return Err(down(reason)),
            DriverLifecycle::Ready => {}
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        // both steps bounded: a caller never hangs, even on a wedged driver reporting Ready
        match timeout(
            COMMAND_ENQUEUE_TIMEOUT,
            self.command_tx.send(DriverRequest { command, reply_tx }),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(down("The Zigbee radio is not running")),
            Err(_) => {
                warn!("the Zigbee radio is not taking commands (queue full)");
                return Err(Refusal::Busy.into());
            }
        }
        timeout(COMMAND_REPLY_TIMEOUT, reply_rx)
            .await
            .map_err(|_| down("The Zigbee radio did not answer in time"))?
            .map_err(|_| down("The Zigbee radio dropped the command"))?
            .map_err(DriverError::into_client)
    }

    pub fn shutdown(&self) {
        lock(&self.task).take();
    }
}

/// `task` on a thread and a runtime of its own, until the sender goes. The serial port's calls can
/// block in the kernel (a dongle that stopped taking bytes leaves a drain waiting for ever): on the
/// Pi's one core, on the app's runtime, that would be every route of the house.
fn on_own_thread(
    task: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<oneshot::Sender<()>> {
    let (stop, stopped) = oneshot::channel();
    std::thread::Builder::new()
        .name("zigbee".into())
        .spawn(move || {
            let runtime = match runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(error) => {
                    error!(%error, "no runtime for the Zigbee driver");
                    return;
                }
            };
            runtime.block_on(async {
                tokio::select! {
                    () = task => {}
                    _ = stopped => {}
                }
            });
        })?;
    Ok(stop)
}

#[cfg(test)]
impl Driver {
    pub fn test_seed_devices(&self, devices: Vec<ZigbeeDevice>) {
        self.shared.devices.send_replace(devices);
    }

    /// Keeps the driver from ever starting (its queue is taken away): a test drives the
    /// manager's state alone, with no serial port opened behind its back.
    pub fn test_detach(&self) {
        self.command_rx.lock().unwrap().take();
    }

    pub async fn test_set_lifecycle(&self, lifecycle: DriverLifecycle) {
        self.shared.set_lifecycle(lifecycle).await;
    }
}

/// The driver, supervised: it owns the command queue across restarts, so a panic in the
/// driver (a parser bug, an unexpected NCP answer) is a restart after a pause, not a Zigbee
/// dead until the service restarts. While it waits the lifecycle says `Failed` (requests
/// fail fast); a clean return (queue closed, no serial port) ends it.
async fn supervise_driver<F>(
    shared: Arc<Shared>,
    mut command_rx: mpsc::Receiver<DriverRequest>,
    mut run: F,
) where
    F: for<'a> FnMut(&'a mut mpsc::Receiver<DriverRequest>) -> futures::future::BoxFuture<'a, ()>,
{
    use futures::FutureExt;
    const CRASHED: &str = "The Zigbee radio crashed; it restarts";
    let mut crashes: u32 = 0;
    loop {
        let Err(panic) = std::panic::AssertUnwindSafe(run(&mut command_rx))
            .catch_unwind()
            .await
        else {
            return;
        };
        crashes = crashes.saturating_add(1);
        let reason = panic
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        let delay = restart_delay(crashes);
        error!(%reason, crashes, ?delay, "native zigbee driver panicked — restarting");
        shared.set_lifecycle(DriverLifecycle::Failed(CRASHED)).await;
        shared.say(Some(CRASHED.to_string())).await;
        // requests queued meanwhile would wait for the reply timeout: answer them now
        drain_pending_requests(&mut command_rx, || {
            DriverError::Unavailable("The Zigbee radio is restarting")
        });
        tokio::time::sleep(delay).await;
        shared.set_lifecycle(DriverLifecycle::Starting).await;
    }
}

/// 1 s, 2 s, 4 s… up to [`MAX_RESTART_DELAY`]: a driver that panics at once every time
/// must not spin the Pi's single core.
fn restart_delay(crashes: u32) -> Duration {
    capped_backoff(Duration::from_secs(1), crashes, MAX_RESTART_DELAY)
}

/// Opens the pipeline, serves commands until the link fails, rebuilds it, forever; returns
/// when the queue closes or no serial port is set.
async fn run_driver(
    config: ZigbeeConfig,
    known: Vec<(u16, ZigbeeDeviceInfo)>,
    shared: Arc<Shared>,
    command_rx: &mut mpsc::Receiver<DriverRequest>,
) {
    let Some(serial_port) = config.serial_port.clone() else {
        warn!("no Zigbee serial port is set (ZIGBEE_SERIAL_PORT): the radio stays off");
        shared.say(Some(NO_SERIAL_PORT.to_string())).await;
        shared
            .set_lifecycle(DriverLifecycle::Failed(NO_SERIAL_PORT))
            .await;
        drain_pending_requests(command_rx, || DriverError::NotConfigured(NO_SERIAL_PORT));
        return;
    };
    // the kept devices first; after a rebuild, the old pipeline's (desired state included)
    let mut devices = known
        .into_iter()
        .map(|(node_id, info)| DiscoveredDevice::known(node_id, info))
        .collect::<Vec<_>>();
    let mut attempts: u32 = 0;
    // the known devices are probed once, the first time the network is up (a rebuild keeps
    // what the old pipeline knew of them)
    let mut discovered = false;
    loop {
        let mut context = match bring_up(&config, &serial_port, &shared, &devices).await {
            Ok(context) => context,
            Err(error) => {
                attempts += 1;
                warn!(%serial_port, attempt = attempts, %error, "Zigbee radio bring-up failed — retrying");
                backoff_before_retry(&shared, attempts).await;
                continue;
            }
        };
        attempts = 0;
        shared.set_lifecycle(DriverLifecycle::Ready).await;

        let Some(reason) = event_loop(&mut context, command_rx, &mut discovered).await else {
            teardown_context(context).await;
            return;
        };
        devices = context.joined_devices.clone();
        attempts += 1;
        warn!(%serial_port, %reason, attempt = attempts, "EZSP pipeline unhealthy — tearing down and reconnecting");
        teardown_context(context).await;
        backoff_before_retry(&shared, attempts).await;
    }
}

/// Serves commands and callbacks as they come, and runs the chores every
/// [`CHORES_INTERVAL`], until the queue closes (`None`) or the link fails (why).
async fn event_loop(
    context: &mut EzspContext,
    command_rx: &mut mpsc::Receiver<DriverRequest>,
    discovered: &mut bool,
) -> Option<String> {
    let mut chores = interval(CHORES_INTERVAL);
    chores.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut chore_count: u32 = 0;
    loop {
        let outcome = tokio::select! {
            request = command_rx.recv() => match request {
                Some(request) => serve(context, request).await,
                None => return None,
            },
            Some(callback) = context.callbacks_rx.recv() => on_callback(context, callback)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            _ = chores.tick() => {
                chore_count = chore_count.wrapping_add(1);
                on_chores(context, chore_count, discovered).await
            }
        };
        // an exchange made outside a command (an interview, a ping) may say the link is gone
        if let Some(reason) = outcome.err().or_else(|| context.rebuild.take()) {
            return Some(reason);
        }
    }
}

/// Runs one command and answers it; says why the pipeline must be rebuilt, if it must.
async fn serve(context: &mut EzspContext, request: DriverRequest) -> Result<(), String> {
    // the caller gave up (HTTP timeout, dropped connection): running it now would be a
    // surprise second execution after the user retried
    if request.reply_tx.is_closed() {
        debug!("skipping queued zigbee command whose caller gave up");
        return Ok(());
    }
    let limit = request.command.timeout();
    let Ok(result) = timeout(limit, handle_command(context, request.command)).await else {
        error!("EZSP command timed out after {limit:?} — triggering reconnect");
        let _ = request
            .reply_tx
            .send(Err(DriverError::Timeout("EZSP command")));
        return Err(format!("EZSP command timed out after {limit:?}"));
    };
    match &result {
        Ok(()) => {
            context.last_activity = Instant::now();
            context.publish();
        }
        Err(error) => {
            warn!(%error, consecutive_errors = context.errors.count(), "native zigbee command failed")
        }
    }
    let rebuild = context.errors.record(&result);
    if request.reply_tx.send(result).is_err() {
        warn!("native zigbee command response receiver dropped");
    }
    rebuild.map_or(Ok(()), Err)
}

/// The chores: link health, the first discovery (once the network is up), interview
/// retries, availability.
async fn on_chores(
    context: &mut EzspContext,
    chore_count: u32,
    discovered: &mut bool,
) -> Result<(), String> {
    check_link(context).await?;
    if !*discovered && *context.shared.network.read().await == NetworkState::Joined {
        // a hung unicast here would block the whole loop
        timeout(EZSP_COMMAND_TIMEOUT, discover_devices(context))
            .await
            .map_err(|_| "first discovery timed out".to_string())?;
        *discovered = true;
        context.publish();
    }
    if chore_count.is_multiple_of(INTERVIEW_RETRY_CHORES) {
        timeout(EZSP_COMMAND_TIMEOUT, retry_pending_interviews(context))
            .await
            .map_err(|_| "interview retry timed out".to_string())?;
        context.publish();
    }
    check_availability(context).await;
    Ok(())
}

/// Commands the dongle failed in a row, without a success between.
#[derive(Debug, Default)]
pub struct ErrorCount(u32);

impl ErrorCount {
    pub fn count(&self) -> u32 {
        self.0
    }

    /// Counts `result`; says why the pipeline must be rebuilt, if it must. A request refused
    /// before reaching the dongle (unknown lamp, missing cluster) counts for nothing: five
    /// of them must not take every lamp offline.
    pub fn record(&mut self, result: &Result<(), DriverError>) -> Option<String> {
        let error = match result {
            Ok(()) => {
                self.0 = 0;
                return None;
            }
            Err(error) => error,
        };
        match error.link() {
            Link::Fine => None,
            Link::Broken => Some(format!("EZSP transport failure: {error}")),
            Link::Suspect => {
                self.0 += 1;
                (self.0 >= MAX_CONSECUTIVE_ERRORS)
                    .then(|| format!("{} consecutive EZSP command failures", self.0))
            }
        }
    }
}

/// Says the driver is down and waits before the next attempt: `Starting` with a short delay
/// at first, `Failed` past [`MAX_RECONNECT_ATTEMPTS`] (requests fail fast), with a capped
/// exponential backoff. It never gives up.
async fn backoff_before_retry(shared: &Shared, attempts: u32) {
    let delay = capped_backoff(RECONNECT_DELAY, attempts, MAX_RECONNECT_DELAY);
    let lifecycle = if attempts > MAX_RECONNECT_ATTEMPTS {
        DriverLifecycle::Failed("The Zigbee radio is unreachable; it keeps trying")
    } else {
        DriverLifecycle::Starting
    };
    shared.set_lifecycle(lifecycle).await;
    shared
        .say(Some(format!(
            "Reconnecting the Zigbee radio (attempt {attempts}, next try in {delay:?})"
        )))
        .await;
    info!(attempts, ?delay, "zigbee radio reconnect scheduled");
    tokio::time::sleep(delay).await;
}

/// Answers the requests already queued with `refusal` (`try_recv`: `recv` would wait forever
/// while the handle lives). A caller that gave up meanwhile hears nothing, and needs not.
fn drain_pending_requests(
    command_rx: &mut mpsc::Receiver<DriverRequest>,
    refusal: impl Fn() -> DriverError,
) {
    while let Ok(request) = command_rx.try_recv() {
        if request.reply_tx.send(Err(refusal())).is_err() {
            debug!("a queued zigbee request's caller had gone");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn restarts_back_off_up_to_a_minute() {
        assert_eq!(restart_delay(1), Duration::from_secs(1));
        assert_eq!(restart_delay(2), Duration::from_secs(2));
        assert_eq!(restart_delay(5), Duration::from_secs(16));
        assert_eq!(restart_delay(50), Duration::from_secs(60));
    }

    #[test]
    fn only_link_failures_count_toward_a_rebuild() {
        let refused = || Err(DriverError::UnknownDevice("nope".into()));
        let failed = || {
            Err(DriverError::ezsp("send unicast")(
                ezsp::Error::TransactionQueueFull,
            ))
        };
        let mut errors = ErrorCount::default();
        for _ in 0..20 {
            assert_eq!(
                errors.record(&refused()),
                None,
                "an unknown lamp is no reason to rebuild"
            );
        }
        assert_eq!(errors.count(), 0);
        for _ in 0..4 {
            assert_eq!(errors.record(&failed()), None);
        }
        assert_eq!(
            errors.record(&refused()),
            None,
            "a refusal neither counts nor resets"
        );
        assert!(errors
            .record(&failed())
            .is_some_and(|reason| reason.starts_with("5 consecutive")));

        let mut errors = ErrorCount::default();
        errors.record(&failed());
        errors.record(&Ok(()));
        assert_eq!(errors.count(), 0, "a success resets");
        let corrupt = Err(DriverError::ezsp("send unicast")(ezsp::Error::Decode(
            ezsp::Decode::TooFewBytes,
        )));
        assert!(
            errors.record(&corrupt).is_some(),
            "a broken frame rebuilds at once"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_serial_call_stuck_in_the_kernel_stops_only_the_driver() {
        let (held, stuck) = std::sync::mpsc::channel::<()>();
        let (entered_tx, entered) = oneshot::channel();
        let _stop = on_own_thread(async move {
            let _ = entered_tx.send(());
            // not a yield: as a drain would, it holds the thread
            let _ = stuck.recv();
        })
        .unwrap();
        timeout(Duration::from_secs(5), entered)
            .await
            .unwrap()
            .unwrap();
        // this test has one thread, as the Pi has one worker: it still runs
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(held);
    }

    #[tokio::test]
    async fn the_driver_stops_when_told() {
        struct Gone(Option<oneshot::Sender<()>>);
        impl Drop for Gone {
            fn drop(&mut self) {
                let _ = self.0.take().map(|gone| gone.send(()));
            }
        }
        let (gone_tx, gone) = oneshot::channel();
        let guard = Gone(Some(gone_tx));
        let stop = on_own_thread(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        })
        .unwrap();
        drop(stop);
        timeout(Duration::from_secs(5), gone)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_driver_is_failed_then_restarted() {
        let shared = Arc::new(Shared::new(None));
        let (command_tx, command_rx) = mpsc::channel::<DriverRequest>(4);
        let runs = Arc::new(AtomicU32::new(0));
        let supervisor = tokio::spawn(supervise_driver(shared.clone(), command_rx, {
            let (runs, shared) = (runs.clone(), shared.clone());
            move |command_rx| {
                let (runs, shared) = (runs.clone(), shared.clone());
                Box::pin(async move {
                    if runs.fetch_add(1, Ordering::SeqCst) == 0 {
                        panic!("parser bug");
                    }
                    shared.set_lifecycle(DriverLifecycle::Ready).await;
                    while let Some(request) = command_rx.recv().await {
                        let _ = request.reply_tx.send(Ok(()));
                    }
                })
            }
        }));

        let mut saw_failed = false;
        for _ in 0..300 {
            match *shared.lifecycle.read().await {
                DriverLifecycle::Failed(reason) => {
                    assert!(
                        !reason.contains("parser bug"),
                        "the panic text stays in the log: {reason}"
                    );
                    saw_failed = true;
                }
                DriverLifecycle::Ready => break,
                DriverLifecycle::Starting => {}
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(saw_failed, "the crash is said while the driver waits");
        assert_eq!(
            *shared.lifecycle.read().await,
            DriverLifecycle::Ready,
            "the driver came back"
        );

        // the queue survived the crash: the restarted driver answers
        let (reply_tx, reply_rx) = oneshot::channel();
        command_tx
            .send(DriverRequest {
                command: DriverCommand::PermitJoin { seconds: 0 },
                reply_tx,
            })
            .await
            .unwrap();
        assert!(reply_rx.await.unwrap().is_ok());

        drop(command_tx);
        supervisor
            .await
            .expect("a clean return ends the supervisor");
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn the_client_hears_no_serial_path_while_the_radio_is_down() {
        let config = ZigbeeConfig {
            serial_port: Some("/dev/null-maison-test".to_string()),
            ..ZigbeeConfig::default()
        };
        let driver = Driver::new(config, Vec::new());
        driver
            .shared
            .set_lifecycle(DriverLifecycle::Failed(
                "The Zigbee radio is unreachable; it keeps trying",
            ))
            .await;
        driver.test_detach();
        let error = driver
            .send(DriverCommand::PermitJoin { seconds: 0 })
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("/dev/"), "{error}");
        assert!(!driver.message().await.unwrap_or_default().contains("/dev/"));
    }

    /// Every drain (event loop, Touchlink, availability, bring-up) hands its callbacks to
    /// `callbacks::on_callback`, which applies a network event here.
    #[tokio::test]
    async fn a_network_event_sets_the_state_the_driver_waits_on() {
        let shared = Shared::new(None);
        shared.set_network(NetworkState::Down).await;
        assert_eq!(*shared.network.read().await, NetworkState::Down);
        assert_eq!(
            shared.message.read().await.as_deref(),
            Some("The Zigbee network is down")
        );
        shared.set_network(NetworkState::Joined).await;
        assert_eq!(*shared.network.read().await, NetworkState::Joined);
        assert_eq!(
            *shared.message.read().await,
            None,
            "a healthy network says nothing"
        );
    }

    #[test]
    fn a_change_is_announced_once() {
        let shared = Shared::new(None);
        let mut seen = shared.devices.subscribe();
        let devices = vec![DiscoveredDevice::test_lamp()];
        shared.publish(&devices);
        assert!(seen.has_changed().unwrap());
        seen.borrow_and_update();
        shared.publish(&devices);
        assert!(
            !seen.has_changed().unwrap(),
            "the same devices again are no news"
        );
        let mut dimmed = devices.clone();
        dimmed[0].light.brightness = 40;
        shared.publish(&dimmed);
        assert!(seen.has_changed().unwrap());
    }
}
