//! What changed, said to every open page and wall screen (`GET /api/events`, server-sent
//! events; the names are `maison_api::events`): each family's own watch (the Zigbee driver's
//! lamps, the shutters' subscriptions, Tempo's answer, each cat device's pushes, the plugs'
//! reader, the last climate order, the remote's keys, the box's power) is followed by one task,
//! which says the family's name on one broadcast. The data is a name, never the value: a client
//! reads the value again through its usual route (one answer, one serialisation).

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::Stream;
use maison_api::events as names;
use tokio::sync::{broadcast, watch};

use crate::{supervised, AppState};

/// How many changes wait for a slow client before it is told « everything ».
const BACKLOG: usize = 64;

/// The broadcast every change goes on.
#[derive(Clone)]
pub struct Events {
    tx: broadcast::Sender<String>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            tx: broadcast::channel(BACKLOG).0,
        }
    }
}

impl Events {
    /// Says `name` changed (nobody listening is no error).
    pub fn said(&self, name: impl Into<String>) {
        let _ = self.tx.send(name.into());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Follows every family's watch for the server's life: each subscribed at once (a change
    /// right after the start is not missed), the cat devices once their list is read.
    pub(crate) fn follow_all(&self, state: &AppState) {
        self.follow(
            "events: zigbee",
            state.zigbee.subscribe(),
            names::ZIGBEE_LAMPS,
        );
        self.follow(
            "events: shutters",
            state.matter.subscribe(),
            names::SHUTTERS,
        );
        self.follow("events: tempo", state.tempo.subscribe(), names::TEMPO);
        self.follow("events: meross", state.meross.subscribe(), names::MEROSS);
        self.follow(
            "events: climate",
            state.broadlink.subscribe_climate(),
            names::CLIMATE,
        );
        self.follow(
            "events: box",
            state.androidtv.subscribe_power(),
            names::ANDROID_TV,
        );
        let mut keys = state.ir.subscribe();
        let events = self.clone();
        supervised("events: ir", async move {
            loop {
                match keys.recv().await {
                    Ok(_) => events.said(names::IR_KEYS),
                    Err(broadcast::error::RecvError::Lagged(_)) => events.said(names::IR_KEYS),
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        let (events, tuya) = (self.clone(), state.tuya.clone());
        supervised("events: tuya", async move {
            for device in tuya.list_devices().await {
                match tuya.subscribe(&device.id) {
                    Ok(rx) => events.follow("events: tuya", rx, &names::tuya(&device.id)),
                    Err(error) => {
                        tracing::warn!(%error, device = %device.id, "a cat device is not followed")
                    }
                }
            }
        });
    }

    /// Says `name` each time `rx` changes, until its sender goes.
    fn follow<T: Send + Sync + 'static>(
        &self,
        task: &'static str,
        mut rx: watch::Receiver<T>,
        name: &str,
    ) {
        let (events, name) = (self.clone(), name.to_string());
        supervised(task, async move {
            while rx.changed().await.is_ok() {
                events.said(name.clone());
            }
        });
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(stream))
}

/// The stream: each change by its name; a client that fell behind is told everything changed.
async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events.subscribe();
    let changes = futures::stream::unfold(rx, |mut rx| async move {
        let name = match rx.recv().await {
            Ok(name) => name,
            Err(broadcast::error::RecvError::Lagged(_)) => names::EVERYTHING.to_string(),
            Err(broadcast::error::RecvError::Closed) => return None,
        };
        Some((Ok(Event::default().event(names::CHANGE).data(name)), rx))
    });
    Sse::new(changes)
        .keep_alive(KeepAlive::new().interval(Duration::from_millis(names::KEEP_ALIVE_MS)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_watched_change_is_said_by_its_name() {
        let events = Events::default();
        let mut heard = events.subscribe();
        let (tx, rx) = watch::channel(0);
        events.follow("test", rx, names::SHUTTERS);
        tx.send(1).unwrap();
        assert_eq!(heard.recv().await.unwrap(), names::SHUTTERS);
        tx.send(2).unwrap();
        assert_eq!(heard.recv().await.unwrap(), names::SHUTTERS);
    }

    #[tokio::test]
    async fn nobody_listening_is_no_error() {
        Events::default().said("tempo");
    }
}
