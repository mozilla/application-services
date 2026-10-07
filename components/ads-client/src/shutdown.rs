#[cfg(feature = "stateful")]
use parking_lot::Mutex;
#[cfg(feature = "stateful")]
use sql_support::open_database;
#[cfg(feature = "stateful")]
use std::sync::Arc;

#[cfg(feature = "stateful")]
use crate::ads_store::AdsStore;
use crate::telemetry::Telemetry;

pub struct ShutdownReferences<T: Telemetry> {
    #[cfg(feature = "stateful")]
    ads_cache_shutdown: AdsStoreShutdown,
    telemetry: T,
}

impl<T: Telemetry> ShutdownReferences<T> {
    pub fn new(
        telemetry: T,
        #[cfg(feature = "stateful")] ads_cache_shutdown: AdsStoreShutdown,
    ) -> ShutdownReferences<T> {
        ShutdownReferences {
            #[cfg(feature = "stateful")]
            ads_cache_shutdown,
            telemetry,
        }
    }

    // Shutdown anything that needs to be shut down safely and drop references to telemetry callbacks.
    // Should be called only when dropping the ads client. This may be extended to drop more things.
    pub fn shutdown(&self) -> Result<(), rusqlite::Error> {
        // Drop telemetry (within the telemetry wrapper)
        self.telemetry.shutdown();

        #[cfg(feature = "stateful")]
        self.ads_cache_shutdown.shutdown();

        // TODO: It may be prudent to call the MARSClient `shutdown_db` function here as well.
        // However, this requires a mutable lock to be held over the MARSClient (and/or AdsClient),
        // which might get held elsewhere over a network request.  We can consider re-adding this after
        // a refactor or for the new stateful sqlite database.

        Ok(())
    }
}

// TODO: Can we remove this?
// TODO: I think this is removable or at least replacing it with a Arc<AdsStore> (no more lock needed)
#[cfg(feature = "stateful")]
pub struct AdsStoreShutdown(Arc<Option<AdsStore>>);
#[cfg(feature = "stateful")]
impl AdsStoreShutdown {
    pub fn new(ads_store: Arc<Option<AdsStore>>) -> AdsStoreShutdown {
        AdsStoreShutdown(ads_store)
    }

    pub fn shutdown(&self) {
        if let Some(ads_store) = self.0.as_ref() {
            ads_store.shutdown_db();
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::{ffi::telemetry::NoopMozAdsTelemetry, MozAdsCacheConfig, MozAdsClientBuilder};
    use std::{
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    fn test_timeout<F>(timeout: Duration, func: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            func();
            tx.send(())
                .expect("Internal test error: Could not send completion signal");
        });

        match rx.recv_timeout(timeout) {
            Ok(_) => handle.join().unwrap(),
            Err(_) => panic!("Test exceeded timeout duration"),
        }
    }

    // TODO: Write new tests so that having an open query/etc is interruptable and shutdown can proceed immediately.

    #[test]
    fn test_shutdown_telemetry_basic() {
        viaduct_dev::init_backend_dev();

        // test with client created from config with no cache
        let builder = Arc::new(MozAdsClientBuilder::new()).telemetry(Box::new(NoopMozAdsTelemetry));
        let weak_reference = builder
            .fetch_telemetry()
            .expect("Inner telemetry should be Some in builder");
        let client = builder.build();

        // weak ref will show 0 strong references when the Arc<dyn MozAdsTelemetry> is gone.
        assert_ne!(weak_reference.strong_count(), 0);
        client.shutdown().unwrap();
        assert_eq!(weak_reference.strong_count(), 0);

        // test also with http cache
        let builder = Arc::new(MozAdsClientBuilder::new())
            .telemetry(Box::new(NoopMozAdsTelemetry))
            .cache_config(MozAdsCacheConfig {
                db_path: "test_shutdown_is_idempotent".to_string(),
                default_cache_ttl_seconds: None,
                max_size_mib: None,
            });
        let weak_reference = builder
            .fetch_telemetry()
            .expect("Inner telemetry should be Some in builder");
        let client = builder.build();

        // weak ref will show 0 strong references when the Arc<dyn MozAdsTelemetry> is gone.
        assert_ne!(weak_reference.strong_count(), 0);
        client.shutdown().unwrap();
        assert_eq!(weak_reference.strong_count(), 0);
    }

    #[test]
    fn test_shutdown_is_idempotent() {
        viaduct_dev::init_backend_dev();

        let builder = Arc::new(MozAdsClientBuilder::new())
            .telemetry(Box::new(NoopMozAdsTelemetry))
            .cache_config(MozAdsCacheConfig {
                db_path: "test_shutdown_is_idempotent".to_string(),
                default_cache_ttl_seconds: None,
                max_size_mib: None,
            });
        let weak_reference = builder
            .fetch_telemetry()
            .expect("Inner telemetry should be Some in builder");
        let client = builder.build();

        client.shutdown().unwrap();
        assert_eq!(weak_reference.strong_count(), 0);

        // Repeated shutdowns must not error or re-close an already closed connection.
        client.shutdown().unwrap();
        client.shutdown().unwrap();
        assert_eq!(weak_reference.strong_count(), 0);
    }
}
