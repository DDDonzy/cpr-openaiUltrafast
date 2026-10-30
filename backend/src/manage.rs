//! 管理页的实例状态、只读候选范围与严格规则保存

use crate::state::{RuleStore, SaveRules};
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        data::AccountFactsQuery,
        host::KeyListRequest,
        management::ManagementRequest,
        middleware::{MiddlewareHeader, http::Version},
    },
    client::{HostClient, HttpBody, HttpFrame, HttpRequest, TypedCall},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
fn fault(message: &'static str) -> PluginFault {
    PluginFault::new(ErrorCode::Fault, message)
}
fn invalid(message: &'static str) -> PluginFault {
    PluginFault::new(ErrorCode::InvalidInput, message)
}

pub async fn native(
    host: &HostClient,
    method: &str,
    uri: String,
    body: Option<Value>,
) -> Result<Value, PluginFault> {
    native_with_timeout(host, method, uri, body, 20_000).await
}

async fn native_with_timeout(
    host: &HostClient,
    method: &str,
    uri: String,
    body: Option<Value>,
    timeout_ms: u64,
) -> Result<Value, PluginFault> {
    let bytes = body
        .map(|b| serde_json::to_vec(&b))
        .transpose()
        .map_err(|_| fault("管理请求编码失败"))?
        .unwrap_or_default();
    let headers = if bytes.is_empty() {
        vec![]
    } else {
        vec![MiddlewareHeader {
            name: "content-type".into(),
            value: b"application/json".to_vec(),
        }]
    };
    let mut response = host
        .dispatch_http(HttpRequest {
            method: method.into(),
            uri,
            version: Version::Http11,
            headers,
            settings: Value::Null,
            timeout_ms: Some(timeout_ms),
            body: HttpBody::from_bytes(bytes),
        })
        .await?;
    let status = response.status;
    let mut data = Vec::new();
    while let Some(frame) = response.body.read().await? {
        if let HttpFrame::Data(chunk) = frame {
            if data.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                response.body.close().await?;
                return Err(fault("宿主管理响应超过读取上限"));
            }
            data.extend_from_slice(&chunk);
        }
    }
    let value: Value =
        serde_json::from_slice(&data).map_err(|_| fault("宿主管理响应不是有效 JSON"))?;
    if !(200..300).contains(&status) || value.get("code").and_then(Value::as_i64) != Some(200) {
        return Err(PluginFault::new(
            if status == 409 {
                ErrorCode::Conflict
            } else if status == 400 || status == 422 {
                ErrorCode::InvalidInput
            } else {
                ErrorCode::Fault
            },
            if status == 409 {
                "宿主配置已变化，请刷新后重试"
            } else {
                "原生管理操作未完成，请核对配置或稍后重试"
            },
        ));
    }
    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

pub async fn keys(host: &HostClient) -> Result<Vec<Value>, PluginFault> {
    let mut out = Vec::new();
    let mut cursor = None;
    for _ in 0..20 {
        let r = host
            .list_keys(KeyListRequest { cursor, limit: 100 })
            .await?;
        out.extend(r.keys.into_iter().map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "enabled": k.enabled,
            })
        }));
        cursor = r.next_cursor;
        if cursor.is_none() {
            return Ok(out);
        }
    }
    Err(PluginFault::new(
        ErrorCode::Capacity,
        "Key 数量超过分页上限",
    ))
}

pub async fn accounts(host: &HostClient) -> Result<Vec<Value>, PluginFault> {
    let mut out = Vec::new();
    let mut cursor = None;
    for _ in 0..20 {
        let r = host
            .account_facts(AccountFactsQuery {
                provider_id: None,
                cursor,
                limit: 100,
            })
            .await?;
        out.extend(r.accounts.into_iter().map(|a| {
            json!({
                "account_id": a.account_id,
                "name": a.name,
                "email": a.email,
                "provider_id": a.provider_id,
                "enabled": a.enabled,
            })
        }));
        cursor = r.next_cursor;
        if cursor.is_none() {
            return Ok(out);
        }
    }
    Err(PluginFault::new(
        ErrorCode::Capacity,
        "账号数量超过分页上限",
    ))
}

pub async fn raw_instance(host: &HostClient, instance_id: &str) -> Result<Value, PluginFault> {
    let data = native(host, "GET", "/api/admin/plugins/instances".into(), None).await?;
    let x = data
        .as_array()
        .ok_or_else(|| fault("实例列表格式无效"))?
        .iter()
        .find(|x| x.get("id").and_then(Value::as_str) == Some(instance_id))
        .ok_or_else(|| fault("插件实例已不存在，请重新打开页面"))?;
    Ok(x.clone())
}

