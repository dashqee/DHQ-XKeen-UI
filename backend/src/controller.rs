use crate::logger::log;
use crate::types::*;
use axum::extract::State;
use axum::response::{IntoResponse, Json};
use serde::Deserialize;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;

#[derive(Deserialize)]
pub struct ControlReq {
    action: String,
    #[serde(default)]
    core: String,
}

pub fn find_init_file(log_enabled: bool) -> Option<String> {
    let (mut path, mut source) = (None, "fallback");

    if let Ok(content) = std::fs::read_to_string("/opt/sbin/.xkeen/01_info/01_info_variable.sh") {
        let (mut dir, mut file) = (None, None);
        for line in content.lines() {
            let clean = line.split('#').next().unwrap_or("").trim();
            if let Some(v) = clean.strip_prefix("initd_dir=") {
                dir = Some(v.trim_matches(&['"', '\''][..]));
            } else if let Some(v) = clean.strip_prefix("initd_file=") {
                file = Some(v.trim_matches(&['"', '\''][..]));
            }
        }
        if let (Some(d), Some(f)) = (dir, file) {
            path = Some(f.replace("$initd_dir", d));
            source = "var";
        }
    }

    let final_path = path.or_else(|| Path::new(S99XKEEN).exists().then(|| S99XKEEN.to_string()));

    if log_enabled {
        if let Some(p) = &final_path {
            println!("{} [INFO] Defined initd_file ({}): {}", crate::logger::ts(), source, p);
        }
    }

    final_path
}

async fn resolve_init_file(state: &AppState) -> Result<String, String> {
    if let Some(path) = state.init_file.read().unwrap().clone() {
        if Path::new(&path).exists() {
            return Ok(path);
        }
    }
    let new_path = tokio::task::spawn_blocking(|| find_init_file(false))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Не найден init файл XKeen".to_string())?;
    println!("{} [INFO] Updated initd_file: {}", crate::logger::ts(), new_path);
    *state.init_file.write().unwrap() = Some(new_path.clone());
    Ok(new_path)
}

async fn select_mihomo(path: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).await.map_err(|e| e.to_string())?;
    let mut found = false;
    let updated = content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("name_client=") {
                found = true;
                let indent_len = line.len() - line.trim_start().len();
                format!("{}name_client=\"mihomo\"", &line[..indent_len])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !found {
        return Err("Init-файл XKeen не содержит параметр name_client".into());
    }
    if updated != content.trim_end_matches('\n') {
        fs::write(path, format!("{updated}\n"))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn run_init_command(state: &AppState, args: &[&str]) -> Result<(), String> {
    let path = resolve_init_file(state).await?;
    if args.iter().any(|arg| matches!(*arg, "start" | "restart")) {
        select_mihomo(&path).await?;
    }
    let result = if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(error_log_path())
    {
        Command::new(&path)
            .args(args)
            .stdout(f.try_clone().unwrap())
            .stderr(f)
            .status()
            .await
    } else {
        Command::new(&path).args(args).status().await
    };
    if let Err(e) = result {
        *state.init_file.write().unwrap() = None;
        return Err(format!("{}: {}", path, e));
    }
    Ok(())
}

fn get_core_info() -> CoreInfo {
    CoreInfo { name: "mihomo".into() }
}

pub fn get_pid(name: &str) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return vec![];
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let pid = path.file_name()?.to_str()?.parse::<i32>().ok()?;
            let comm = std::fs::read_to_string(path.join("comm")).ok()?;
            (comm.trim_end_matches('\n') == name).then_some(pid)
        })
        .collect()
}

/// The port and secret of Mihomo's controller, from the top level of its config.
///
/// Read line by line on purpose: the keys are top-level scalars, and a config
/// that does not parse as YAML is exactly when a restart is asked for.
pub fn parse_controller(config: &str) -> Option<(u16, Option<String>)> {
    fn value<'a>(config: &'a str, key: &str) -> Option<&'a str> {
        config.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?.strip_prefix(':')?;
            let rest = rest.trim();
            let quoted = rest.starts_with('"') || rest.starts_with('\'');
            let rest = if quoted { rest } else { rest.split(" #").next().unwrap_or(rest).trim() };
            let rest = rest.trim_matches(&['"', '\''][..]);
            (!rest.is_empty()).then_some(rest)
        })
    }
    let port = value(config, "external-controller")?.rsplit(':').next()?.parse().ok()?;
    Some((port, value(config, "secret").map(str::to_string)))
}

async fn controller_request(
    client: &reqwest::Client, method: reqwest::Method, port: u16, secret: Option<&str>, path: &str,
) -> Result<(), String> {
    let mut request = client
        .request(method, format!("http://127.0.0.1:{port}{path}"))
        .timeout(std::time::Duration::from_secs(3));
    if let Some(secret) = secret {
        request = request.bearer_auth(secret);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{path}: {}", response.status()));
    }
    Ok(())
}

