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
    let value = normalize_pasted_url(value);
    let parsed = Url::parse(&value).map_err(|_| "Укажите корректную ссылку на .yaml-конфиг".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.path().to_ascii_lowercase().ends_with(".yaml") {
        return Err("Ссылка должна использовать HTTP(S) и вести на файл .yaml".into());
    }
    Ok(value)
}

fn normalize_pasted_url(value: &str) -> String {
    let value = value.trim();
    let unwrapped = if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if matches!(
            (first, last),
            (b'"', b'"') | (b'\'', b'\'') | (b'`', b'`') | (b'<', b'>')
        ) {
            value[1..value.len() - 1].trim()
        } else {
            value
        }
    } else {
        value
    };
    unwrapped.replace("&amp;", "&")
}

pub async fn apply(previous: &RouterConfigSettings, next: &RouterConfigSettings) -> Result<(), String> {
    if next.url.is_empty() {
        if next.auto_update {
            return Err("Сначала укажите ссылку на .yaml-конфиг".into());
        }
        configure_cron(false).await?;
        return Ok(());
    }

    let url = validate_url(&next.url)?;
    let script_changed = reconcile_script(&url).await?;
    configure_cron(next.auto_update).await?;
    if script_changed || previous.url.is_empty() {
        run_script().await?;
    }
    Ok(())
}

async fn reconcile_script(url: &str) -> Result<bool, String> {
    let script_path = Path::new(AUTO_UPDATE_SCRIPT);
    let parent = script_path
        .parent()
        .ok_or_else(|| "Некорректный путь скрипта автообновления".to_string())?;
    fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;

    let existing = fs::read_to_string(script_path).await.ok();
    let (content, changed) = reconcile_script_content(existing.as_deref(), url);
    if !changed {
        ensure_executable(script_path).await?;
        return Ok(false);
    }

    let temporary = format!("{AUTO_UPDATE_SCRIPT}.tmp");
    fs::write(&temporary, content).await.map_err(|e| e.to_string())?;
    fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
        .await
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, AUTO_UPDATE_SCRIPT)
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

async fn ensure_executable(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path).await.map_err(|e| e.to_string())?;
    let mode = metadata.permissions().mode();
    if mode & 0o111 == 0 {
        fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o111))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn run_script() -> Result<(), String> {
    run_command("/bin/sh", &[AUTO_UPDATE_SCRIPT]).await
}

async fn configure_cron(enabled: bool) -> Result<(), String> {
    let was_installed = cron_is_installed();
    if enabled && !was_installed {
        run_command(OPKG_BIN, &["install", "cron"]).await?;
    }

    let current = read_crontab().await?;
    let updated = update_crontab(&current, enabled);
    let changed = updated != current;
    if changed {
        write_crontab(&updated).await?;
    }

    if enabled && !cron_is_installed() {
        return Err("После установки не найдены Entware cron и /opt/etc/init.d/S10cron".into());
    }
    if !Path::new(CRON_INIT).exists() {
        return if enabled {
            Err("Не найден /opt/etc/init.d/S10cron".into())
        } else {
            Ok(())
        };
    }

    if enabled && !was_installed {
        // Some Entware builds return a non-zero code when the service is already
        // enabled. The crontab is installed at this point, so do not turn an
        // otherwise successful first save into a URL validation error.
        let _ = run_command(CRON_INIT, &["enable"]).await;
        if run_command(CRON_INIT, &["restart"]).await.is_err() {
            run_command(CRON_INIT, &["start"]).await?;
        }
    } else if changed {
        if run_command(CRON_INIT, &["restart"]).await.is_err() && enabled {
            run_command(CRON_INIT, &["start"]).await?;
        }
    } else if enabled && run_command(CRON_INIT, &["status"]).await.is_err() {
        run_command(CRON_INIT, &["start"]).await?;
    }
    Ok(())
}

fn cron_is_installed() -> bool {
    Path::new(CRONTAB_BIN).exists() && Path::new(CRON_INIT).exists()
}

async fn read_crontab() -> Result<String, String> {
    if Path::new(CRONTAB_FILE).exists() {
        return fs::read_to_string(CRONTAB_FILE).await.map_err(|e| e.to_string());
    }
    if !Path::new(CRONTAB_BIN).exists() {
        return Ok(String::new());
    }
    Ok(Command::new(CRONTAB_BIN)
        .arg("-l")
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default())
}

async fn write_crontab(content: &str) -> Result<(), String> {
    fs::create_dir_all(CRONTAB_DIR).await.map_err(|e| e.to_string())?;
    let temporary = format!("{CRONTAB_FILE}.tmp");
    fs::write(&temporary, content).await.map_err(|e| e.to_string())?;
    fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, CRONTAB_FILE).await.map_err(|e| e.to_string())
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
    let managed_entries = content.lines().filter(|line| is_managed_cron_line(line)).count();
    let has_marker = content.lines().any(|line| line.trim() == CRONTAB_MARKER);
    let has_exact_schedule = content.lines().any(|line| line.trim() == CRONTAB_SCHEDULE);
    if (enabled && managed_entries == 1 && has_marker && has_exact_schedule)
        || (!enabled && managed_entries == 0 && !has_marker)
    {
        return content.to_string();
    }

    let mut lines = content
        .lines()
        .filter(|line| line.trim() != CRONTAB_MARKER && !is_managed_cron_line(line))
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