pub async fn instance(host: &HostClient, instance_id: &str) -> Result<Value, PluginFault> {
    let x = raw_instance(host, instance_id).await?;
    Ok(json!({
        "id":x["id"],"enabled":x["enabled"],"running":x["running"],
        "artifactSha256":x["artifactSha256"],"revision":x["revision"],
        "versionWarning":x["compatibilityWarning"],"loadError":x["loadError"],
        "runtime":x["runtime"],
    }))
}

pub async fn models(
    host: &HostClient,
    accounts: &[Value],
    rules: &[Value],
    options: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut models: BTreeSet<String> = options.iter().cloned().collect();
    for item in rules {
        for model in item["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if crate::config::valid_model(model) {
                models.insert(model.into());
            }
        }
    }
    let candidates: Vec<_> = accounts
        .iter()
        .filter(|a| a["provider_id"] == "openai" && a["enabled"] == true)
        .collect();
    let mut warnings = Vec::new();
    if candidates.len() > 16 {
        warnings.push("候选列表只采样前 16 个启用账号，其他模型可手动输入".into());
    }
    for account in candidates.into_iter().take(16) {
        let Some(id) = account["account_id"].as_str() else {
            continue;
        };
        if !crate::config::valid_ids(&[id.into()]) {
            continue;
        }
        let escaped: String = id
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let response = native_with_timeout(
            host,
            "GET",
            format!("/api/admin/accounts/models?accountId={escaped}"),
            None,
            1_500,
        )
        .await;
        match response {
            Ok(value) => {
                for model in value["models"].as_array().into_iter().flatten() {
                    if let Some(id) = model["id"]
                        .as_str()
                        .filter(|m| crate::config::valid_model(m))
                    {
                        models.insert(id.into());
                    }
                }
            }
            Err(_) => {
                if warnings.is_empty() {
                    warnings.push("部分账号的模型候选读取失败，仍可手动输入模型 ID".into());
                }
            }
        }
    }
    (models.into_iter().collect(), warnings)
}

fn input_error(e: PluginFault) -> (u16, Value) {
    (
        400,
        json!({"error":{"code":"invalid_input","message":e.message}}),
    )
}

pub async fn handle(
    call: TypedCall<ManagementRequest>,
    instance_id: &str,
    fallback: &[Value],
    model_options: &[String],
    store: &RuleStore,
) -> Result<(u16, Value), PluginFault> {
    match (call.request.method.as_str(), call.request.path.as_str()) {
        ("GET", "api/snapshot") => {
            let rules = store.snapshot(&call.host, fallback).await?;
            let accounts = accounts(&call.host).await?;
            let instance = instance(&call.host, instance_id).await?;
            let (models, warnings) =
                models(&call.host, &accounts, &rules.rules, model_options).await;
            Ok((
                200,
                json!({
                    "version":rules.version,"rules":rules.rules,"pluginVersion":env!("CARGO_PKG_VERSION"),
                    "models":models,"modelWarnings":warnings,"keys":keys(&call.host).await?,
                    "accounts":accounts,"instance":instance,
                }),
            ))
        }
        ("POST", "api/rules") => {
            let body: SaveRules = match serde_json::from_slice(&call.payload) {
                Ok(body) => body,
                Err(_) => {
                    return Ok(input_error(invalid(
                        "请求格式无效：rules 必须是规则数组，expectedVersion 必须提供，不接受未知字段",
                    )));
                }
            };
            let (rules, expected) = match body.validate() {
                Ok(v) => v,
                Err(e) => return Ok(input_error(e)),
            };
            if !rules.is_empty() {
                let key_facts = keys(&call.host).await?;
                let account_facts = accounts(&call.host).await?;
                for rule in &rules {
                    for key in rule["clientKeyIds"].as_array().expect("validated rules") {
                        if !key_facts.iter().any(|k| k["id"] == *key) {
                            return Ok(input_error(invalid(
                                "规则引用了不存在的 Key，请刷新候选列表",
                            )));
                        }
                    }
                    for account in rule["accountIds"].as_array().expect("validated rules") {
                        if !account_facts
                            .iter()
                            .any(|a| a["account_id"] == *account && a["provider_id"] == "openai")
                        {
                            return Ok(input_error(invalid(
                                "规则引用了不存在的 OpenAI 账号，请刷新候选列表",
                            )));
                        }
                    }
                }
            }
            let saved = store.save(&call.host, rules, expected).await?;
            Ok((200, json!({"version":saved.version,"rules":saved.rules})))
        }
        _ => Err(PluginFault::new(ErrorCode::Unsupported, "未知管理接口")),
    }
}
