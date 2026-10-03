//! One live EZSP pipeline (serial → ASH → EZSP) and what the driver knows while it lives:
//! the devices, sequence numbers, the state of the background chores. Rebuilt whole when
//! the link fails; only the devices are carried over.

use std::{collections::HashMap, sync::Arc};

use ezsp::{
    Callback, Connection as EzspConnection, Messaging,
    ember::{
        Eui64, NodeId,
        aps::{Frame as EzspApsFrame, Options as EzspApsOptions},
        message::Destination,
    },
};
use tokio::{sync::mpsc, task::JoinHandle, time::Instant};

use super::{
    availability::Availability,
    device::DiscoveredDevice,
    driver::{ErrorCount, Shared},
    error::DriverError,
    remotes::RemotePresses,
    touchlink::TouchlinkScan,
    zcl::{DEFAULT_SOURCE_ENDPOINT, HOME_AUTOMATION_PROFILE_ID, ZDO_PROFILE_ID, build_read_attributes_payload},
};

/// Join handles for the four pipeline tasks (ASH transmitter/receiver and EZSP
/// transmitter/receiver actors). All spawned by us, so teardown is a bounded abort + join.
/// Dropped without a teardown (the driver panicked), they are aborted all the same: a
/// detached actor would keep the serial port open.
pub struct PipelineTasks(pub [JoinHandle<()>; 4]);

impl PipelineTasks {
    /// All four tasks must be running for the pipeline to be usable; any finished task
    /// (serial unplug, channel closure, panic) means the whole stack must be rebuilt.
    pub fn is_alive(&self) -> bool {
        self.0.iter().all(|handle| !handle.is_finished())
    }

