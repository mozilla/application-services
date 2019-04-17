pub mod msg_types {
    include!(concat!(env!("OUT_DIR"), "/msg_types.rs"));
}

use ffi_support::implement_into_ffi_by_protobuf;
use sync15::telemetry::{SyncFailure, Stopwatch, EngineIncoming, Engine};

use crate::msg_types::{EngineFailureReason, EnginePingPayload, EngineOutgoingPayload, EngineIncomingPayload};

impl From<EngineIncoming> for EngineIncomingPayload {
    fn from(i: EngineIncoming) -> EngineIncomingPayload {
        EngineIncomingPayload {
            applied: Some(i.applied),
            failed: Some(i.failed),
            new_failed: Some(i.new_failed),
            reconciled: Some(i.reconciled),
        }
    }
}


impl From<Engine> for EnginePingPayload {
    fn from(e: Engine) -> EnginePingPayload {
        let (failure_reason, failure_text, failure_code) = match e.failure {
            Some(SyncFailure::Shutdown) => (Some(EngineFailureReason::Shutdown), None, None),
            Some(SyncFailure::Other { error }) => (Some(EngineFailureReason::Other), Some(error), None),
            Some(SyncFailure::Unexpected { error }) => (Some(EngineFailureReason::Unexpected), Some(error), None),
            Some(SyncFailure::Auth { from }) => (Some(EngineFailureReason::Auth), Some(from), None),
            Some(SyncFailure::Http { code }) => (Some(EngineFailureReason::Http), None, Some(code)),
            None => (None, None, None),
        };
        EnginePingPayload {
            name: e.name,
            took: match e.when_took {
                Stopwatch::Started(_, _) => None,
                Stopwatch::Finished(t) => Some(t.took),
            },
            incoming: e.incoming.map(EngineIncomingPayload::from).unwrap_or_default(),
            outgoing: e.outgoing.into_iter().fold(EngineOutgoingPayload::default(), |acc, outgoing| {
                EngineOutgoingPayload {
                    sent: Some(acc.sent.unwrap_or(0u64) + outgoing.sent as u64),
                    failed: Some(acc.failed.unwrap_or(0u64) + outgoing.failed as u64),
                    batches: Some(acc.batches.unwrap_or(0u64) + 1),
                }
            }),
            failure_reason: failure_reason.map(|r| r as i32),
            failure_text,
            failure_code,
        }
    }
}

implement_into_ffi_by_protobuf!(EnginePingPayload);
