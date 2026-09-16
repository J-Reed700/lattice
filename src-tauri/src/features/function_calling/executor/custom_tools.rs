//! User-configured custom query tools: execution and argument validation.

use super::FunctionExecutor;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::*;
use crate::features::settings::dto::CustomToolSettingsDto;
use crate::shared::error::{AppError, Result};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE};
use url::Url;

impl FunctionExecutor {
    /// Execute a configured custom query tool.
    pub(super) fn handle_custom_query_tool<'a>(
        &'a self,
        tool: &'a CustomToolSettingsDto,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: CustomQueryToolInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            let mut parsed_url = Url::parse(tool.endpoint.trim()).map_err(|e| {
                AppError::InvalidInput(format!(
                    "Invalid endpoint for custom tool '{}': {}",
                    tool.name, e
                ))
            })?;

            let requested_max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(tool.default_max_results as u64)
                .max(1);

            {
                let mut query_pairs = parsed_url.query_pairs_mut();
                query_pairs.append_pair(tool.query_param.trim(), input.query.trim());
                if let Some(max_results_param) = &tool.max_results_param {
                    query_pairs
                        .append_pair(max_results_param.trim(), &requested_max_results.to_string());
                }
            }

            let request_url = parsed_url.to_string();
            self.web_service.validate_url(&request_url)?;

            let response = self
                .http_client
                .get(&request_url)
                .header(ACCEPT, "application/json, text/plain;q=0.9, */*;q=0.8")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .send()
                .await
                .map_err(|e| {
                    AppError::Network(format!("Custom tool '{}' request failed: {}", tool.name, e))
                })?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Custom tool '{}' returned HTTP {}",
                    tool.name,
                    response.status()
                )));
            }

            let payload = response.json::<serde_json::Value>().await.map_err(|e| {
                AppError::Network(format!(
                    "Custom tool '{}' returned non-JSON payload: {}",
                    tool.name, e
                ))
            })?;

            let output = CustomQueryToolOutput {
                tool_name: tool.name.clone(),
                request_url,
                data: payload,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }

    pub(super) fn validate_custom_query_arguments(arguments: &serde_json::Value) -> Result<()> {
        let object = arguments
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("Arguments must be a JSON object".to_string()))?;

        let query = object
            .get("query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| AppError::InvalidInput("Missing required field 'query'".to_string()))?;

        if query.is_empty() {
            return Err(AppError::InvalidInput(
                "Missing required field 'query'".to_string(),
            ));
        }

        if let Some(max_results) = object.get("max_results") {
            let as_u64 = max_results
                .as_u64()
                .or_else(|| {
                    max_results
                        .as_i64()
                        .and_then(|value| u64::try_from(value).ok())
                })
                .or_else(|| {
                    max_results.as_f64().and_then(|value| {
                        let integer = value.trunc();
                        if (value - integer).abs() <= f64::EPSILON && integer >= 0.0 {
                            Some(integer as u64)
                        } else {
                            None
                        }
                    })
                });

            match as_u64 {
                Some(value) if (1..=100).contains(&value) => {}
                _ => {
                    return Err(AppError::InvalidInput(
                        "Field 'max_results' must be an integer between 1 and 100".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}
