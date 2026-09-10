use crate::{
    client::error::{BackgroundWorkerError, ComponentError},
    worker::command::DispatchCommand,
    MozAdsClientInner,
};
use std::{
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
};

pub mod command;

pub const ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE: usize = 1000;
pub const ADS_CLIENT_WORKER_THREAD_NAME: &str = "ads-client.worker";

pub struct AdsClientWorkerWrapper {
    _worker_thread: Option<JoinHandle<()>>,
    worker_dispatch: Option<SyncSender<DispatchCommand>>,
}

impl AdsClientWorkerWrapper {
    pub fn new(inner: MozAdsClientInner) -> AdsClientWorkerWrapper {
        let (worker_dispatch, worker_thread) = Option::unzip(build_worker_thread(inner.clone()));
        AdsClientWorkerWrapper {
            _worker_thread: worker_thread,
            worker_dispatch,
        }
    }

    pub fn dispatch(&self, command: DispatchCommand) -> Result<(), ComponentError> {
        if let Some(worker_dispatch) = &self.worker_dispatch {
            worker_dispatch
                .try_send(command)
                .map_err(BackgroundWorkerError::from)?;

            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }
}

// Spawn worker thread from a reference to the client, returning a synchronous channel transmitter to the thread, and its JoinHandle.
// Returns None if thread fails to build.
pub fn build_worker_thread(
    inner_client: MozAdsClientInner,
) -> Option<(SyncSender<DispatchCommand>, JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel(ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE);
    let worker_thread_handle = std::thread::Builder::new()
        .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
        .spawn(move || crate::worker::worker(inner_client, rx)).inspect_err(|err| {
            error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
        }).ok()?;
    Some((tx, worker_thread_handle))
}

fn worker(inner_client: MozAdsClientInner, rx: Receiver<DispatchCommand>) {
    // Synchronously run tasks in the order they are passed in this separate channel.
    while let Ok(command) = rx.recv() {
        // Error is naturally logged through `handle_error` conversion macro.
        let _ = command.run_command(&inner_client);
    }
}
