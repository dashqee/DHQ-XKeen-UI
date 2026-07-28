use crate::types::RouterConfigSettings;
use reqwest::Url;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs;
use tokio::process::Command;

const AUTO_UPDATE_SCRIPT: &str = "/opt/etc/mihomo/mihomo_config_auto_update_local.sh";
const CRON_INIT: &str = "/opt/etc/init.d/S10cron";
const CRONTAB_BIN: &str = "/opt/bin/crontab";
const CRONTAB_DIR: &str = "/opt/var/spool/cron/crontabs";
const CRONTAB_FILE: &str = "/opt/var/spool/cron/crontabs/root";
const CRONTAB_MARKER: &str = "# DHQClash Router config auto-update";
const CRONTAB_SCHEDULE: &str = "0 3 * * * /opt/etc/mihomo/mihomo_config_auto_update_local.sh";
const OPKG_BIN: &str = "/opt/bin/opkg";

pub fn validate_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    let parsed = Url::parse(value).map_err(|_| "Укажите корректную ссылку на .yaml-конфиг".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.path().to_ascii_lowercase().ends_with(".yaml") {
        return Err("Ссылка должна использовать HTTP(S) и вести на файл .yaml".into());
    }
    Ok(value.to_string())
}

pub async fn apply(previous: &RouterConfigSettings, next: &RouterConfigSettings) -> Result<(), String> {
    let url_changed = previous.url != next.url;
    let schedule_changed = previous.auto_update != next.auto_update;

    if next.url.is_empty() {
        if next.auto_update {
            return Err("Сначала укажите ссылку на .yaml-конфиг".into());
        }
        if schedule_changed {
            configure_cron(false).await?;
        }
        return Ok(());
    }

    let url = validate_url(&next.url)?;
    if url_changed || !Path::new(AUTO_UPDATE_SCRIPT).exists() {
        install_script(&url).await?;
    }

    if url_changed || (schedule_changed && next.auto_update) {
        run_script().await?;
    }

    if url_changed || schedule_changed {
        configure_cron(next.auto_update).await?;
    }
    Ok(())
}

async fn install_script(url: &str) -> Result<(), String> {
    let script_path = Path::new(AUTO_UPDATE_SCRIPT);
    let parent = script_path
        .parent()
        .ok_or_else(|| "Некорректный путь скрипта автообновления".to_string())?;
    fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;

    let temporary = format!("{AUTO_UPDATE_SCRIPT}.tmp");
    fs::write(&temporary, render_script(url))
        .await
        .map_err(|e| e.to_string())?;
    fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
        .await
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, AUTO_UPDATE_SCRIPT)
        .await
        .map_err(|e| e.to_string())
}

async fn run_script() -> Result<(), String> {
    run_command("/bin/sh", &[AUTO_UPDATE_SCRIPT]).await
}

async fn configure_cron(enabled: bool) -> Result<(), String> {
    if enabled && !Path::new(CRONTAB_BIN).exists() {
        run_command(OPKG_BIN, &["install", "cron"]).await?;
    }

    fs::create_dir_all(CRONTAB_DIR).await.map_err(|e| e.to_string())?;
    let current = fs::read_to_string(CRONTAB_FILE).await.unwrap_or_default();
    let updated = update_crontab(&current, enabled);
    if updated != current {
        let temporary = format!("{CRONTAB_FILE}.tmp");
        fs::write(&temporary, updated).await.map_err(|e| e.to_string())?;
        fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, CRONTAB_FILE).await.map_err(|e| e.to_string())?;
    }

    if !Path::new(CRON_INIT).exists() {
        return if enabled {
            Err("После установки cron не найден /opt/etc/init.d/S10cron".into())
        } else {
            Ok(())
        };
    }

    if enabled {
        run_command(CRON_INIT, &["enable"]).await?;
        if run_command(CRON_INIT, &["restart"]).await.is_err() {
            run_command(CRON_INIT, &["start"]).await?;
        }
    } else {
        run_command(CRON_INIT, &["restart"]).await?;
    }
    Ok(())
}

async fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        return Ok(());
    }

    let mut details = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if details.is_empty() {
        details = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }
    Err(if details.is_empty() {
        format!("{program} завершился с ошибкой")
    } else {
        format!("{program}: {details}")
    })
}

fn update_crontab(content: &str, enabled: bool) -> String {
    let mut lines = content
        .lines()
        .filter(|line| line.trim() != CRONTAB_MARKER && !line.contains(AUTO_UPDATE_SCRIPT))
        .map(str::to_string)
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if enabled {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(CRONTAB_MARKER.into());
        lines.push(CRONTAB_SCHEDULE.into());
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn render_script(url: &str) -> String {
    let escaped_url = url.replace('\'', "'\"'\"'");
    format!(
        r#"#!/bin/sh
set -u

CONFIG_URL='{escaped_url}'
CONFIG_DIR='/opt/etc/mihomo'
CONFIG_LINK="$CONFIG_DIR/config.yaml"
LOG_FILE="$CONFIG_DIR/update.log"
XKEEN_BIN='/opt/sbin/xkeen'

log() {{
    echo "$(date): $1" >> "$LOG_FILE"
}}

REAL_CONFIG="$(readlink -f "$CONFIG_LINK")"
if [ -z "$REAL_CONFIG" ] || [ ! -f "$REAL_CONFIG" ]; then
    log "не удалось определить файл конфигурации $CONFIG_LINK"
    exit 1
fi

TMP_FILE="$REAL_CONFIG.new"
BACKUP_FILE="$REAL_CONFIG.bak-$(date +%Y%m%d-%H%M%S)"
trap 'rm -f "$TMP_FILE"' EXIT HUP INT TERM

if ! curl -fsSL --connect-timeout 15 --max-time 90 -o "$TMP_FILE" "$CONFIG_URL"; then
    log 'download failed'
    exit 1
fi

cp "$REAL_CONFIG" "$BACKUP_FILE"
cp "$TMP_FILE" "$REAL_CONFIG"
if ! "$XKEEN_BIN" -mtest; then
    log 'invalid config, rolling back'
    cp "$BACKUP_FILE" "$REAL_CONFIG"
    exit 1
fi

rm -f "$TMP_FILE"
trap - EXIT HUP INT TERM
"$XKEEN_BIN" -restart
log "config updated ($REAL_CONFIG) and applied"
ls -t "$REAL_CONFIG".bak-* 2>/dev/null | tail -n +11 | xargs -r rm -f
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_yaml_http_urls() {
        assert_eq!(
            validate_url(" https://example.com/client/config.yaml?token=secret ").unwrap(),
            "https://example.com/client/config.yaml?token=secret"
        );
        assert!(validate_url("file:///tmp/config.yaml").is_err());
        assert!(validate_url("https://example.com/config.yml").is_err());
        assert!(validate_url("https://example.com/config.yaml\nbad").is_err());
    }

    #[test]
    fn crontab_update_preserves_unrelated_jobs_and_is_idempotent() {
        let original = "0 6 * * * /opt/sbin/xkeen -ug\n";
        let enabled = update_crontab(original, true);
        assert!(enabled.contains(original.trim()));
        assert!(enabled.contains(CRONTAB_MARKER));
        assert_eq!(update_crontab(&enabled, true), enabled);
        assert_eq!(update_crontab(&enabled, false), original);
    }

    #[test]
    fn script_quotes_subscription_url() {
        let script = render_script("https://example.com/config.yaml?token=a'b");
        assert!(script.contains("CONFIG_URL='https://example.com/config.yaml?token=a'\"'\"'b'"));
    }
}
