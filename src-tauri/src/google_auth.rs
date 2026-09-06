use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub access_token: String,
    pub expires_at: i64,
    pub account: Option<String>,
}
fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(windows)]
fn crypt(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len().try_into()?,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    anyhow::ensure!(
        success != 0,
        "Windows could not access the encrypted Google credentials"
    );
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut _);
    };
    Ok(result)
}
#[cfg(not(windows))]
fn crypt(_data: &[u8], _encrypt: bool) -> Result<Vec<u8>> {
    anyhow::bail!("Secure Google credential storage is currently implemented for Windows only")
}
pub fn load(path: &Path) -> Result<Option<Credentials>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&crypt(
        &std::fs::read(path)?,
        false,
    )?)?))
}
pub fn save(path: &Path, credentials: &Credentials) -> Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, crypt(&serde_json::to_vec(credentials)?, true)?)?;
    std::fs::rename(temp, path)?;
    Ok(())
}

pub fn callback_code(target: &str, state: &str) -> Result<String> {
    let url = url::Url::parse(&format!("http://localhost{target}"))?;
    let pairs = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    anyhow::ensure!(
        pairs.get("state").map(|s| s.as_ref()) == Some(state),
        "Google sign-in state did not match. Please try connecting again."
    );
    anyhow::ensure!(
        !pairs.contains_key("error"),
        "Google sign-in was declined or cancelled"
    );
    pairs
        .get("code")
        .map(|s| s.to_string())
        .context("Google did not return an authorization code")
}
pub async fn connect(
    client: &reqwest::Client,
    client_id: String,
    client_secret: String,
    cancel: Arc<AtomicBool>,
) -> Result<Credentials> {
    anyhow::ensure!(
        client_id.ends_with(".apps.googleusercontent.com"),
        "Enter a Google OAuth Desktop app client ID"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = random();
    let state = random();
    let mut auth = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    auth.query_pairs_mut().extend_pairs([
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/drive.file"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("code_challenge", challenge(&verifier).as_str()),
        ("code_challenge_method", "S256"),
        ("state", state.as_str()),
    ]);
    crate::open_source(auth.to_string()).map_err(|e| anyhow::anyhow!(e))?;
    let start = tokio::time::Instant::now();
    let code = loop {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Google sign-in cancelled");
        anyhow::ensure!(
            start.elapsed() < std::time::Duration::from_secs(180),
            "Google sign-in timed out. Please connect again."
        );
        let connection = tokio::select! {value=listener.accept()=>value?,_=tokio::time::sleep(std::time::Duration::from_millis(200))=>continue};
        let (mut socket, _) = connection;
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let count = socket.read(&mut chunk).await?;
                if count == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..count]);
                anyhow::ensure!(buffer.len() <= 8192, "OAuth callback is too large");
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<_, anyhow::Error>(buffer)
        })
        .await;
        let buffer = match read {
            Ok(Ok(buffer)) => buffer,
            _ => continue,
        };
        let request = match std::str::from_utf8(&buffer) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let target = request
            .lines()
            .next()
            .and_then(|s| s.split_whitespace().nth(1))
            .unwrap_or("/");
        if !target.contains("code=") && !target.contains("error=") {
            let _ = socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            continue;
        }
        let result = callback_code(target, &state);
        let html="<!doctype html><title>NotesAI</title><h2>Return to NotesAI</h2><p>You can close this tab. NotesAI will show your connection status.</p>";
        let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",html.len(),html);
        let _ = socket.write_all(response.as_bytes()).await;
        break result?;
    };
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;
    anyhow::ensure!(response.status().is_success(),"Google token exchange failed (HTTP {}). Check the Desktop client configuration and test-user access.",response.status());
    let data: serde_json::Value = response.json().await?;
    let access = data["access_token"]
        .as_str()
        .context("Google returned no access token")?
        .to_string();
    let account = client
        .get("https://www.googleapis.com/drive/v3/about?fields=user(emailAddress)")
        .bearer_auth(&access)
        .send()
        .await
        .ok();
    let account = if let Some(response) = account {
        response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["user"]["emailAddress"].as_str().map(str::to_owned))
    } else {
        None
    };
    Ok(Credentials {
        client_id,
        client_secret,
        refresh_token: data["refresh_token"]
            .as_str()
            .context("Google returned no refresh token. Disconnect and reconnect with consent.")?
            .into(),
        access_token: access,
        expires_at: chrono::Utc::now().timestamp() + data["expires_in"].as_i64().unwrap_or(3600),
        account,
    })
}
pub async fn access_token(client: &reqwest::Client, path: &Path) -> Result<String> {
    let mut auth = load(path)?.context("Connect Google Drive in Settings first")?;
    if auth.expires_at < chrono::Utc::now().timestamp() + 60 {
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", auth.client_id.as_str()),
                ("client_secret", auth.client_secret.as_str()),
                ("refresh_token", auth.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Google authorization expired or was revoked. Reconnect Google Drive in Settings."
        );
        let data: serde_json::Value = response.json().await?;
        auth.access_token = data["access_token"]
            .as_str()
            .context("Google returned no access token")?
            .into();
        auth.expires_at =
            chrono::Utc::now().timestamp() + data["expires_in"].as_i64().unwrap_or(3600);
        save(path, &auth)?;
    }
    Ok(auth.access_token)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_matches_rfc_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn callback_rejects_forged_state() {
        assert!(callback_code("/?code=secret&state=wrong", "expected").is_err());
        assert_eq!(
            callback_code("/?code=abc&state=expected", "expected").unwrap(),
            "abc"
        );
    }
    #[cfg(windows)]
    #[test]
    fn stored_credentials_are_encrypted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.bin");
        let auth = Credentials {
            client_id: "id".into(),
            client_secret: "client-secret".into(),
            refresh_token: "refresh-secret".into(),
            access_token: "access-secret".into(),
            expires_at: 100,
            account: None,
        };
        save(&path, &auth).unwrap();
        assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("refresh-secret"));
        assert_eq!(
            load(&path).unwrap().unwrap().refresh_token,
            "refresh-secret"
        );
    }
}
