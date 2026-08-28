use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::SystemTime;
use tokio::process::Command;
use yaml_rust2::{Yaml, YamlLoader};

use crate::types::{ApiResponse, AppState, MIHOMO_CONF_DIR};

const MIHOMO_CONF_DIRIG_PATH: &str = "/opt/etc/mihomo/config.yaml";

static MIHOMO_YAML_CACHE: LazyLock<RwLock<Option<(SystemTime, Arc<Vec<Yaml>>)>>> = LazyLock::new(|| RwLock::new(None));

async fn load_mihomo_yaml() -> Result<Arc<Vec<Yaml>>, String> {
    let mtime = tokio::fs::metadata(MIHOMO_CONF_DIRIG_PATH)
        .await
        .map_err(|e| format!("Ошибка чтения конфига: {e}"))?
        .modified()
        .map_err(|e| format!("Ошибка чтения mtime: {e}"))?;

    if let Some(cached) = {
        let guard = MIHOMO_YAML_CACHE.read().unwrap();
        guard
            .as_ref()
            .filter(|(ts, _)| *ts == mtime)
            .map(|(_, docs)| docs.clone())
    } {
        return Ok(cached);
    }

    let content = tokio::fs::read_to_string(MIHOMO_CONF_DIRIG_PATH)
        .await
        .map_err(|e| format!("Ошибка чтения конфига: {e}"))?;
    let docs = YamlLoader::load_from_str(&content).map_err(|e| format!("Ошибка парсинга YAML: {e}"))?;
    let arc = Arc::new(docs);
    *MIHOMO_YAML_CACHE.write().unwrap() = Some((mtime, arc.clone()));
    Ok(arc)
}

/// Resolved rule-set text, keyed by provider name and the mtime of the file it
/// came from. Without it a search across N providers re-runs `mihomo
/// convert-ruleset` N times per keystroke, and that is a router CPU.
static CONTENT_CACHE: LazyLock<RwLock<HashMap<String, (Option<SystemTime>, Arc<String>)>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The text of one rule-provider, whatever shape it is stored in: an inline
/// payload in the config, a plain file on disk, or a binary MRS set that only
/// mihomo itself can decode.
async fn provider_content(
    name: &str, provider: &Yaml, format: Option<&str>, behavior: Option<&str>, vehicle_type: Option<&str>,
) -> Result<Arc<String>, String> {
    if vehicle_type.is_some_and(|v| v.eq_ignore_ascii_case("inline")) {
        let items: Vec<&str> = provider["payload"]
            .as_vec()
            .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        if items.is_empty() {
            return Err("Payload пуст или не найден".into());
        }
        // Inline payloads live in the config, whose own mtime already
        // invalidates the parsed-YAML cache above, so they need no file stamp.
        return Ok(Arc::new(items.join("\n")));
    }

    let final_path = match provider["path"].as_str() {
        Some(p) => resolve_provider_path(p),
        None => match provider["url"].as_str() {
            Some(u) => format!("{}/rules/{:x}", MIHOMO_CONF_DIR, md5::compute(u)),
            None => return Err("В провайдере нет ни path, ни url".into()),
        },
    };

    let mtime = tokio::fs::metadata(&final_path)
        .await
        .ok()
        .and_then(|m| m.modified().ok());

    if let Some(hit) = {
        let guard = CONTENT_CACHE.read().unwrap();
        guard
            .get(name)
            .filter(|(stamp, _)| *stamp == mtime && stamp.is_some())
            .map(|(_, text)| text.clone())
    } {
        return Ok(hit);
    }

    let is_mrs = format.is_some_and(|f| f.eq_ignore_ascii_case("mrs") || f.eq_ignore_ascii_case("mrsrule"));
    let content = if is_mrs {
        convert_mrs(&final_path, behavior.unwrap_or("domain")).await?
    } else {
        tokio::fs::read_to_string(&final_path)
            .await
            .map_err(|e| format!("Не удалось прочитать файл {final_path}: {e}"))?
    };

    let content = Arc::new(content);
    CONTENT_CACHE
        .write()
        .unwrap()
        .insert(name.to_string(), (mtime, content.clone()));
    Ok(content)
}

#[derive(Deserialize)]
pub struct RuleContentQuery {
    pub name: String,
    pub format: Option<String>,
    pub behavior: Option<String>,
    #[serde(rename = "vehicleType")]
    pub vehicle_type: Option<String>,
}

