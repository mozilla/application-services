/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::FirefoxAccount;
use crate::{FirefoxAccountEventHandler, Profile};

// [`FirefoxAccountEventHandler`] to use if the app doesn't set one, it's a noop
pub(crate) struct DefaultFirefoxAccountEventHandler;
impl FirefoxAccountEventHandler for DefaultFirefoxAccountEventHandler {
    fn profile_updated(&self, _profile: Profile) {}
}

impl FirefoxAccount {
    /// Register a new event handler that is implemented by the application
    pub(crate) fn register_event_handler(
        &mut self,
        event_handler: Box<dyn FirefoxAccountEventHandler>,
    ) {
        self.event_handler = event_handler;
    }

    /// Unregister the event handler set by the app, and resets it to the default, no-op handler
    pub(crate) fn unregister_event_handler(&mut self) {
        self.event_handler = Box::new(DefaultFirefoxAccountEventHandler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_register_unregister_event_handler() {
        struct CustomEventHandler {
            called: Arc<Mutex<u32>>,
        }
        impl FirefoxAccountEventHandler for CustomEventHandler {
            fn profile_updated(&self, _profile: Profile) {
                *self.called.lock().unwrap() += 1;
            }
        }

        // We keep a thread safe counter of the number of times the `profile_updated`
        // was called on our callback interface
        let called = Arc::new(Mutex::new(0));
        let handler = CustomEventHandler {
            called: Arc::clone(&called),
        };

        let mut fxa = FirefoxAccount::new("", "", "", None);
        fxa.register_event_handler(Box::new(handler));
        fxa.event_handler.profile_updated(Default::default());
        assert_eq!(*called.lock().unwrap(), 1);
        fxa.event_handler.profile_updated(Default::default());
        assert_eq!(*called.lock().unwrap(), 2);

        // We unregister the event handler, so now subsequent `profile_updated` events are no-ops
        fxa.unregister_event_handler();
        fxa.event_handler.profile_updated(Default::default());
        // We verify the last `profile_updated` call was a no-op, the counter was not updated
        assert_eq!(*called.lock().unwrap(), 2);
    }
}
