use serde::{Deserialize, Serialize};
use tracing::{error, instrument};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FrontendErrorPayload {
    source_file: Option<String>,
    line_number: Option<i32>,
    column_number: Option<i32>,

    message: Option<String>,
    stack_trace: Option<String>,
    current_url: Option<String>,
    timestamp: Option<String>,
}

impl FrontendErrorPayload {
    /// # Panics
    ///
    /// Panics if the `FrontendErrorPayload` cannot be serialized.
    #[instrument(skip_all)]
    pub fn log(&self) {
        error!(
            "{}",
            serde_json::to_string_pretty(self).unwrap_or_else(|_| { panic!("{self:?}") })
        );
    }
}
