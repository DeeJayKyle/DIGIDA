//! Spotify Connect for the Spotify engine. A Spirc on a session of its own registers Sonora as a
//! Connect device, so other clients see what plays here and can pause, seek or skip it. Sonora's
//! queue still decides what plays: each load hands Spirc a one-track context, and a remote skip
//! comes back as the track ending.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::StreamExt as _;
use librespot_connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc};
use librespot_core::config::DeviceType;
use librespot_core::dealer::protocol::Message;
use librespot_core::{Session, SessionConfig, SpotifyUri};
use librespot_playback::mixer::{self, MixerConfig};
use librespot_playback::player::Player;
use librespot_protocol::connect::ClusterUpdate;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

use crate::Device;

const DEVICE_NAME: &str = "Sonora";

#[derive(Default)]
struct Slot {
    spirc: Option<Spirc>,
    /// The session Spirc runs on, which is the one that can ask Spotify to move playback.
    session: Option<Session>,
    /// Set when the engine goes away, so a Spirc that connects later shuts down at once.
    closed: bool,
}

pub struct Connect {
    slot: Arc<Mutex<Slot>>,
    handle: Option<Handle>,
    /// Whether the current track went through Spirc. Spirc only resumes a track it loaded.
    loaded: AtomicBool,
}

impl Connect {
    /// Registers in the background. Until Spirc is up, or if it never comes up, every call
    /// answers `false` and the engine drives the player directly.
    pub fn start(
        handle: Option<&Handle>,
        session: &Session,
        player: Arc<Player>,
        devices: UnboundedSender<Vec<Device>>,
    ) -> Self {
        let slot = Arc::new(Mutex::new(Slot::default()));
        let credentials = session.cache().and_then(|cache| cache.credentials());
        let (Some(handle), Some(credentials)) = (handle, credentials) else {
            log::warn!("connect: no runtime or stored credential, staying off Spotify Connect");
            return Self {
                slot,
                handle: None,
                loaded: AtomicBool::new(false),
            };
        };

        // Spirc connects its session itself, and a librespot session connects only once, so
        // it gets a fresh one. Autoplay is off: Sonora's queue picks what comes next.
        let config = SessionConfig {
            device_id: SessionConfig::default().device_id,
            autoplay: Some(false),
            ..session.config().clone()
        };
        let cache = session.cache().map(|cache| cache.as_ref().clone());
        let shared = slot.clone();
        handle.spawn(async move {
            // Remote volume stays off: the player's volume is Sonora's own gain, not this mixer.
            let Some(mixer) = mixer::find(None).and_then(|open| open(MixerConfig::default()).ok())
            else {
                log::warn!("connect: no mixer, staying off Spotify Connect");
                return;
            };
            let connect_config = ConnectConfig {
                name: DEVICE_NAME.to_owned(),
                device_type: DeviceType::Computer,
                disable_volume: true,
                ..ConnectConfig::default()
            };
            let connect_session = Session::new(config, cache);
            let local = connect_session.device_id().to_owned();
            // Listening has to start before Spirc connects the dealer.
            let seen = Arc::new(AtomicBool::new(false));
            watch_devices(&connect_session, devices.clone(), seen.clone());
            match Spirc::new(
                connect_config,
                connect_session.clone(),
                credentials,
                player,
                mixer,
            )
            .await
            {
                Ok((spirc, task)) => {
                    {
                        let mut slot = lock(&shared);
                        if slot.closed {
                            let _ = spirc.shutdown();
                        } else {
                            slot.spirc = Some(spirc);
                            slot.session = Some(connect_session);
                        }
                    }
                    log::info!("connect: registered as a Spotify Connect device");
                    // Spotify pushes a cluster only when something changes, and the first one
                    // went to Spirc. Until a real one arrives, list this device alone so the
                    // picker is there at all.
                    if !seen.load(Ordering::Relaxed) {
                        let _ = devices.send(vec![Device {
                            id: local,
                            name: DEVICE_NAME.to_owned(),
                            active: false,
                            local: true,
                        }]);
                    }
                    task.await;
                }
                Err(error) => log::warn!("connect: cannot register on Spotify Connect: {error}"),
            }
        });
        Self {
            slot,
            handle: Some(handle.clone()),
            loaded: AtomicBool::new(false),
        }
    }

