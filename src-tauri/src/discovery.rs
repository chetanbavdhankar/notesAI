use crate::llm::{self, Profile};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Discovery {
    pub base_url: String,
    pub models: Vec<String>,
    pub message: String,
}

pub fn is_ollama(profile: &Profile) -> bool {
    match profile.provider.as_deref() {
        Some(provider) => provider == "ollama",
        None => {
            profile.name.eq_ignore_ascii_case("ollama")
                || url::Url::parse(&profile.base_url)
                    .ok()
                    .and_then(|u| u.port())
                    == Some(11434)
        }
    }
}

// Accept OLLAMA_HOST bind addresses as well as a saved OpenAI-compatible base URL.
pub fn ollama_root(raw: &str) -> Result<String> {
    let raw = raw.trim();
    anyhow::ensure!(!raw.is_empty(), "Ollama address is empty");
    let qualified = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    let mut url = url::Url::parse(&qualified)?;
    if !raw.contains("://") && url.port().is_none() {
        url.set_port(Some(11434))
            .map_err(|_| anyhow::anyhow!("Invalid Ollama port"))?;
    }
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Ollama requires an HTTP(S) address"
    );
    anyhow::ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Ollama URL cannot contain embedded credentials, query, or fragment"
    );
    if url.host_str() == Some("0.0.0.0") {
        url.set_host(Some("127.0.0.1"))?;
    }
    if url.host_str() == Some("[::]") {
        url.set_host(Some("[::1]"))?;
    }
    let path = url.path().trim_end_matches('/').to_string();
    let path = path
        .strip_suffix("/v1")
        .or_else(|| path.strip_suffix("/api/tags"))
        .or_else(|| path.strip_suffix("/api"))
        .unwrap_or(&path);
    url.set_path(path);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn configured_hosts() -> Vec<String> {
    let mut hosts = vec![];
    if let Ok(host) = std::env::var("OLLAMA_HOST") {
        hosts.push(host);
    }
    // Read persisted Windows environment too: NotesAI may predate a settings change.
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
            RegKey,
        };
        for (hive, key) in [
            (HKEY_CURRENT_USER, "Environment"),
            (
                HKEY_LOCAL_MACHINE,
                "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            ),
        ] {
            if let Ok(key) = RegKey::predef(hive).open_subkey(key) {
                if let Ok(host) = key.get_value::<String, _>("OLLAMA_HOST") {
                    hosts.push(host);
                }
            }
        }
    }
    hosts
}

fn candidates(profile: &Profile, hosts: Vec<String>) -> Vec<String> {
    let mut candidates = vec![];
    let defaults = [
        "http://127.0.0.1:11434",
        "http://localhost:11434",
        "http://[::1]:11434",
    ];
    let is_default = ollama_root(&profile.base_url)
        .ok()
        .is_some_and(|root| defaults.contains(&root.as_str()));
    let configured = if is_default {
        hosts
            .into_iter()
            .chain([profile.base_url.clone()])
            .collect::<Vec<_>>()
    } else {
        std::iter::once(profile.base_url.clone())
            .chain(hosts)
            .collect()
    };
    for value in configured.into_iter().chain(defaults.map(str::to_owned)) {
        if let Ok(root) = ollama_root(&value) {
            if !candidates.contains(&root) {
                candidates.push(root);
            }
        }
    }
    candidates
}

async fn probe(profile: &Profile, root: &str) -> Result<Vec<String>> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none());
    // Local servers must not be routed through a corporate HTTP proxy.
    let url = url::Url::parse(root)?;
    if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
        builder = builder.no_proxy();
    }
    let mut request = builder.build()?.get(format!("{root}/api/tags"));
    // Never forward a configured server's key to fallback addresses.
    if ollama_root(&profile.base_url).ok().as_deref() == Some(root) && !profile.api_key.is_empty() {
        request = request.bearer_auth(&profile.api_key);
    }
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            anyhow::anyhow!("timed out")
        } else {
            anyhow::anyhow!("could not connect")
        }
    })?;
    anyhow::ensure!(response.status().is_success(), "HTTP {}", response.status());
    let data: Value = response.json().await.context("Invalid JSON from server")?;
    let entries = data["models"]
        .as_array()
        .context("Server is not returning an Ollama model list")?;
    let mut models = entries
        .iter()
        .filter_map(|m| m["name"].as_str().or_else(|| m["model"].as_str()))
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(models)
}

