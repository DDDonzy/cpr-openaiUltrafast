//! 解析规则并限定账号、Key 与用户选择的模型交集

use gateway_plugin_sdk::call::middleware::{MiddlewareMount, MiddlewareRequestHead};
use serde::{Deserialize, Serialize};

pub const MAX_MODELS: usize = 256;

pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.chars().count() <= 128
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    #[serde(default)]
    pub account_ids: Vec<String>,
    #[serde(default)]
    pub client_key_ids: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error(
    "Ultrafast configuration requires bounded rules with an account or Key scope and valid model IDs"
)]
pub struct ConfigurationError;

impl Configuration {
    pub fn parse(value: serde_json::Value) -> Result<Self, ConfigurationError> {
        let config: Self = serde_json::from_value(value).map_err(|_| ConfigurationError)?;
        if config.rules.len() > 64 || config.rules.iter().any(|rule| !rule.valid()) {
            return Err(ConfigurationError);
        }
        Ok(config)
    }

    pub fn matches(&self, head: &MiddlewareRequestHead) -> bool {
        head.mount == MiddlewareMount::Attempt
            && head.protocol == "openai"
            && head.provider.as_deref() == Some("openai")
            && head.operation == "generate"
            && self.rules.iter().any(|rule| rule.matches(head))
    }
}

impl Rule {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 128
            && !self.name.chars().any(char::is_control)
            && (!self.account_ids.is_empty() || !self.client_key_ids.is_empty())
            && valid_ids(&self.account_ids)
            && valid_ids(&self.client_key_ids)
            && !self.models.is_empty()
            && self.models.len() <= MAX_MODELS
            && self.models.iter().all(|model| valid_model(model))
            && self
                .models
                .iter()
                .enumerate()
                .all(|(index, model)| !self.models[..index].contains(model))
    }

    fn matches(&self, head: &MiddlewareRequestHead) -> bool {
        head.model
            .as_ref()
            .is_some_and(|model| self.models.contains(model))
            && (self.account_ids.is_empty()
                || head
                    .account_id
                    .as_ref()
                    .is_some_and(|id| self.account_ids.contains(id)))
            && (self.client_key_ids.is_empty() || self.client_key_ids.contains(&head.client_key_id))
    }
}

pub fn valid_ids(ids: &[String]) -> bool {
    ids.len() <= 128
        && ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
        && ids.iter().all(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id.trim() == id
                && !id.chars().any(char::is_control)
        })
}
