use crate::logger::log;
use crate::types::*;
use axum::extract::State;
use axum::response::{IntoResponse, Json};
use nix::sys::resource::{Resource, setrlimit};
use nix::sys::signal::{Signal, kill};
use nix::unistd::{Gid, Pid, setgid, setsid};
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

pub async fn soft_restart(core: &str) -> Result<(), String> {
    if core != "mihomo" {
        return Err("Поддерживается только ядро Mihomo".into());
    }

    for pid in get_pid("mihomo") {
        _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
    }

    let mut cmd = Command::new("mihomo");
    cmd.env("CLASH_HOME_DIR", MIHOMO_CONF_DIR);

    let lim = if cfg!(target_arch = "aarch64") { 40000 } else { 10000 };
    unsafe {
        cmd.pre_exec(move || {
            setsid()?;
            setgid(Gid::from_raw(11111))?;
            setrlimit(Resource::RLIMIT_NOFILE, lim, lim)?;
            Ok(())
        });
    }

    if let Ok(f) = std::fs::File::options()
        .append(true)
        .create(true)
        .open(error_log_path())
    {
        cmd.stdout(f.try_clone().unwrap()).stderr(f);
    }

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    match child.try_wait() {
        Ok(Some(status)) if !status.success() => {
            return Err(format!("Не удалось перезапустить {}: {}", core, status));
        }
        _ => {
            tokio::spawn(async move {
                _ = child.wait().await;
            });
        }
    }

    Ok(())
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
            if let Err(e) = soft_restart(&req.core).await {
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
