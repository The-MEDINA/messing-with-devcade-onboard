use crate::api::current_game;
use devcade_onboard_types::{Map, Value};
use gatekeeper_members::{GateKeeperMemberListener, RealmType};
use lazy_static::lazy_static;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use std::fmt;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

type NfcCallback = oneshot::Sender<Option<String>>;
pub struct NfcClient {
    request_queue: Mutex<Sender<NfcRequest>>,
    thread: JoinHandle<()>,
}

enum NfcRequest {
    Tags {
        callback: NfcCallback,
    },
    User {
        association_id: String,
        callback: oneshot::Sender<Option<Map<String, Value>>>,
    },
}

lazy_static! {
    pub static ref NFC_CLIENT: NfcClient = Default::default();
}

impl Default for NfcClient {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let thread = thread::spawn(|| {
            NfcClient::run(rx);
        });
        NfcClient {
            thread,
            request_queue: tx.into(),
        }
    }
}

const NFC_DEVICE_NAME: &str = "pn532_uart:/dev/ttyACM0";

impl NfcClient {
    fn run(rx: Receiver<NfcRequest>) {
        let mut association_ids: AllocRingBuffer<(String, String)> = AllocRingBuffer::new(8);
        loop {
            // Unwrap rationale: If the main thread is crashed, not much we can do
            let mut callback = rx.recv().unwrap();
            // Unwrap rationale: If we can't allocate memory, we're not long for this world anyways
            let mut listener = match GateKeeperMemberListener::new(
                NFC_DEVICE_NAME.to_string(),
                RealmType::MemberProjects,
            ) {
                Some(listener) => listener,
                None => {
                    log::error!("Couldn't build Gatekeeper listener?");
                    // Unwrap rationale: If the main thread is crashed, not much we can do
                    match callback {
                        NfcRequest::User { callback, .. } => callback.send(None).unwrap(),
                        NfcRequest::Tags { callback } => callback.send(None).unwrap(),
                    }
                    continue;
                }
            };

            loop {
                match callback {
                    NfcRequest::User {
                        callback,
                        association_id: association_handle,
                    } => {
                        let association_id =
                            (&association_ids)
                                .into_iter()
                                .find_map(|(handle, association_id)| {
                                    match handle == &association_handle {
                                        true => Some(association_id),
                                        false => None,
                                    }
                                });
                        callback
                            .send(
                                association_id
                                    .and_then(|association_id| {
                                        listener.fetch_user(association_id.clone()).ok()
                                    })
                                    .and_then(|user| user["user"].as_object().cloned()),
                            )
                            .unwrap();
                    }
                    NfcRequest::Tags { callback } => {
                        let association_id =
                            listener
                                .poll_for_user()
                                .map(|association_id| {
                                    match (&association_ids).into_iter().find(
                                        |(_, candidate_association_id)| {
                                            candidate_association_id == &association_id
                                        },
                                    ) {
                                        Some((handle, _)) => handle.clone(),
                                        None => {
                                            let game_uuid = current_game().id;
                                            let handle = sha256::digest(format!(
                                                "{association_id}:{game_uuid}"
                                            ));
                                            association_ids.push((handle.clone(), association_id));
                                            handle
                                        }
                                    }
                                });
                        // Unwrap rationale: If the main thread is crashed, not much we can do
                        callback.send(association_id).unwrap();
                    }
                }

                if let Ok(new_request) = rx.recv_timeout(Duration::from_secs(30)) {
                    callback = new_request;
                } else {
                    break;
                }
            }
        }
    }
    pub async fn submit(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let (tx, rx) = oneshot::channel();

        self.request_queue
            .lock()
            .await
            .send(NfcRequest::Tags { callback: tx })?;
        Ok(rx.await?)
    }
    pub async fn get_user(
        &self,
        association_id: String,
    ) -> Result<Map<String, Value>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();

        self.request_queue.lock().await.send(NfcRequest::User {
            association_id,
            callback: tx,
        })?;
        match rx.await? {
            Some(user) => Ok(user),
            None => Err(anyhow::anyhow!("User not found with that association ID")),
        }
    }
}

#[derive(Debug)]
struct NfcThreadError;

impl fmt::Display for NfcThreadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NfcThreadError")
    }
}

impl std::error::Error for NfcThreadError {}
