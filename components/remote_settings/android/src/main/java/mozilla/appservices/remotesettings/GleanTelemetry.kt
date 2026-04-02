/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.remotesettings

import mozilla.appservices.remote_settings.RemoteSettingsTelemetry
import mozilla.appservices.remote_settings.UptakeEventExtras
import org.mozilla.appservices.remote_settings.GleanMetrics.RemoteSettings as RSMetrics


/**
 * GleanTelemetry is a thin wrapper used to expose
 * callbacks used to emit telemetry events to Glean.
 */
class GleanTelemetry : RemoteSettingsTelemetry {
    override fun report_uptake(extras: SyncStatusExtras) {
        RSMetrics.uptakeRemotesettings.record(extras)
    }
}