    pub async fn shutdown(mut self) {
        for handle in &mut self.0 {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for PipelineTasks {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
    }
}

pub struct EzspContext {
    pub connection: EzspConnection,
    pub tasks: PipelineTasks,
    pub callbacks_rx: mpsc::Receiver<Callback>,
    /// What the driver shares with the manager (state, devices).
    pub shared: Arc<Shared>,
    pub joined_devices: Vec<DiscoveredDevice>,
    next_global_sequence: u8,
    next_device_sequence: HashMap<u16, u8>,
    /// Every successful EZSP round-trip and every callback: the watchdog's proof of life.
    pub last_activity: Instant,
    /// The coordinator's own EUI64, fetched at startup: a ZDO Bind_req names it.
    pub coordinator_eui64: Option<Eui64>,
    pub remote_presses: RemotePresses,
    pub touchlink: TouchlinkScan,
    pub errors: ErrorCount,
    pub availability: Availability,
}

impl EzspContext {
    pub fn new(
        connection: EzspConnection,
        callbacks_rx: mpsc::Receiver<Callback>,
        tasks: PipelineTasks,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            connection,
            tasks,
            callbacks_rx,
            shared,
            joined_devices: Vec::new(),
            next_global_sequence: 1,
            next_device_sequence: HashMap::new(),
            last_activity: Instant::now(),
            coordinator_eui64: None,
            remote_presses: RemotePresses::default(),
            touchlink: TouchlinkScan::default(),
            errors: ErrorCount::default(),
            availability: Availability::default(),
        }
    }

    pub fn device(&self, node_id: u16) -> Option<&DiscoveredDevice> {
        self.joined_devices.iter().find(|device| device.node_id == node_id)
    }

    pub fn device_mut(&mut self, node_id: u16) -> Option<&mut DiscoveredDevice> {
        self.joined_devices.iter_mut().find(|device| device.node_id == node_id)
    }

    /// The devices, shared with the manager (only a change is announced).
    pub fn publish(&self) {
        self.shared.publish(&self.joined_devices);
    }

    /// A device left: forget it and its sequence numbers.
    pub fn forget(&mut self, node_id: u16, eui64: &str) {
        // by EUI64: a node id can be given to another device after a rejoin
        self.joined_devices.retain(|device| device.info.eui64 != eui64);
        self.next_device_sequence.remove(&node_id);
    }

    pub fn next_device_sequence(&mut self, node_id: u16) -> u8 {
        next_sequence_for_device(&mut self.next_device_sequence, &mut self.next_global_sequence, node_id)
    }

    /// A ZCL frame (home-automation profile, from our endpoint) to `endpoint` of `node_id`;
    /// `build` gets the device's next sequence number.
    pub async fn send_zcl(
        &mut self,
        node_id: u16,
        endpoint: u8,
        cluster_id: u16,
        what: &'static str,
        build: impl FnOnce(u8) -> Vec<u8>,
    ) -> Result<(), DriverError> {
        let sequence = self.next_device_sequence(node_id);
        let frame = aps_frame(HOME_AUTOMATION_PROFILE_ID, cluster_id, DEFAULT_SOURCE_ENDPOINT, endpoint);
        self.send_unicast(node_id, frame, build(sequence), what).await
    }

    /// A ZDO request (profile and endpoints 0) to `node_id`; `build` gets the sequence number.
    pub async fn send_zdo(
        &mut self,
        node_id: u16,
        cluster_id: u16,
        what: &'static str,
        build: impl FnOnce(u8) -> Vec<u8>,
    ) -> Result<(), DriverError> {
        let sequence = self.next_device_sequence(node_id);
        let frame = aps_frame(ZDO_PROFILE_ID, cluster_id, 0, 0);
        self.send_unicast(node_id, frame, build(sequence), what).await
    }

    pub async fn send_read_attributes(
        &mut self,
        node_id: u16,
        endpoint: u8,
        cluster_id: u16,
        attributes: &[u16],
    ) -> Result<(), DriverError> {
        self.send_zcl(node_id, endpoint, cluster_id, "ZCL read attributes", |sequence| {
            build_read_attributes_payload(attributes, sequence)
        })
        .await
    }

    /// One unicast round-trip with the NCP; its success proves the link alive to the watchdog.
    async fn send_unicast(
        &mut self,
        node_id: u16,
        frame: EzspApsFrame,
        payload: Vec<u8>,
        what: &'static str,
    ) -> Result<(), DriverError> {
        self.connection
            .send_unicast(Destination::Direct(NodeId::from(node_id)), frame, 0, payload.into_iter().collect())
            .await
            .map_err(DriverError::ezsp(what))?;
        self.last_activity = Instant::now();
        Ok(())
    }
}

/// Every unicast is retried, with route discovery.
fn aps_frame(profile_id: u16, cluster_id: u16, source_endpoint: u8, destination_endpoint: u8) -> EzspApsFrame {
    EzspApsFrame::new(
        profile_id,
        cluster_id,
        source_endpoint,
        destination_endpoint,
        EzspApsOptions::RETRY | EzspApsOptions::ENABLE_ROUTE_DISCOVERY,
        0,
        0,
    )
}

/// Each device has its own sequence, started from a global one.
fn next_sequence_for_device(per_device: &mut HashMap<u16, u8>, next_global: &mut u8, node_id: u16) -> u8 {
    let current = per_device.entry(node_id).or_insert_with(|| {
        let value = *next_global;
        *next_global = next_global.wrapping_add(1);
        value
    });
    let sequence = *current;
    *current = current.wrapping_add(1);
    sequence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_sequences_are_independent() {
        let mut per_device = HashMap::new();
        let mut next_global = 1;
        assert_eq!(next_sequence_for_device(&mut per_device, &mut next_global, 0x8a4c), 1);
        assert_eq!(next_sequence_for_device(&mut per_device, &mut next_global, 0x8a4c), 2);
        assert_eq!(next_sequence_for_device(&mut per_device, &mut next_global, 0x6cce), 2);
        assert_eq!(next_sequence_for_device(&mut per_device, &mut next_global, 0x8a4c), 3);
        assert_eq!(next_sequence_for_device(&mut per_device, &mut next_global, 0x6cce), 3);
    }
}
