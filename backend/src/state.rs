//! 插件实例私有规则的严格读写与缓存，保存与刷新共用异步锁

use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::host::{
        StateGetRequest, StateGetResult, StateMigrationChange, StateMigrationRequest,
        StateMigrationResult, StatePutRequest, StatePutResult,
    },
    client::HostClient,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub const NAMESPACE: &str = "ultrafast-rules";
pub const KEY: &str = "document";
pub const SCHEMA_VERSION: u32 = 2;
const CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Serialize)]
pub struct RuleSet {
    pub version: Option<u64>,
    pub rules: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveRules {
    pub rules: Vec<crate::config::Rule>,
    // Require the field even for creation; null is an explicit create-only CAS
    pub expected_version: Value,
}

impl SaveRules {
    pub fn validate(self) -> Result<(Vec<Value>, Option<u64>), PluginFault> {
        let version = if self.expected_version.is_null() {
            None
        } else {
            Some(
                self.expected_version
                    .as_u64()
                    .ok_or_else(|| invalid("规则版本必须是非负整数或 null"))?,
            )
        };
        let rules = normalize(json!(self.rules))?;
        Ok((rules, version))
    }
}

fn invalid(message: &'static str) -> PluginFault {
    PluginFault::new(ErrorCode::InvalidInput, message)
}
fn fault(message: &'static str) -> PluginFault {
    PluginFault::new(ErrorCode::Fault, message)
}

pub fn normalize(rules: Value) -> Result<Vec<Value>, PluginFault> {
    let mut config = crate::config::Configuration::parse(json!({"rules":rules}))
        .map_err(|_| invalid("规则字段无效：必须限定 Key 或账号，模型、ID 与名称须合法且不重复"))?;
    for rule in &mut config.rules {
        rule.name = rule.name.trim().to_owned();
    }
    let value = serde_json::to_value(config).map_err(|_| fault("规则编码失败"))?;
    if serde_json::to_vec(&value)
        .map_err(|_| fault("规则编码失败"))?
        .len()
        > 60 * 1024
    {
        return Err(invalid("规则总大小超过 60 KiB，请缩小范围或减少规则"));
    }
    Ok(value["rules"]
        .as_array()
        .ok_or_else(|| fault("规则编码失败"))?
        .clone())
}

/// 宿主负责排空、暂存与原子提交；插件只验证旧规则，不改写其值或版本。
pub fn migrate(request: StateMigrationRequest) -> Result<StateMigrationResult, PluginFault> {
    if request.namespace != NAMESPACE
        || request.from_schema_version != 1
        || request.to_schema_version != SCHEMA_VERSION
    {
        return Err(invalid("不支持的规则状态迁移"));
    }
    let mut changes = Vec::with_capacity(request.records.len());
    for record in request.records {
        if record.key != KEY {
            return Err(invalid("未知规则状态键，迁移已取消"));
        }
        crate::config::Configuration::parse(record.value)
            .map_err(|_| invalid("旧规则校验失败，迁移已取消"))?;
        changes.push(StateMigrationChange::Keep { key: record.key });
    }
    Ok(StateMigrationResult { changes })
}

#[derive(Default)]
pub struct RuleStore {
    cache: Mutex<Option<(Instant, crate::config::Configuration)>>,
}

impl RuleStore {
    async fn read(host: &HostClient, fallback: &[Value]) -> Result<RuleSet, PluginFault> {
        let reply = host
            .call(
                "host.state.get",
                json!(StateGetRequest {
                    namespace: NAMESPACE.into(),
                    key: KEY.into()
                }),
                vec![],
            )
            .await
            .map_err(|e| e.into_plugin_fault())?;
        let result: StateGetResult =
            serde_json::from_value(reply.result).map_err(|_| fault("规则状态格式无效"))?;
        match result.record {
            None => Ok(RuleSet {
                version: None,
                rules: normalize(json!(fallback))?,
            }),
            Some(record) => {
                if record.schema_version != SCHEMA_VERSION {
                    return Err(fault("规则状态版本不受支持"));
                }
                // Use the same strict contract as the middleware, including unknown fields
                let config = crate::config::Configuration::parse(record.value)
                    .map_err(|_| fault("规则状态格式无效"))?;
                Ok(RuleSet {
                    version: Some(record.version),
                    rules: normalize(json!(config.rules))?,
                })
            }
        }
    }

    fn cached(rules: &RuleSet) -> Result<(Instant, crate::config::Configuration), PluginFault> {
        let config = crate::config::Configuration::parse(json!({"rules":rules.rules}))
            .map_err(|_| fault("规则状态格式无效"))?;
        Ok((Instant::now(), config))
    }

    pub async fn snapshot(
        &self,
        host: &HostClient,
        fallback: &[Value],
    ) -> Result<RuleSet, PluginFault> {
        let mut cache = self.cache.lock().await;
        let rules = Self::read(host, fallback).await?;
        *cache = Some(Self::cached(&rules)?);
        Ok(rules)
    }

    pub async fn configuration(
        &self,
        host: &HostClient,
        fallback: &[Value],
    ) -> Result<crate::config::Configuration, PluginFault> {
        let mut cache = self.cache.lock().await;
        if let Some((at, config)) = cache.as_ref()
            && at.elapsed() < CACHE_TTL
        {
            return Ok(config.clone());
        }
        // Only one in-flight read per instance; a slow read cannot overwrite a newer save
        let next = Self::cached(&Self::read(host, fallback).await?)?;
        let config = next.1.clone();
        *cache = Some(next);
        Ok(config)
    }

    pub async fn save(
        &self,
        host: &HostClient,
        rules: Vec<Value>,
        expected_version: Option<u64>,
    ) -> Result<RuleSet, PluginFault> {
        let mut cache = self.cache.lock().await;
        // Invalidate before the RPC: a lost reply can follow a committed state change
        *cache = None;
        let reply = host
            .call(
                "host.state.put",
                json!(StatePutRequest {
                    namespace: NAMESPACE.into(),
                    key: KEY.into(),
                    value: json!({"rules":rules}),
                    expected_version,
                }),
                vec![],
            )
            .await
            .map_err(|e| e.into_plugin_fault())?;
        let saved: StatePutResult = serde_json::from_value(reply.result)
            .map_err(|_| fault("规则保存结果无效，请刷新确认"))?;
        let rules = RuleSet {
            version: Some(saved.version),
            rules,
        };
        *cache = Some(Self::cached(&rules)?);
        Ok(rules)
    }
}