pub async fn discover(client: &reqwest::Client, profile: &Profile) -> Result<Discovery> {
    if !is_ollama(profile) {
        let models = llm::models(client, profile).await?;
        return Ok(Discovery {
            base_url: profile.base_url.trim_end_matches('/').to_string(),
            message: format!("Found {} models from this provider.", models.len()),
            models,
        });
    }
    discover_ollama(profile, candidates(profile, configured_hosts())).await
}

async fn discover_ollama(profile: &Profile, roots: Vec<String>) -> Result<Discovery> {
    let mut failures = vec![];
    // Respect an explicit address first; fall back only when that endpoint fails.
    for root in roots {
        match probe(profile, &root).await {
            Ok(models) => {
                let message = if models.is_empty() {
                    format!("Connected to {root}. No models installed yet. Run `ollama pull <model>` and discover again.")
                } else {
                    format!("Connected to {root}. Found {} installed models (including unloaded models).",models.len())
                };
                return Ok(Discovery {
                    base_url: format!("{root}/v1"),
                    models,
                    message,
                });
            }
            Err(error) => failures.push(format!("{root}: {error}")),
        }
    }
    anyhow::bail!("Ollama could not be reached. Open Ollama or run `ollama serve`, then retry. If it uses another address, enter it above or set OLLAMA_HOST. Checked: {}",failures.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn server(body: &str, status: u16) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = body.to_owned();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            let mut buf = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let count = socket.read(&mut buf).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..count]);
            }
            write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{addr}"), thread)
    }
    #[test]
    fn normalizes_ollama_hosts() {
        for (input, expected) in [
            ("0.0.0.0:11435", "http://127.0.0.1:11435"),
            ("http://localhost:11434/v1/", "http://localhost:11434"),
            ("http://[::]:11434", "http://[::1]:11434"),
            ("https://server/ollama/api/tags", "https://server/ollama"),
        ] {
            assert_eq!(ollama_root(input).unwrap(), expected);
        }
        assert!(ollama_root("file:///tmp").is_err());
        assert!(ollama_root("http://user:secret@host").is_err());
    }
    #[tokio::test]
    async fn lists_installed_models_and_returns_correct_chat_url() {
        let (root, server) = server(
            r#"{"models":[{"name":"qwen3:8b"},{"name":"llama3.2:latest"},{"model":"qwen3:8b"}]}"#,
            200,
        );
        let mut profile = llm::Settings::default().profiles.remove(0);
        profile.base_url = format!("{root}/v1");
        let result = discover_ollama(&profile, vec![root.clone()]).await.unwrap();
        assert_eq!(result.models, vec!["llama3.2:latest", "qwen3:8b"]);
        assert_eq!(result.base_url, format!("{root}/v1"));
        assert!(server.join().unwrap().starts_with("GET /api/tags "));
    }
    #[tokio::test]
    async fn fallback_does_not_leak_api_key_and_empty_inventory_is_success() {
        let (bad, bad_server) = server(r#"{"error":"not ollama"}"#, 404);
        let (good, good_server) = server(r#"{"models":[]}"#, 200);
        let mut profile = llm::Settings::default().profiles.remove(0);
        profile.base_url = bad.clone();
        profile.api_key = "test-secret".into();
        let result = discover_ollama(&profile, vec![bad, good.clone()])
            .await
            .unwrap();
        assert!(result.models.is_empty());
        assert_eq!(result.base_url, format!("{good}/v1"));
        assert!(result.message.contains("No models installed"));
        assert!(bad_server.join().unwrap().contains("test-secret"));
        assert!(!good_server.join().unwrap().contains("test-secret"));
    }
    #[test]
    fn old_profiles_and_custom_provider_route_correctly() {
        let mut profile = llm::Settings::default().profiles.remove(0);
        profile.provider = None;
        assert!(is_ollama(&profile));
        profile.provider = Some("custom".into());
        assert!(!is_ollama(&profile));
    }
    #[test]
    fn environment_overrides_default_but_not_explicit_custom_address() {
        let mut profile = llm::Settings::default().profiles.remove(0);
        assert_eq!(
            candidates(&profile, vec!["0.0.0.0:11500".into()])[0],
            "http://127.0.0.1:11500"
        );
        profile.base_url = "http://localhost:11600/v1".into();
        assert_eq!(
            candidates(&profile, vec!["0.0.0.0:11500".into()])[0],
            "http://localhost:11600"
        );
    }
}
