use crate::{
    client::{
        error::{BackgroundWorkerError, ComponentError},
        WorkerMetaEvent,
    },
    telemetry::Telemetry,
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

pub struct AdsClientWorkerWrapper<T>
where
    T: Clone + Telemetry,
{
    _worker_thread: Option<JoinHandle<()>>,
    worker_dispatch: Option<SyncSender<DispatchCommand>>,

    telemetry: T,
}

impl<T: Clone + Telemetry + Send + 'static> AdsClientWorkerWrapper<T> {
    pub fn new(inner: MozAdsClientInner, telemetry: T) -> AdsClientWorkerWrapper<T> {
        let (worker_dispatch, worker_thread) =
            Option::unzip(build_worker_thread(inner.clone(), telemetry.clone()));
        AdsClientWorkerWrapper {
            _worker_thread: worker_thread,
            worker_dispatch,
            telemetry,
        }
    }

    pub fn dispatch(&self, command: DispatchCommand) -> Result<(), ComponentError> {
        let telemetry_event = command.dispatch_telemetry_event();
        if let Some(worker_dispatch) = &self.worker_dispatch {
            worker_dispatch
                .try_send(command)
                .map_err(BackgroundWorkerError::from)
                .inspect_err(|e| {
                    self.telemetry.record(e);
                })
                .inspect(|_| {
                    if let Some(event) = telemetry_event {
                        self.telemetry.record(&event);
                    }
                })?;

            Ok(())
        } else {
            Err(BackgroundWorkerError::WorkerClosed.into())
        }
    }
}

// Spawn worker thread from a reference to the client, returning a synchronous channel transmitter to the thread, and its JoinHandle.
// Returns None if thread fails to build.
pub fn build_worker_thread<T: Telemetry + Send + 'static>(
    inner_client: MozAdsClientInner,
    telemetry: T,
) -> Option<(SyncSender<DispatchCommand>, JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel(ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE);
    let worker_thread_handle = std::thread::Builder::new()
        .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
        .spawn(move || crate::worker::worker(inner_client, rx, telemetry)).inspect_err(|err| {
            error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
        }).ok()?;
    Some((tx, worker_thread_handle))
}

fn worker<T: Telemetry + Send + 'static>(
    inner_client: MozAdsClientInner,
    rx: Receiver<DispatchCommand>,
    telemetry: T,
) {
    telemetry.record(&WorkerMetaEvent::Start);

    // Synchronously run tasks in the order they are passed in this separate channel.
    while let Ok(command) = rx.recv() {
        let failure_telemetry_event = command.failed_telemetry_event();

        // Error is naturally logged through `handle_error` conversion macro.
        if command.run_command(&inner_client, &telemetry).is_err() {
            // This telemetry logs which command fails, but does not separately record the error itself.
            // Because the command hits the underlying client's method, it reuses the `.record(e)` call (eg: for RequestAdsError)
            telemetry.record(&failure_telemetry_event);
        }
    }
    telemetry.record(&WorkerMetaEvent::Stop);
}
