//! Ship XKeen's routing lists with the panel, and put them in place when the
//! router has none.
//!
//! A fresh XKeen install leaves these empty or absent, and without them the
//! defaults are wrong in a way that is hard to diagnose: Steam and Yandex go
//! through the tunnel, and only the handful of ports XKeen guesses gets
//! redirected at all.

use std::path::Path;
use tokio::fs;

use crate::logger::log;
use crate::types::XKEEN_CONF_DIR;

/// The lists, embedded at build time so putting them in place needs no network
/// and cannot half-succeed.
const DEFAULTS: &[(&str, &str)] = &[
    ("ip_exclude.lst", include_str!("../assets/xkeen/ip_exclude.lst")),
    ("port_exclude.lst", include_str!("../assets/xkeen/port_exclude.lst")),
    ("port_proxying.lst", include_str!("../assets/xkeen/port_proxying.lst")),
];

/// Write any list the router is missing. Returns the names it created.
///
/// Only missing files: these are meant to be edited, and a file the user
/// tailored must survive every update.
pub async fn ensure_defaults() -> Result<Vec<&'static str>, String> {
    fs::create_dir_all(XKEEN_CONF_DIR)
        .await
        .map_err(|e| format!("{XKEEN_CONF_DIR}: {e}"))?;

    let mut created = Vec::new();
    for (name, content) in DEFAULTS {
        let path = Path::new(XKEEN_CONF_DIR).join(name);
        if !should_write(&path, content).await {
            continue;
        }
        // Written next to the target and renamed, so a panel killed mid-write
        // cannot leave XKeen a truncated list.
        let temporary = path.with_extension("lst.tmp");
        fs::write(&temporary, content)
            .await
            .map_err(|e| format!("{}: {e}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .await
            .map_err(|e| format!("{}: {e}", path.display()))?;
        created.push(*name);
    }
    Ok(created)
}

/// Whether the shipped list should replace what is on disk.
///
/// A file that is absent, obviously. Also one that exists but holds nothing but
/// comments — that is the placeholder XKeen's own installer leaves, and an
/// empty port_proxying.lst redirects nothing at all.
///
/// But only when the default has rules to put there. port_exclude.lst is
/// deliberately comments-only, and without this it would count as empty against
/// itself and be rewritten, and logged, on every single start.
async fn should_write(path: &Path, default: &str) -> bool {
    match fs::read_to_string(path).await {
        Ok(text) => !has_rules(&text) && has_rules(default),
        Err(_) => true,
    }
}

fn has_rules(text: &str) -> bool {
    text.lines()
        .any(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
}

/// Put the lists in place and say so in the log, never failing the caller: a
/// missing list degrades routing, it does not break an update that otherwise
/// succeeded.
pub async fn ensure_defaults_logged() {
    match ensure_defaults().await {
        Ok(created) if created.is_empty() => {}
        Ok(created) => log(
            "INFO",
            format!("Установлены конфигурации XKeen по умолчанию: {}", created.join(", ")),
        ),
        Err(e) => log("WARN", format!("Не удалось установить конфигурации XKeen: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lists_that_should_carry_rules_do() {
        for (name, content) in DEFAULTS {
            if *name == "port_exclude.lst" {
                // Deliberately empty: XKeen gives port_proxying.lst priority, and
                // setting both is a misconfiguration its own comment warns about.
                assert!(!has_rules(content), "{name} must stay empty");
            } else {
                assert!(has_rules(content), "{name} has no rules");
            }
        }
    }

    #[tokio::test]
    async fn a_comments_only_default_is_not_rewritten_over_itself() {
        // port_exclude.lst ships as comments only. Judging it by "has no rules"
        // alone makes it count as empty against its own content, so every start
        // would rewrite it and log that it had installed it.
        let dir = std::env::temp_dir().join(format!("xkeen-defaults-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("port_exclude.lst");
        let default = DEFAULTS.iter().find(|(name, _)| *name == "port_exclude.lst").unwrap().1;

        assert!(should_write(&path, default).await, "absent file must be written");
        std::fs::write(&path, default).unwrap();
        assert!(!should_write(&path, default).await, "would rewrite on every start");
        _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_edited_list_survives_but_a_placeholder_does_not() {
        let dir = std::env::temp_dir().join(format!("xkeen-edited-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("port_proxying.lst");
        let default = DEFAULTS
            .iter()
            .find(|(name, _)| *name == "port_proxying.lst")
            .unwrap()
            .1;

        std::fs::write(&path, "# only comments\n\n").unwrap();
        assert!(
            should_write(&path, default).await,
            "XKeen's placeholder must be replaced"
        );

        std::fs::write(&path, "8443\n").unwrap();
        assert!(!should_write(&path, default).await, "an edited list must survive");
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_proxying_ports_cover_web_and_the_usual_game_and_voice_ranges() {
        let ports = DEFAULTS
            .iter()
            .find(|(name, _)| *name == "port_proxying.lst")
            .expect("port_proxying.lst")
            .1;
        for expected in ["80", "443", "3478", "50000:50030"] {
            assert!(
                ports.lines().any(|line| line.trim() == expected),
                "port {expected} missing"
            );
        }
    }
}
