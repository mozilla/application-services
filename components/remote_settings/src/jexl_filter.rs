use crate::RemoteSettingsContext;
use firefox_versioning::compare::version_compare;
use jexl_eval::Evaluator;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum ParseError {
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
    #[error("Invalid result type")]
    InvalidResultType,
}

/// The JEXL filter is getting instatiated when a new `RemoteSettingsClient` is being created.
pub struct JexlFilter {
    /// a JEXL `Evaluator` to run transforms and evaluations on.
    evaluator: Evaluator<'static>,
    /// The transformed `RemoteSettingsContext` as a `serde_json::Value`
    context: Value,
}

impl JexlFilter {
    /// Creating a new `JEXL` filter. If no `context` is set, all future `records` are being
    /// evaluated as `true` by default.
    pub(crate) fn new(context: Option<RemoteSettingsContext>) -> Self {
        let env_context = match context {
            Some(ctx) => {
                serde_json::to_value(ctx).expect("Failed to serialize RemoteSettingsContext")
            }
            None => json!({}),
        };

        Self {
            evaluator: Evaluator::new()
                // We want to add more transforms later on. We started with `versionCompare`.
                // https://remote-settings.readthedocs.io/en/latest/target-filters.html#transforms
                // The goal is to get on pare with the desktop.
                .with_transform("versionCompare", |args| Ok(version_compare(args)?)),
            context: env_context,
        }
    }

    /// Evaluates the given filter expression in the provided context.
    /// Returns `Ok(true)` if the expression evaluates to true, `Ok(false)` otherwise.
    pub(crate) fn evaluate(&self, filter_expr: &str) -> Result<bool, ParseError> {
        if filter_expr.trim().is_empty() {
            return Ok(true);
        }

        let result = self
            .evaluator
            .eval_in_context(filter_expr, &self.context)
            .map_err(|e| {
                ParseError::EvaluationError(format!("Failed to evaluate '{}': {}", filter_expr, e))
            })?;

        result.as_bool().ok_or(ParseError::InvalidResultType)
    }
}
