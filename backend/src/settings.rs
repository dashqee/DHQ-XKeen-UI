use crate::types::*;
use axum::extract::State;
use axum::response::{IntoResponse, Json};

pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.settings.read().unwrap();
    Json(
        serde_json::json!({ "success": true, "updater": s.updater, "router_config": s.router_config, "log": s.log, "clash_api": s.clash_api, "auth": { "enabled": s.auth.enabled } }),
    )
}

pub async fn patch_settings(State(state): State<AppState>, Json(patch): Json<serde_json::Value>) -> impl IntoResponse {
    let _guard = state.app_config_lock.lock().await;
    let mut file_json = tokio::fs::read_to_string(APP_CONFIG)
        .await
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(serde_json::json!({}));

    let patch_sets_tz = patch.get("log").and_then(|l| l.get("timezone")).is_some();

    json_merge(&mut file_json, patch);

    if let serde_json::Value::Object(ref mut map) = file_json {
        if let Some(legacy) = map.remove("timezoneOffset") {
            if !patch_sets_tz {
                if let Some(log) = map.entry("log").or_insert(serde_json::json!({})).as_object_mut() {
                    log.insert("timezone".into(), legacy);
                }
            }
        }
    }

    let mut settings: AppSettings = match serde_json::from_value(file_json.clone()) {
        Ok(s) => s,
        Err(e) => return Json(serde_json::json!({"success": false, "error": e.to_string()})),
    };

    if settings.log.timezone < -12 || settings.log.timezone > 14 {
        return Json(serde_json::json!({"success": false, "error": "Неверный часовой пояс"}));
    }
    settings.clash_api.ping_url = settings.clash_api.ping_url.trim().to_string();
    if settings.clash_api.ping_url.is_empty() {
        return Json(serde_json::json!({"success": false, "error": "URL пинг-теста не может быть пустым"}));
    }
    if settings.clash_api.ping_timeout == 0 {
        return Json(serde_json::json!({"success": false, "error": "Таймаут пинг-теста должен быть больше 0"}));
    }
    settings.normalize_proxies();

    let previous = state.settings.read().unwrap().router_config.clone();
    if previous.url != settings.router_config.url || previous.auto_update != settings.router_config.auto_update {
        settings.router_config.url = settings.router_config.url.trim().to_string();
        if let Err(e) = crate::router_config::apply(&previous, &settings.router_config).await {
            return Json(serde_json::json!({"success": false, "error": e}));
        }
        if let Some(router_config) = file_json
            .get_mut("router_config")
            .and_then(|value| value.as_object_mut())
        {
            router_config.insert(
                "url".into(),
                serde_json::Value::String(settings.router_config.url.clone()),
            );
        }
    }

    let serialized = serde_json::to_string_pretty(&file_json).unwrap();
    let tmp = format!("{}.tmp", APP_CONFIG);
    match tokio::fs::write(&tmp, &serialized).await {
        Ok(_) => match tokio::fs::rename(&tmp, APP_CONFIG).await {
            Ok(_) => {
                *state.settings.write().unwrap() = settings;
                Json(serde_json::json!({"success": true}))
            }
            Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
        },
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

fn json_merge(a: &mut serde_json::Value, b: serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                json_merge(a.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (a, b) => *a = b,
    }
}
