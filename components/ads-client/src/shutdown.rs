use crate::{ffi::telemetry::MozAdsTelemetryWrapper, telemetry::Telemetry};

pub struct ShutdownReferences {
    telemetry: MozAdsTelemetryWrapper,
}

impl ShutdownReferences {
    pub fn new(telemetry: MozAdsTelemetryWrapper) -> ShutdownReferences {
        ShutdownReferences { telemetry }
    }

    // Shutdown anything that needs to be shut down safely and drop references to telemetry callbacks.
    // Should be called only when dropping the ads client. This may be extended to drop more things.
    pub fn shutdown(&self) {
        // Drop telemetry (within the telemetry wrapper)
        self.telemetry.shutdown();

        // TODO: It may be prudent to call the MARSClient `shutdown_db` function here as well.
        // However, this requires a mutable lock to be held over the MARSClient (and/or AdsClient),
        // which might get held elsewhere over a network request.  We can consider re-adding this after
        // a refactor or for the new stateful sqlite database.
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

    // Shutdown procedure must not require a lock to be held on the inner AdsClient.
    // This is because sync functions like `request_tile_ads` require (at worst) to wait on a hanging non-cancellable network request to resolve,
    // and they hold the lock for the entirety of that time. Shutdown should only require the minimal amount of waiting/locking possible.
    #[test]
    fn shutdown_does_not_require_ads_client_lock() {
        test_timeout(Duration::from_secs(5), || {
            let builder = MozAdsClientBuilder::new().build();
            let lock = builder.inner.lock();

            // Holding a inner lock, we try to run shutdown.
            builder.shutdown().unwrap();

            // We explicitly drop the lock at the end.
            drop(lock);
        });
    }

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
