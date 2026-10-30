//! 按已选账号、客户端 Key 和模型设置 Ultrafast，响应交还宿主管理

pub mod config;
pub mod manage;
pub mod rewrite;
pub mod state;

use std::sync::Arc;

use gateway_plugin_sdk::call::management::{
    ManagementPage, ManagementRegistration, ManagementRequest, ManagementResource, ManagementRoute,
};
use gateway_plugin_sdk::{
    ErrorCode, Manifest, PluginFault,
    call::middleware::MiddlewareHeader,
    client::{AuthorError, ComposedPlugin, PluginBuilder, RequestCall, TypedCall, TypedReply},
};

pub const PLUGIN_ID: &str = "donzy.ultrafast";
pub const MANIFEST_BYTES: &[u8] = include_bytes!("../../plugin.json");

pub fn manifest() -> Result<Manifest, gateway_plugin_sdk::ManifestError> {
    Manifest::from_author_slice(MANIFEST_BYTES)
}

fn reply(
    status: u16,
    v: serde_json::Value,
) -> Result<TypedReply<gateway_plugin_sdk::call::management::ManagementResponse>, PluginFault> {
    Ok(
        TypedReply::new(gateway_plugin_sdk::call::management::ManagementResponse {
            status,
            content_type: "application/json".into(),
            headers: vec![MiddlewareHeader {
                name: "cache-control".into(),
                value: "no-store".into(),
            }],
        })
        .with_payload(
            serde_json::to_vec(&v)
                .map_err(|_| PluginFault::new(ErrorCode::Fault, "响应编码失败"))?,
        ),
    )
}

fn safe_error(e: &PluginFault) -> String {
    match e.code {
        ErrorCode::InvalidInput => "规则字段无效，请检查名称、Key/账号范围和模型".into(),
        ErrorCode::Conflict => "规则已被其他管理员修改，请刷新后重试".into(),
        ErrorCode::Unsupported => "未知管理接口".into(),
        _ => "操作未完成，请查看 CPR 日志".into(),
    }
}

pub fn registration() -> ManagementRegistration {
    let routes = [("GET", "api/snapshot"), ("POST", "api/rules")]
        .into_iter()
        .map(|(m, p)| ManagementRoute {
            method: m.into(),
            path: p.into(),
            request_content_types: if m == "POST" {
                vec!["application/json".into()]
            } else {
                vec![]
            },
            response_content_types: vec!["application/json".into()],
        })
        .collect();
    ManagementRegistration {
        routes,
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|p| ManagementResource {
                path: p.into(),
                public: false,
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "ultrafast".into(),
            title: "Ultrafast 服务档位".into(),
            description: Some("按 Key、账号和模型强制请求 ultrafast 档位".into()),
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}

pub fn plugin(configuration: serde_json::Value) -> Result<ComposedPlugin, AuthorError> {
    let fallback: Vec<serde_json::Value> = configuration
        .get("rules")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let model_options: Vec<String> = configuration
        .get("modelOptions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|m| config::valid_model(m))
        .map(str::to_owned)
        .collect();
    let middleware_fallback = fallback.clone();
    let store = Arc::new(state::RuleStore::default());
    let middleware_store = Arc::clone(&store);
    PluginBuilder::from_json(MANIFEST_BYTES)?
        .on(
            gateway_plugin_sdk::client::methods::STATE_MIGRATE,
            |call| async move { Ok(TypedReply::new(state::migrate(call.request)?)) },
        )?
        .middleware(move |call: RequestCall| {
            let middleware_fallback = middleware_fallback.clone();
            let store = Arc::clone(&middleware_store);
            async move {
                let RequestCall {
                    mut request,
                    next,
                    cancellation,
                    host,
                    ..
                } = call;
                if cancellation.is_cancelled() {
                    return Err(PluginFault::new(
                        ErrorCode::Cancelled,
                        "Ultrafast request cancelled",
                    ));
                }
                if request.head.mount
                    == gateway_plugin_sdk::call::middleware::MiddlewareMount::Attempt
                    && request.head.protocol == "openai"
                    && request.head.provider.as_deref() == Some("openai")
                    && request.head.operation == "generate"
                {
                    let config = store.configuration(&host, &middleware_fallback).await?;
                    if config.matches(&request.head)
                        && let Some(body) = rewrite::ultrafast(&request.body).map_err(|_| {
                            PluginFault::new(
                                ErrorCode::InvalidInput,
                                "Ultrafast requires an unambiguous JSON object",
                            )
                        })?
                    {
                        request.replace_body(body);
                    }
                }
                // 不读取响应正文，让 SDK 保留流控、取消及真实上游档位
                next.run(request).await
            }
        })?
        .management(registration(), move |call: TypedCall<ManagementRequest>| {
            let fallback = fallback.clone();
            let store = Arc::clone(&store);
            let model_options = model_options.clone();
            async move {
                let instance_id = call.context.instance_id.clone();
                match manage::handle(call, &instance_id, &fallback, &model_options, &store).await {
                    Ok((status, value)) => reply(status, value),
                    Err(e) => {
                        let status = match e.code {
                            ErrorCode::InvalidInput => 400,
                            ErrorCode::Conflict => 409,
                            ErrorCode::Unsupported => 404,
                            _ => 502,
                        };
                        reply(
                            status,
                            serde_json::json!({"error":{"code":e.code,"message":safe_error(&e)}}),
                        )
                    }
                }
            }
        })?
        .build()
}
