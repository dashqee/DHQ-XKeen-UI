use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};
use tokio::task::AbortHandle;

pub const APP_CONFIG: &str = "/opt/etc/xkeen/xkeen-ui.json";
pub const APP_CONFIG_LEGACY: &str = "/opt/share/www/XKeen-UI/config.json";
pub const MIHOMO_LOG: &str = "/opt/var/log/mihomo.log";
pub const MIHOMO_CONF_DIR: &str = "/opt/etc/mihomo";
pub const S99XKEEN: &str = "/opt/etc/init.d/S99xkeen";
pub const S99XKEEN_UI: &str = "/opt/etc/init.d/S99xkeen-ui";
pub const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));
pub const XKEEN_CONF_DIR: &str = "/opt/etc/xkeen";
pub const XKEEN_CONF: &str = "/opt/etc/xkeen/xkeen.json";
pub const XKEEN_UI_LOG: &str = "/opt/var/log/xkeen-ui.log";
pub fn error_log_path() -> String {
    MIHOMO_LOG.into()
}

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<RwLock<CoreInfo>>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub init_file: Arc<RwLock<Option<String>>>,
    pub http_client: reqwest::Client,
    pub update_checker: UpdateChecker,
    pub log_tx: Arc<broadcast::Sender<String>>,
    pub log_watcher: Arc<Mutex<Option<AbortHandle>>>,
    pub app_config_lock: Arc<Mutex<()>>,
    pub debug: bool,
    pub rci_token: Option<String>,
}

#[derive(Clone, Default)]
pub struct UpdateChecker {
    pub ui_outdated: Arc<RwLock<bool>>,
    pub core_outdated: Arc<RwLock<bool>>,
    pub last_ui_check: Arc<RwLock<Option<Instant>>>,
    pub last_core_check: Arc<RwLock<Option<Instant>>>,
    pub last_ui_toast: Arc<RwLock<Option<Instant>>>,
    pub last_core_toast: Arc<RwLock<Option<Instant>>>,
    pub ui_latest_tag: Arc<RwLock<Option<String>>>,
    pub core_latest_tag: Arc<RwLock<Option<String>>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CoreInfo {
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdaterSettings {
    pub auto_check_ui: bool,
    pub auto_check_core: bool,
    pub backup_core: bool,
    pub github_proxy: Vec<String>,
}

impl Default for UpdaterSettings {
    fn default() -> Self {
        Self {
            github_proxy: vec!["https://gh-proxy.com".into(), "https://ghfast.top".into()],
            backup_core: true,
            auto_check_ui: true,
            auto_check_core: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogSettings {
    pub timezone: i32,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self { timezone: 3 }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClashApiSettings {
    pub ping_url: String,
    pub ping_timeout: u32,
    pub show_source_name: bool,
    pub hide_unavailable_proxies: bool,
    pub hide_unavailable_proxies_counter: u32,
    pub proxy_sort_order: String,
}

impl Default for ClashApiSettings {
    fn default() -> Self {
        Self {
            ping_url: "https://www.gstatic.com/generate_204".into(),
            ping_timeout: 5000,
            show_source_name: false,
            hide_unavailable_proxies: false,
            hide_unavailable_proxies_counter: 3,
            proxy_sort_order: "default".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthSettings {
    pub enabled: bool,
    pub password_hash: Option<String>,
    pub session_ids: Vec<String>,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            password_hash: None,
            session_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppendConfigPaths {
    pub mihomo: Vec<String>,
}

#[derive(Clone, Serialize, Default)]
pub struct AppSettings {
    pub updater: UpdaterSettings,
    pub log: LogSettings,
    pub clash_api: ClashApiSettings,
    pub append_config_paths: AppendConfigPaths,
    pub auth: AuthSettings,
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawConfig {
            #[serde(default)]
            updater: UpdaterSettings,
            #[serde(default)]
            log: LogSettings,
            #[serde(default)]
            clash_api: ClashApiSettings,
            #[serde(default)]
            append_config_paths: AppendConfigPaths,
            #[serde(default)]
            auth: AuthSettings,
            #[serde(rename = "timezoneOffset")]
            legacy_tz: Option<i32>,
        }
        let mut raw = RawConfig::deserialize(deserializer)?;
        if let Some(tz) = raw.legacy_tz {
            raw.log.timezone = tz;
        }
        Ok(Self {
            updater: raw.updater,
            log: raw.log,
            clash_api: raw.clash_api,
            append_config_paths: raw.append_config_paths,
            auth: raw.auth,
        })
    }
}

impl AppSettings {
    pub fn normalize_proxies(&mut self) {
        self.updater.github_proxy = self
            .updater
            .github_proxy
            .iter()
            .map(|p| {
                if p.starts_with("http") {
                    p.to_string()
                } else {
                    format!("https://{}", p.trim_start_matches("://"))
                }
            })
            .collect();
    }
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub data: Option<T>,
}

#[derive(Deserialize)]
pub struct UpdateReq {
    pub core: String,
    pub version: String,
    pub backup_core: bool,
    #[serde(default)]
    pub assets: Vec<String>,
}