    /// Takes over as the active device and loads `uri` as a one-track context.
    pub fn load(&self, uri: &SpotifyUri, start_playing: bool, seek_to: u32) -> bool {
        let slot = lock(&self.slot);
        let loaded = match (slot.spirc.as_ref(), uri.to_uri()) {
            (Some(spirc), Ok(uri)) => {
                let options = LoadRequestOptions {
                    start_playing,
                    seek_to,
                    ..LoadRequestOptions::default()
                };
                let request = LoadRequest::from_tracks(vec![uri], options);
                spirc.activate().and_then(|()| spirc.load(request)).is_ok()
            }
            _ => false,
        };
        self.loaded.store(loaded, Ordering::Relaxed);
        loaded
    }

    /// Resumes through Spirc, taking playback back if another device had it.
    pub fn play(&self) -> bool {
        if !self.loaded.load(Ordering::Relaxed) {
            return false;
        }
        let slot = lock(&self.slot);
        slot.spirc
            .as_ref()
            .is_some_and(|spirc| spirc.activate().and_then(|()| spirc.play()).is_ok())
    }

    /// Asks Spotify to move playback to another device, which pauses this one.
    pub fn transfer(&self, device_id: &str) {
        let (Some(handle), Some(session)) = (&self.handle, lock(&self.slot).session.clone()) else {
            return;
        };
        let to = device_id.to_owned();
        handle.spawn(async move {
            let spclient = session.spclient();
            // The target as the source too means "from whichever device is active".
            if let Err(first) = spclient.transfer(&to, &to, None).await {
                log::debug!("connect: transfer from the active device failed: {first}");
                if let Err(error) = spclient.transfer(session.device_id(), &to, None).await {
                    log::warn!("connect: cannot transfer playback: {error}");
                }
            }
        });
    }
}

impl Drop for Connect {
    /// Leaves the device list, rather than lingering there until Spotify times the device out.
    fn drop(&mut self) {
        let mut slot = lock(&self.slot);
        slot.closed = true;
        if let Some(spirc) = slot.spirc.take() {
            let _ = spirc.shutdown();
        }
    }
}

/// Forwards every cluster snapshot Spotify pushes as the account's device list.
fn watch_devices(session: &Session, devices: UnboundedSender<Vec<Device>>, seen: Arc<AtomicBool>) {
    let local = session.device_id().to_owned();
    let mut clusters = match session.dealer().listen_for(
        "hm://connect-state/v1/cluster",
        Message::from_raw::<ClusterUpdate>,
    ) {
        Ok(clusters) => clusters,
        Err(error) => {
            log::warn!("connect: cannot watch the device list: {error}");
            return;
        }
    };
    tokio::spawn(async move {
        while let Some(update) = clusters.next().await {
            let Some(list) = update.ok().and_then(|update| listed(&update, &local)) else {
                continue;
            };
            seen.store(true, Ordering::Relaxed);
            if devices.send(list).is_err() {
                break;
            }
        }
    });
}

/// This device first, then the rest by name. A snapshot with no devices says nothing.
fn listed(update: &ClusterUpdate, local: &str) -> Option<Vec<Device>> {
    let cluster = update.cluster.as_ref()?;
    let mut list: Vec<Device> = cluster
        .device
        .iter()
        .map(|(id, info)| Device {
            id: id.clone(),
            name: info.name.clone(),
            active: *id == cluster.active_device_id,
            local: id == local,
        })
        .collect();
    list.sort_by(|a, b| b.local.cmp(&a.local).then_with(|| a.name.cmp(&b.name)));
    (!list.is_empty()).then_some(list)
}

fn lock(slot: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use librespot_protocol::connect::{Cluster, DeviceInfo};

    fn update(active: &str, devices: &[(&str, &str)]) -> ClusterUpdate {
        let mut cluster = Cluster::new();
        cluster.active_device_id = active.to_owned();
        for (id, name) in devices {
            let mut info = DeviceInfo::new();
            info.name = (*name).to_owned();
            cluster.device.insert((*id).to_owned(), info);
        }
        let mut update = ClusterUpdate::new();
        update.cluster = Some(cluster).into();
        update
    }

    #[test]
    fn lists_this_device_first_and_marks_the_active_one() {
        let list = listed(
            &update("p", &[("p", "Phone"), ("a", "Alpha"), ("me", "Zed")]),
            "me",
        )
        .unwrap();
        let names: Vec<_> = list.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["Zed", "Alpha", "Phone"]);
        assert!(list[0].local && !list[0].active);
        assert!(list[2].active && !list[2].local);
    }

    #[test]
    fn an_empty_snapshot_is_ignored() {
        assert!(listed(&update("", &[]), "me").is_none());
    }
}
