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
            timeout_ms: Some(20_000),
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
        return Err(fault("原生管理操作未完成"));
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

pub async fn instance(host: &HostClient, instance_id: &str) -> Result<Value, PluginFault> {
    let data = native(host, "GET", "/api/admin/plugins/instances".into(), None).await?;
    let x = data
        .as_array()
        .ok_or_else(|| fault("实例列表格式无效"))?
        .iter()
        .find(|x| x.get("id").and_then(Value::as_str) == Some(instance_id))
        .ok_or_else(|| fault("插件实例已不存在，请重新打开页面"))?;
    Ok(json!({
        "id":x["id"],"enabled":x["enabled"],"running":x["running"],
        "artifactSha256":x["artifactSha256"],"revision":x["revision"],
        "versionWarning":x["compatibilityWarning"],"loadError":x["loadError"],
        "runtime":x["runtime"],"bindings":x["bindings"],
    }))
}

fn list_allows(binding: &Value, field: &str, value: &str) -> bool {
    binding
        .get(field)
        .and_then(Value::as_array)
        .is_some_and(|list| list.is_empty() || list.iter().any(|item| item.as_str() == Some(value)))
}

/// 不替管理员扩大宿主绑定，拒绝那些保存成功却进不了中间件的规则
pub fn validate_binding_scope(rules: &[Value], instance: &Value) -> Result<(), PluginFault> {
    if rules.is_empty() {
        return Ok(());
    }
    let bindings = instance["bindings"]
        .as_array()
        .ok_or_else(|| invalid("缺少入口绑定，请在插件管理中配置 attempt 绑定"))?;
    for rule in rules {
        let keys = rule["clientKeyIds"]
            .as_array()
            .ok_or_else(|| invalid("Key 范围无效"))?;
        for model in rule["models"]
            .as_array()
            .ok_or_else(|| invalid("模型范围无效"))?
        {
            let matches = |key: Option<&str>| {
                bindings.iter().any(|b| {
                    b["contribution"].as_str() == Some("donzy.ultrafast.middleware")
                        && b["stage"].as_str() == Some("attempt")
                        && list_allows(b, "providerIds", "openai")
                        && list_allows(b, "models", model.as_str().unwrap_or_default())
                        && match key {
                            Some(key) => list_allows(b, "clientKeyIds", key),
                            None => b["clientKeyIds"].as_array().is_some_and(Vec::is_empty),
                        }
                })
            };
            if keys.is_empty() {
                if !matches(None) {
                    return Err(invalid(
                        "当前入口绑定限制了 Key，请明确选择入口允许的 Key，或由管理员调整绑定",
                    ));
                }
            } else if keys.iter().any(|key| !matches(key.as_str())) {
                return Err(invalid(
                    "所选 Key 或模型不在入口绑定范围内，未保存，请先由管理员调整绑定",
                ));
            }
        }
    }
    Ok(())
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
    store: &RuleStore,
) -> Result<(u16, Value), PluginFault> {
    match (call.request.method.as_str(), call.request.path.as_str()) {
        ("GET", "api/snapshot") => {
            let rules = store.snapshot(&call.host, fallback).await?;
            Ok((
                200,
                json!({
                    "version":rules.version,"rules":rules.rules,"pluginVersion":env!("CARGO_PKG_VERSION"),
                    "models":crate::config::SUPPORTED_MODELS,"keys":keys(&call.host).await?,
                    "accounts":accounts(&call.host).await?,"instance":instance(&call.host,instance_id).await?,
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
                let inst = instance(&call.host, instance_id).await?;
                if let Err(e) = validate_binding_scope(&rules, &inst) {
                    return Ok(input_error(e));
                }
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
