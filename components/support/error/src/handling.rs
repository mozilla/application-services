/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Helpers for components to "handle" errors.

/// Describes what error reporting action should be taken.
#[derive(Debug, Default)]
pub struct ErrorReporting {
    /// If Some(level), will write a log message at that level.
    log_level: Option<log::Level>,
    /// If Some(report_class) will call the error reporter with details.
    report_class: Option<String>,
}

/// Specifies how an "internal" error is converted to an "external" public error and
/// any logging or reporting that should happen.
pub struct ErrorHandling<E> {
    /// The external error that should be returned.
    pub err: E,
    /// How the error should be reported.
    pub reporting: ErrorReporting,
}

impl<E> ErrorHandling<E> {
    // Some helpers to cut the verbosity down.
    /// Just convert the error without any special logging or error reporting.
    pub fn passthrough(err: E) -> Self {
        Self {
            err,
            reporting: ErrorReporting::default(),
        }
    }

    /// Just convert and log the error without any special error reporting.
    pub fn log(err: E, level: log::Level) -> Self {
        Self {
            err,
            reporting: ErrorReporting {
                log_level: Some(level),
                ..Default::default()
            },
        }
    }

    /// Convert, report and log the error.
    pub fn report(err: E, level: log::Level, report_class: String) -> Self {
        Self {
            err,
            reporting: ErrorReporting {
                log_level: Some(level),
                report_class: Some(report_class),
            },
        }
    }

    /// Convert, report and log the error in a way suitable for "unexpected" errors.
    // (With more generics we might be able to abstract away the creation of `err`,
    // but that will have a significant complexity cost for only marginal value)
    pub fn unexpected(err: E, report_class: Option<&str>) -> Self {
        Self::report(
            err,
            log::Level::Error,
            report_class.unwrap_or("unexpected").to_string(),
        )
    }
}

/// A trait to define how errors are converted and reported.
pub trait GetErrorHandling {
    type ExternalError;

    /// Return how to handle our internal errors
    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError>;
}

/// Handle the specified "internal" error, taking any logging or error
/// reporting actions and converting the error to the public error.
/// Called by our `handle_error` macro so needs to be public.
pub fn convert_log_report_error<IE, EE>(e: IE) -> EE
where
    IE: GetErrorHandling<ExternalError = EE> + std::error::Error,
    EE: std::error::Error,
{
    let handling = e.get_error_handling();
    let reporting = handling.reporting;
    if let Some(level) = reporting.log_level {
        log::log!(level, "{}", e.to_string());
    }
    if let Some(report_class) = reporting.report_class {
        // notify the error reporter if the feature is enabled.
        // XXX - should we arrange for the `report_class` to have the
        // original crate calling this as a prefix, or will we still be
        // able to identify that?
        #[cfg(feature = "reporting")]
        crate::report_error(report_class, e.to_string());
        #[cfg(not(feature = "reporting"))]
        let _ = report_class; // avoid clippy warning when feature's not enabled.
    }
    handling.err
}
