/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.errorsupport

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import mozilla.appservices.tracing.EventSink
import mozilla.appservices.tracing.EventSinkSpecification
import mozilla.appservices.tracing.EventTarget
import mozilla.appservices.tracing.TracingEvent
import mozilla.appservices.tracing.TracingLevel
import mozilla.appservices.tracing.registerEventSink
import mozilla.telemetry.glean.Glean
import org.mozilla.appservices.errorsupport.GleanMetrics.Pings
import org.mozilla.appservices.errorsupport.GleanMetrics.RustComponentErrors

/**
 * RustErrorTelemetry forwarder
 *
 * This receives error pings from Rust and sends them to Glean.
 * It's only necessary because we currently can't call Glean directly from Rust
 */
public object RustComponentsErrorTelemetry {
    /**
     * Register the RustComponentsErrorTelemetry and start forwarding telemetry to glean
     */
    fun register() {
        Glean.registerPings(Pings)
        val spec = EventSinkSpecification(
            targets = listOf(EventTarget("app-services-error-reporter", TracingLevel.DEBUG)),
        )
        registerEventSink(spec, ErrorEventSink())
    }
}

@Serializable
internal data class TracingErrorFields(
    @SerialName("type_name")
    val typeName: String,
    val breadcrumbs: String,
)

@Serializable
internal data class TracingBreadcrumbFields(
    val module: String,
    val line: UInt,
    val column: UInt,
)

private class ErrorEventSink : EventSink {
    val json = Json { ignoreUnknownKeys = true }

    override fun onEvent(event: TracingEvent) {
        if (event.target == "app-services-error-reporter::error") {
            val fields = json.decodeFromString<TracingErrorFields>(event.fields)
            RustComponentErrors.errorType.set(fields.typeName)
            RustComponentErrors.details.set(event.message)
            RustComponentErrors.breadcrumbs.set(fields.breadcrumbs.split("\n"))
            Pings.rustComponentErrors.submit()

            ApplicationErrorReporterRegistry.errorReporter?.reportError(fields.typeName, event.message)
        } else if (event.target == "app-services-error-reporter::breadcrumb") {
            val fields = json.decodeFromString<TracingBreadcrumbFields>(event.fields)

            ApplicationErrorReporterRegistry.errorReporter?.reportBreadcrumb(
                event.message,
                fields.module,
                fields.line,
                fields.column,
            )
        }
    }
}

/**
 * Report Rust errors to Sentry (supplied by the application
 *
 * This represents the legacy error reporting interface. We're keeping this around for now so that
 * Android can send errors to Sentry.  At some point we should migrate Android to only use
 * Glean-based error reporting.
 */
public interface ApplicationErrorReporter {
    /**
     * Report an error
     */
    fun reportError(typeName: String, message: String)

    /**
     * Report a breadbcrumb
     */
    fun reportBreadcrumb(message: String, module: String, line: UInt, column: UInt)
}

/**
 * Set the global ApplicationErrorReporter
 */
public fun setApplicationErrorReporter(errorReporter: ApplicationErrorReporter) {
    ApplicationErrorReporterRegistry.errorReporter = errorReporter
}

/**
 * Unset the global ApplicationErrorReporter
 */
public fun unsetApplicationErrorReporter() {
    ApplicationErrorReporterRegistry.errorReporter = null
}

internal object ApplicationErrorReporterRegistry {
    var errorReporter: ApplicationErrorReporter? = null
}
