//! 覆盖规则作用域、强制档位覆盖、异常输入与大请求字节保留

use cpr_ultrafast::{config::Configuration, rewrite::ultrafast};
use gateway_plugin_sdk::call::middleware::MiddlewareRequestHead;
use serde_json::json;
fn config() -> Configuration {
    Configuration::parse(json!({"rules":[{"name":"account and key", "accountIds":["acct_a"], "clientKeyIds":["key_a"], "models":["gpt-6-astra","gpt-6.1-sol"]}]})).unwrap()
}

fn head() -> MiddlewareRequestHead {
    serde_json::from_value(json!({"settings_sources":{},"settings":null,"request_id":"req_test","client_key_id":"key_a","account_group_ids":[],"mount":"attempt","attempt_index":1,"operation":"generate","protocol":"openai","endpoint":"/v1/responses","transport":"web_socket","provider":"openai","model":"gpt-6-astra","account_id":"acct_a","headers":[]})).unwrap()
}

#[test]
fn matched_scope_requires_every_dimension_and_supported_operation() {
    let c = config();
    let h = head();
    assert!(c.matches(&h));
    for (field, value) in [
        ("client_key_id", json!("key_b")),
        ("account_id", json!("acct_b")),
        ("account_id", json!(null)),
        ("provider", json!("xai")),
        ("protocol", json!("xai")),
        ("model", json!("gpt-6-luna")),
        ("model", json!(null)),
        ("operation", json!("compact")),
        ("mount", json!("request")),
    ] {
        let mut h = serde_json::to_value(head()).unwrap();
        h[field] = value;
        assert!(
            !c.matches(&serde_json::from_value(h).unwrap()),
            "must not match {field}"
        );
    }
}

#[test]
fn either_identity_can_scope_a_rule_and_rules_are_unioned() {
    for field in ["accountIds", "clientKeyIds"] {
        let mut r = json!({"name":"one scope","models":["gpt-6-astra"]});
        r[field] = if field == "accountIds" {
            json!(["acct_a"])
        } else {
            json!(["key_a"])
        };
        assert!(
            Configuration::parse(json!({"rules":[r]}))
                .unwrap()
                .matches(&head())
        );
    }
    let c=Configuration::parse(json!({"rules":[{"name":"miss","accountIds":["acct_b"],"models":["gpt-6-astra"]},{"name":"hit","clientKeyIds":["key_a"],"models":["gpt-6-astra"]}]})).unwrap();
    assert!(c.matches(&head()));
    assert!(
        !Configuration::parse(json!({"rules":[]}))
            .unwrap()
            .matches(&head())
    );
}

#[test]
fn unsafe_or_malformed_configuration_is_rejected() {
    for v in [
        json!({}),
        json!({"rules":[],"unknown":true}),
        json!({"rules":[{"name":"all","models":["gpt-6-astra"]}]}),
        json!({"rules":[{"name":"unsupported","accountIds":["a"],"models":["gpt-6-luna"]}]}),
        json!({"rules":[{"name":"empty","accountIds":[""],"models":["gpt-6-astra"]}]}),
        json!({"rules":[{"name":"no models","accountIds":["a"],"models":[]}]}),
    ] {
        assert!(Configuration::parse(v).is_err());
    }
}

#[test]
fn replaces_only_the_top_level_value_preserving_exact_bytes() {
    let input=br#"{ "service_tier" : "priority", "input":[{"service_tier":"default","text":"quote: \" \\ \u4e2d"}], "precise":123456789012345678901234567890, "float":1.00000000000000000001e-999 }"#;
    let expected =
        String::from_utf8(input.to_vec())
            .unwrap()
            .replacen("\"priority\"", "\"ultrafast\"", 1);
    assert_eq!(ultrafast(input).unwrap().unwrap(), expected.as_bytes());
}

#[test]
fn every_explicit_tier_is_forced_regardless_of_client_choice() {
    for value in [
        "\"priority\"",
        "\"fast\"",
        "\"flex\"",
        "\"long_fast\"",
        "\"long_ultrafast\"",
        "\"default\"",
        "\"DEFAULT\"",
        "\" Default \"",
        "null",
        "17",
        "[1,2]",
        "{\"nested\":true}",
    ] {
        let input = format!("{{\"service_tier\":{value},\"input\":[]}}");
        assert_eq!(
            ultrafast(input.as_bytes()).unwrap().unwrap(),
            br#"{"service_tier":"ultrafast","input":[]}"#,
            "must force {value}"
        );
    }
}
#[test]
fn inserts_missing_field_without_altering_other_content() {
    for input in [
        &b"{}\r\n"[..],
        &b"{ \t } "[..],
        &br#"{"input":[],"nested":{"service_tier":"default"}}"#[..],
    ] {
        let output = ultrafast(input).unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(v["service_tier"], "ultrafast");
        let marker = if serde_json::from_slice::<serde_json::Value>(input)
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty()
        {
            "\"service_tier\":\"ultrafast\""
        } else {
            ", \"service_tier\":\"ultrafast\""
        };
        assert_eq!(
            String::from_utf8(output)
                .unwrap()
                .replacen(marker, "", 1)
                .as_bytes(),
            input
        );
    }
}

#[test]
fn escaped_field_name_is_recognized_and_duplicate_fields_are_rejected() {
    let input = br#"{"service\u005ftier":"default"}"#;
    assert_eq!(
        ultrafast(input).unwrap().unwrap(),
        br#"{"service\u005ftier":"ultrafast"}"#
    );
    assert!(ultrafast(br#"{"service_tier":"default","service\u005ftier":"priority"}"#).is_err());
}

#[test]
fn rejects_non_objects_and_invalid_json() {
    for input in [
        &b"[]"[..],
        &b"null"[..],
        &b"1"[..],
        &b"{"[..],
        &b"{\"input\":1} trailing"[..],
        &b"\xff"[..],
    ] {
        assert!(ultrafast(input).is_err());
    }
}

#[test]
fn handles_large_image_history_without_extra_size_cap() {
    let mut input = b"{\"input\":\"".to_vec();
    input.resize(input.len() + 17 * 1024 * 1024, b'a');
    input.extend_from_slice(b"\",\"service_tier\":null}");
    let output = ultrafast(&input).unwrap().unwrap();
    assert_eq!(&output[..input.len() - 5], &input[..input.len() - 5]);
    assert!(output.ends_with(b"\"ultrafast\"}"));
}

#[test]
fn already_ultrafast_is_preserved_without_copy_or_reserialization() {
    for input in [
        &br#"{ "service_tier": "ultrafast", "n":1e10 }"#[..],
        &br#"{"service_tier":"ultra\u0066ast"}"#[..],
    ] {
        assert!(ultrafast(input).unwrap().is_none());
    }
}