/// Ask Mihomo to restart itself (POST /restart) and wait until it answers again.
///
/// Mihomo shuts its listeners down and re-execs its own binary: the same PID,
/// environment (GOMEMLIMIT, SSL_CERT_FILE), limits and group XKeen started it
/// with, and no moment where XKeen's netfilter hook sees the core missing.
async fn api_restart(client: &reqwest::Client) -> Result<(), String> {
    let config = fs::read_to_string(format!("{MIHOMO_CONF_DIR}/config.yaml"))
        .await
        .map_err(|e| format!("конфиг Mihomo не прочитан: {e}"))?;
    let (port, secret) = parse_controller(&config).ok_or("в конфиге Mihomo нет external-controller")?;
    let secret = secret.as_deref();

    controller_request(client, reqwest::Method::POST, port, secret, "/restart").await?;

    // The answer is sent before the re-exec, so give that a moment, then wait
    // for the controller of the new process.
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    for _ in 0..40 {
        if get_pid("mihomo").is_empty() {
            return Err("Mihomo завершился при перезапуске".into());
        }
        if controller_request(client, reqwest::Method::GET, port, secret, "/version").await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err("Mihomo не ответил после перезапуска".into())
}

/// Restart the Mihomo process without rebuilding the firewall rules.
///
/// Through Mihomo's own API first. Killing the process and starting a new one
/// from here raced the old one for its ports (Mihomo keeps running without a
/// listener it could not bind), raced XKeen's hook that respawns a missing
/// core, and started it without the environment XKeen gives it. When the API
/// cannot do it, XKeen restarts everything the way it does at boot.
pub async fn soft_restart(state: &AppState, core: &str) -> Result<(), String> {
    if core != "mihomo" {
        return Err("Поддерживается только ядро Mihomo".into());
    }
    match api_restart(&state.http_client).await {
        Ok(()) => Ok(()),
        Err(e) => {
            log("WARN", format!("Перезапуск Mihomo через API не удался ({e}), полный перезапуск XKeen"));
            full_restart(state).await
        }
    }
}

/// Restart through XKeen's init script: stops the core, starts it with XKeen's
/// environment and rebuilds the firewall rules.
pub async fn full_restart(state: &AppState) -> Result<(), String> {
    run_init_command(state, &["restart", "on"]).await
}

pub async fn get_control(State(state): State<AppState>) -> impl IntoResponse {
    let mihomo_exists = tokio::fs::metadata("/opt/sbin/mihomo").await.is_ok();
    let mihomo_running = mihomo_exists
        && tokio::task::spawn_blocking(|| !get_pid("mihomo").is_empty())
            .await
            .unwrap_or(false);
    let available_cores = if mihomo_exists {
        vec!["mihomo".to_string()]
    } else {
        Vec::new()
    };
    *state.core.write().unwrap() = get_core_info();

    Json(
        serde_json::json!({ "success": true, "cores": available_cores, "currentCore": "mihomo", "running": mihomo_running }),
    )
}

async fn check_core_config() -> Result<(), String> {
    fs::create_dir_all(MIHOMO_CONF_DIR).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn post_control(State(state): State<AppState>, Json(req): Json<ControlReq>) -> impl IntoResponse {
    match req.action.as_str() {
        "switchCore" => {
            if req.core == "mihomo" {
                return Json(ApiResponse {
                    success: true,
                    error: None,
                    data: None,
                });
            }
            return Json(ApiResponse {
                success: false,
                error: Some("Поддерживается только ядро Mihomo".into()),
                data: None,
            });
        }
        "softRestart" => {
            if let Err(e) = soft_restart(&state, &req.core).await {
                return Json(ApiResponse {
                    success: false,
                    error: Some(e),
                    data: None,
                });
            }
        }
        a if ["start", "stop", "hardRestart"].contains(&a) => {
            let arg = match a {
                "start" => "start",
                "stop" => "stop",
                _ => "restart",
            };

            if a == "start" || a == "hardRestart" {
                if let Err(e) = check_core_config().await {
                    log("ERROR", e);
                    return Json(ApiResponse {
                        success: false,
                        error: Some("Не удалось запустить Mihomo".into()),
                        data: None,
                    });
                }
            }
            if a == "start" || a == "hardRestart" {
                _ = fs::write(error_log_path(), b"").await;
            }

            let args: &[&str] = match a {
                "start" => &["start", "on"],
                "hardRestart" => &["restart", "on"],
                _ => &[arg],
            };

            if let Err(e) = run_init_command(&state, args).await {
                return Json(ApiResponse {
                    success: false,
                    error: Some(e),
                    data: None,
                });
            }
        }
        _ => {
            return Json(ApiResponse {
                success: false,
                error: Some("Bad action".into()),
                data: None,
            });
        }
    }
    Json(ApiResponse::<()> {
        success: true,
        error: None,
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_controller;

    #[test]
    fn reads_port_and_secret_from_the_router_config() {
        let config = "log-level: silent\nexternal-controller: 0.0.0.0:9066\nsecret: 'abc123'\ndns:\n  listen: 0.0.0.0:1053\n";
        assert_eq!(parse_controller(config), Some((9066, Some("abc123".into()))));
    }

    #[test]
    fn a_config_without_a_secret_still_has_a_port() {
        assert_eq!(parse_controller("external-controller: 127.0.0.1:9090 # ui\n"), Some((9090, None)));
    }

    #[test]
    fn nested_and_commented_keys_do_not_count() {
        let config = "# external-controller: 0.0.0.0:1\nfoo:\n  external-controller: 0.0.0.0:2\n  secret: x\n";
        assert_eq!(parse_controller(config), None);
    }

    #[test]
    fn external_controller_tls_is_not_the_controller() {
        let config = "external-controller-tls: 0.0.0.0:9443\nexternal-controller: \"[::]:9066\"\n";
        assert_eq!(parse_controller(config), Some((9066, None)));
    }
}