pub async fn get_ruleset_content(State(_state): State<AppState>, Query(params): Query<RuleContentQuery>) -> Response {
    let docs = match load_mihomo_yaml().await {
        Ok(d) => d,
        Err(e) => return error_response(e),
    };
    let Some(parsed) = docs.first() else {
        return error_response("YAML пуст".into());
    };

    let provider = &parsed["rule-providers"][params.name.as_str()];
    if provider.is_badvalue() {
        return error_response(format!("Провайдер '{}' не найден", params.name));
    }

    match provider_content(
        &params.name,
        provider,
        params.format.as_deref(),
        params.behavior.as_deref(),
        params.vehicle_type.as_deref(),
    )
    .await
    {
        Ok(content) => ok_response(content.as_str().to_string()),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
pub struct RuleSearchQuery {
    pub q: String,
}

/// Matching lines are capped per provider: a bare "com" matches most of a
/// geosite set, and neither the router's memory nor the browser's list needs
/// three hundred thousand rows to answer "is it in here".
const MAX_MATCHES_PER_PROVIDER: usize = 200;

/// Search every rule-provider for a substring, case-insensitively.
///
/// Server-side on purpose. The sets live on the router already, and answering
/// "which set puts this domain outside the tunnel" in the browser would mean
/// shipping every set over the LAN first.
pub async fn search_rulesets(State(_state): State<AppState>, Query(params): Query<RuleSearchQuery>) -> Response {
    let needle = params.q.trim().to_lowercase();
    if needle.is_empty() {
        return error_response("Пустой запрос".into());
    }

    let docs = match load_mihomo_yaml().await {
        Ok(d) => d,
        Err(e) => return error_response(e),
    };
    let Some(parsed) = docs.first() else {
        return error_response("YAML пуст".into());
    };
    let Some(providers) = parsed["rule-providers"].as_hash() else {
        return ok_search(Vec::new(), 0);
    };

    let mut results = Vec::new();
    let mut total = 0usize;
    for (key, provider) in providers {
        let Some(name) = key.as_str() else { continue };
        let content = match provider_content(
            name,
            provider,
            provider["format"].as_str(),
            provider["behavior"].as_str(),
            provider["type"].as_str(),
        )
        .await
        {
            Ok(c) => c,
            // One unreadable set must not blank the whole answer: the others
            // may well hold the domain being looked for.
            Err(e) => {
                results.push(serde_json::json!({
                    "name": name, "error": e, "matches": [], "matchCount": 0, "truncated": false,
                }));
                continue;
            }
        };

        let (matches, count) = match_lines(&content, &needle);
        if count == 0 {
            continue;
        }
        total += count;
        results.push(serde_json::json!({
            "name": name,
            "matches": matches,
            "matchCount": count,
            "truncated": count > matches.len(),
        }));
    }

    // Most hits first: the set that really governs the domain is usually the
    // one with the specific entry, not the one that happens to share a suffix.
    results.sort_by_key(|r| std::cmp::Reverse(r["matchCount"].as_u64().unwrap_or(0)));
    ok_search(results, total)
}

/// Lines containing `needle`, capped, plus how many there were in total.
///
/// `needle` must already be lowercase: it is compared against a lowercased copy
/// of each line, which is what makes the search case-insensitive.
fn match_lines<'a>(content: &'a str, needle: &str) -> (Vec<&'a str>, usize) {
    let mut matches = Vec::new();
    let mut count = 0usize;
    for line in content.lines() {
        if line.to_lowercase().contains(needle) {
            count += 1;
            if matches.len() < MAX_MATCHES_PER_PROVIDER {
                matches.push(line);
            }
        }
    }
    (matches, count)
}

fn ok_search(results: Vec<serde_json::Value>, total: usize) -> Response {
    (
        StatusCode::OK,
        axum::Json(ApiResponse {
            success: true,
            error: None,
            data: Some(serde_json::json!({ "results": results, "total": total })),
        }),
    )
        .into_response()
}

async fn convert_mrs(mrs_path: &str, behavior: &str) -> Result<String, String> {
    if tokio::fs::metadata(mrs_path).await.is_err() {
        return Err(format!("MRS файл не найден: {mrs_path}"));
    }

    let behavior = behavior.to_ascii_lowercase();
    let tmp_path = format!("/opt/tmp/convert_{}", random_suffix());

    let output = Command::new("/opt/sbin/mihomo")
        .args(["convert-ruleset", behavior.as_str(), "mrs", mrs_path, &tmp_path])
        .output()
        .await
        .map_err(|e| format!("Ошибка запуска mihomo: {e}"))?;

    if !output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "mihomo convert-ruleset упал с кодом {}: {}",
            output.status, stderr
        ));
    }

    let content = tokio::fs::read_to_string(&tmp_path)
        .await
        .map_err(|e| format!("Ошибка чтения результата конвертации: {e}"));

    let _ = tokio::fs::remove_file(&tmp_path).await;
    content
}

fn random_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:08x}", nanos ^ std::process::id().wrapping_shl(8))
}

fn resolve_provider_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{}/{}", MIHOMO_CONF_DIR, path.trim_start_matches("./"))
    }
}

fn ok_response(content: String) -> Response {
    (
        StatusCode::OK,
        axum::Json(ApiResponse {
            success: true,
            error: None,
            data: Some(serde_json::json!({ "content": content })),
        }),
    )
        .into_response()
}

fn error_response(msg: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(ApiResponse::<()> {
            success: false,
            error: Some(msg),
            data: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ignore_case_on_both_sides() {
        let (matches, count) = match_lines("+.Example.COM\nother.net", "example.com");
        assert_eq!(count, 1);
        assert_eq!(matches, vec!["+.Example.COM"]);
    }

    #[test]
    fn a_substring_matches_inside_a_rule_line() {
        // Sets store entries as "+.domain", "DOMAIN-SUFFIX,domain" and bare
        // hosts, so anchoring the search would miss most of the real formats.
        let content = "DOMAIN-SUFFIX,vk.com\n+.vk.com\nvk.com\nnotvk.company";
        let (_, count) = match_lines(content, "vk.com");
        assert_eq!(count, 4);
    }

    #[test]
    fn the_full_count_is_reported_even_when_the_sample_is_capped() {
        // A short query matches most of a geosite set. The browser gets a
        // sample; the count still has to be the truth, or "is it in here" is
        // answered with a number that silently means "at least".
        let content = (0..MAX_MATCHES_PER_PROVIDER + 50)
            .map(|i| format!("host{i}.com"))
            .collect::<Vec<_>>()
            .join("\n");
        let (matches, count) = match_lines(&content, ".com");
        assert_eq!(matches.len(), MAX_MATCHES_PER_PROVIDER);
        assert_eq!(count, MAX_MATCHES_PER_PROVIDER + 50);
    }

    #[test]
    fn nothing_matches_an_absent_needle() {
        let (matches, count) = match_lines("a.com\nb.com", "zzz");
        assert_eq!(count, 0);
        assert!(matches.is_empty());
    }
}
