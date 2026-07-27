use std::sync::mpsc::Receiver;
use crate::{DispatchCommand, MozAdsClientInner};

pub fn worker(inner_client : MozAdsClientInner, rx : Receiver<DispatchCommand>) {
    // TODO: This could be an async environment where we wait on buffered futures- can send many out at once, instead of FIFO.
    while let Ok(task) = rx.recv() {
        let task : DispatchCommand = task;
        task.run_command(&inner_client).expect("TODO: handle error here. maybe retries should be here?");
    }
}