fn is_managed_cron_line(line: &str) -> bool {
    let line = line.trim();
    !line.starts_with('#') && line.contains(AUTO_UPDATE_SCRIPT)
}

fn reconcile_script_content(existing: Option<&str>, url: &str) -> (String, bool) {
    let Some(existing) = existing else {
        return (render_script(url), true);
    };
    if extract_config_url(existing).as_deref() == Some(url) {
        return (existing.to_string(), false);
    }

    let assignment = config_url_assignment(url);
    let mut found = false;
    let mut lines = Vec::new();
    for line in existing.lines() {
        if line.trim_start().starts_with("CONFIG_URL=") {
            if !found {
                lines.push(assignment.clone());
                found = true;
            }
        } else {
            lines.push(line.to_string());
        }
    }
    if !found {
        return (render_script(url), true);
    }
    (format!("{}\n", lines.join("\n")), true)
}

fn extract_config_url(content: &str) -> Option<String> {
    let value = content
        .lines()
        .find_map(|line| line.trim().strip_prefix("CONFIG_URL="))?
        .trim();
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if matches!((first, last), (b'"', b'"') | (b'\'', b'\'')) {
            let unquoted = &value[1..value.len() - 1];
            return Some(if first == b'\'' {
                unquoted.replace("'\"'\"'", "'")
            } else {
                unquoted.to_string()
            });
        }
    }
    Some(value.to_string())
}

fn config_url_assignment(url: &str) -> String {
    let escaped_url = url.replace('\'', "'\"'\"'");
    format!("CONFIG_URL='{escaped_url}'")
}

fn render_script(url: &str) -> String {
    let assignment = config_url_assignment(url);
    format!(
        r#"#!/bin/sh
set -u

{assignment}
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
        assert_eq!(
            validate_url("`https://example.com/config.yaml`").unwrap(),
            "https://example.com/config.yaml"
        );
        assert_eq!(
            validate_url("https://example.com/config.yaml?a=1&amp;b=2").unwrap(),
            "https://example.com/config.yaml?a=1&b=2"
        );
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
    fn crontab_reconcile_deduplicates_only_managed_entries() {
        let original = concat!(
            "MAILTO=root\n",
            "0 6 * * * /opt/sbin/xkeen -ug\n",
            "0 3 * * * /opt/etc/mihomo/mihomo_config_auto_update_local.sh\n",
            "15 4 * * * /opt/etc/mihomo/mihomo_config_auto_update_local.sh >> /tmp/update.log 2>&1\n",
            "# Keep docs for /opt/etc/mihomo/mihomo_config_auto_update_local.sh\n",
            "30 5 * * * /opt/bin/backup\n",
        );
        let updated = update_crontab(original, true);
        assert!(updated.contains("MAILTO=root"));
        assert!(updated.contains("0 6 * * * /opt/sbin/xkeen -ug"));
        assert!(updated.contains("30 5 * * * /opt/bin/backup"));
        assert!(updated.contains("# Keep docs for /opt/etc/mihomo/mihomo_config_auto_update_local.sh"));
        assert_eq!(updated.lines().filter(|line| is_managed_cron_line(line)).count(), 1);
        assert_eq!(update_crontab(&updated, true), updated);
    }

    #[test]
    fn script_reconcile_keeps_existing_script_when_url_matches() {
        let existing = "#!/bin/sh\nCONFIG_URL=\"https://example.com/config.yaml\"\necho ok\n";
        assert_eq!(
            reconcile_script_content(Some(existing), "https://example.com/config.yaml"),
            (existing.to_string(), false)
        );
    }

    #[test]
    fn script_reconcile_replaces_only_url_and_removes_duplicates() {
        let existing = concat!(
            "#!/bin/sh\n",
            "CONFIG_URL=\"https://old.example/config.yaml\"\n",
            "echo keep-me\n",
            "CONFIG_URL='https://duplicate.example/config.yaml'\n",
        );
        let (updated, changed) = reconcile_script_content(Some(existing), "https://new.example/config.yaml");
        assert!(changed);
        assert!(updated.contains("echo keep-me"));
        assert!(updated.contains("https://new.example/config.yaml"));
        assert!(!updated.contains("old.example"));
        assert!(!updated.contains("duplicate.example"));
        assert_eq!(updated.matches("CONFIG_URL=").count(), 1);
    }

    #[test]
    fn script_quotes_subscription_url() {
        let url = "https://example.com/config.yaml?token=a'b";
        let script = render_script(url);
        assert!(script.contains("CONFIG_URL='https://example.com/config.yaml?token=a'\"'\"'b'"));
        assert_eq!(reconcile_script_content(Some(&script), url), (script.clone(), false));
    }
}
