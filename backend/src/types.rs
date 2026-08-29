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

/// Our own GitHub mirrors, tried before the public ones.
///
/// A router on a Russian ISP often cannot reach GitHub at all, and the public
/// gh-proxy services are themselves reachable only sometimes. These run on our
/// nodes, which can.
pub const DHQ_GITHUB_MIRRORS: &[&str] = &["https://141.105.68.132.sslip.io"];

/// The public services, kept as a fallback: if our mirror is down, updates
/// should still work rather than stop everywhere at once.
pub const PUBLIC_GITHUB_MIRRORS: &[&str] = &["https://gh-proxy.com", "https://ghfast.top"];

pub fn default_github_proxy() -> Vec<String> {
    DHQ_GITHUB_MIRRORS
        .iter()
        .chain(PUBLIC_GITHUB_MIRRORS)
        .map(|s| s.to_string())
        .collect()
}

impl Default for UpdaterSettings {
    fn default() -> Self {
        Self {
            github_proxy: default_github_proxy(),
            backup_core: true,
            auto_check_ui: true,
            auto_check_core: true,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterConfigSettings {
    pub url: String,
    pub auto_update: bool,
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
    pub router_config: RouterConfigSettings,
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
            router_config: RouterConfigSettings,
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
            router_config: raw.router_config,
            log: raw.log,
            clash_api: raw.clash_api,
            append_config_paths: raw.append_config_paths,
            auth: raw.auth,
        })
    }
}

impl AppSettings {
    pub fn normalize_proxies(&mut self) {
        let mut proxies: Vec<String> = self
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

        // A router installed before the mirrors existed keeps its saved list for
        // good, so a new default alone would reach nobody who already has the UI.
        // Upgrade the untouched list in place — but only that one: a list the
        // user has edited is their answer, including if what they removed was
        // our mirror.
        if proxies.iter().map(String::as_str).eq(PUBLIC_GITHUB_MIRRORS.iter().copied()) {
            proxies = default_github_proxy();
        }
        self.updater.github_proxy = proxies;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(proxies: &[&str]) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.updater.github_proxy = proxies.iter().map(|p| p.to_string()).collect();
        settings
    }

    #[test]
    fn our_mirrors_come_before_the_public_ones() {
        // The list is tried top down, so order is the whole feature.
        let proxies = default_github_proxy();
        let ours = proxies.iter().position(|p| p == DHQ_GITHUB_MIRRORS[0]);
        let public = proxies.iter().position(|p| p == PUBLIC_GITHUB_MIRRORS[0]);
        assert!(ours < public, "{proxies:?}");
        for mirror in PUBLIC_GITHUB_MIRRORS {
            assert!(proxies.iter().any(|p| p == mirror), "public fallback dropped");
        }
    }

    #[test]
    fn an_install_from_before_the_mirrors_is_upgraded() {
        // A router that already has the UI keeps its saved list forever, so a
        // new default alone would never reach it.
        let mut settings = settings_with(PUBLIC_GITHUB_MIRRORS);
        settings.normalize_proxies();
        assert_eq!(settings.updater.github_proxy, default_github_proxy());
    }

    #[test]
    fn a_list_the_user_edited_is_left_alone() {
        // Including when what they removed is our mirror: that is an answer.
        let mut settings = settings_with(&["https://gh-proxy.com"]);
        settings.normalize_proxies();
        assert_eq!(settings.updater.github_proxy, vec!["https://gh-proxy.com"]);

        let mut settings = settings_with(&["https://mine.example"]);
        settings.normalize_proxies();
        assert_eq!(settings.updater.github_proxy, vec!["https://mine.example"]);
    }

    #[test]
    fn a_bare_host_still_gets_a_scheme() {
        let mut settings = settings_with(&["mirror.example", "://other.example"]);
        settings.normalize_proxies();
        assert_eq!(
            settings.updater.github_proxy,
            vec!["https://mirror.example", "https://other.example"]
        );
    }
}
