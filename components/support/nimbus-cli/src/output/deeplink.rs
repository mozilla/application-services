// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};

use crate::protocol::StartAppProtocol;
use crate::{AppOpenArgs, LaunchableApp};

impl LaunchableApp {
    pub(crate) fn create_deeplink(&self, open: &AppOpenArgs) -> Result<Option<String>> {
        let deeplink = &open.deeplink;
        if deeplink.is_none() {
            return Ok(None);
        }
        let deeplink = self.prepend_scheme(deeplink.as_ref().unwrap())?;
        Ok(Some(deeplink))
    }

    fn prepend_scheme(&self, deeplink: &str) -> Result<String> {
        Ok(if deeplink.contains("://") {
            deeplink.to_string()
        } else {
            let scheme = self.mandatory_scheme()?;
            format!("{scheme}://{deeplink}")
        })
    }

    fn mandatory_scheme(&self) -> Result<&str> {
        match self {
            Self::Android { scheme, .. } | Self::Ios { scheme, .. } => scheme
                .as_deref()
                .ok_or_else(|| anyhow!("A scheme is not defined for this app")),
        }
    }

    pub(crate) fn longform_deeplink_url(
        &self,
        deeplink: &str,
        app_protocol: StartAppProtocol,
    ) -> Result<String> {
        use percent_encoding::{AsciiSet, CONTROLS};
        const QUERY: &AsciiSet = &CONTROLS
            // The following are the special query percent encode set.
            // https://url.spec.whatwg.org/#query-percent-encode-set
            .add(b' ')
            .add(b'"')
            .add(b'<')
            .add(b'>')
            .add(b'#')
            .add(b'\'')
            // Additionally, we've added '{' and '}' to make  sure iOS simctl works with it.
            .add(b'{')
            .add(b'}')
            // Then some belt and braces: we're quoting a single query attribute value.
            .add(b':')
            .add(b'/')
            .add(b'?')
            .add(b'&');

        let StartAppProtocol {
            reset_db,
            experiments,
            log_state,
        } = app_protocol;
        if !reset_db && experiments.is_none() && !log_state {
            return Ok(deeplink.to_string());
        }

        let mut parts: Vec<_> = vec!["--nimbus-cli".to_string()];
        if let Some(v) = experiments {
            let json = serde_json::to_string(v)?;
            let string = percent_encoding::utf8_percent_encode(&json, QUERY).to_string();
            parts.push(format!("--experiments={string}"));
        }

        if reset_db {
            parts.push("--reset-db".to_string());
        }
        if log_state {
            parts.push("--log-state".to_string());
        }

        let suffix = if deeplink.contains('?') { '&' } else { '?' };

        Ok(format!("{deeplink}{suffix}{args}", args = parts.join("&")))
    }
}
