use anyhow::{Context, Result};
use serde_json::{json, Value};
pub struct Drive<'a> {
    pub client: &'a reqwest::Client,
    pub token: &'a str,
}
pub fn folder_id(link: &str) -> Result<Option<String>> {
    if link.trim().is_empty() {
        return Ok(None);
    }
    let value = link.trim();
    let id = if value.starts_with("https://") {
        let url = url::Url::parse(value)?;
        anyhow::ensure!(
            url.host_str() == Some("drive.google.com"),
            "Use a Google Drive folder link"
        );
        url.path()
            .split("/folders/")
            .nth(1)
            .context("This is not a Drive folder link")?
            .trim_end_matches('/')
            .to_string()
    } else {
        value.into()
    };
    anyhow::ensure!(
        !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "Invalid Drive folder ID"
    );
    Ok(Some(id))
}
impl Drive<'_> {
    async fn json(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = request.bearer_auth(self.token).send().await?;
        anyhow::ensure!(response.status().is_success(),"Google Drive returned HTTP {}. Check account access, free storage, and the selected folder.",response.status());
        Ok(response.json().await?)
    }
    pub async fn folder(&self, preferred: &str) -> Result<String> {
        if let Some(id) = folder_id(preferred)? {
            let data = self
                .json(
                    self.client
                        .get(format!("https://www.googleapis.com/drive/v3/files/{id}"))
                        .query(&[("fields", "id,mimeType,trashed")]),
                )
                .await?;
            anyhow::ensure!(
                data["mimeType"] == "application/vnd.google-apps.folder" && data["trashed"] != true,
                "The Drive destination must be an accessible folder"
            );
            return Ok(id);
        }
        let result=self.json(self.client.get("https://www.googleapis.com/drive/v3/files").query(&[("q","trashed = false and mimeType = 'application/vnd.google-apps.folder' and appProperties has { key='notesai_kind' and value='backups' }"),("fields","files(id)"),("pageSize","100")])).await?;
        if let Some(id) = result["files"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v["id"].as_str())
        {
            return Ok(id.into());
        }
        let result=self.json(self.client.post("https://www.googleapis.com/drive/v3/files").json(&json!({"name":"NotesAI Backups","mimeType":"application/vnd.google-apps.folder","appProperties":{"notesai_kind":"backups"}}))).await?;
        result["id"]
            .as_str()
            .map(str::to_owned)
            .context("Drive did not return a folder ID")
    }
    pub async fn upload(
        &self,
        folder: &str,
        device: &str,
        slot: u8,
        bytes: Vec<u8>,
        hash: &str,
    ) -> Result<()> {
        let query=format!("trashed = false and '{folder}' in parents and appProperties has {{ key='notesai_device' and value='{device}' }} and appProperties has {{ key='notesai_slot' and value='{slot}' }}");
        let data = self
            .json(
                self.client
                    .get("https://www.googleapis.com/drive/v3/files")
                    .query(&[
                        ("q", query.as_str()),
                        ("fields", "files(id)"),
                        ("pageSize", "100"),
                    ]),
            )
            .await?;
        let name = format!("notesai-{device}-{slot}.json.gz");
        let existing = data["files"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v["id"].as_str());
        if let Some(id) = existing {
            // Updating in place keeps a bounded two-slot history. Google expires old binary revisions automatically.
            self.json(
                self.client
                    .patch(format!(
                        "https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media"
                    ))
                    .header("Content-Type", "application/gzip")
                    .body(bytes),
            )
            .await?;
        } else {
            let boundary = format!("notesai_{}", uuid::Uuid::new_v4().simple());
            let metadata = json!({"name":name,"parents":[folder],"mimeType":"application/gzip","appProperties":{"notesai_device":device,"notesai_slot":slot.to_string(),"notesai_hash":hash,"notesai_format":"1"}});
            let mut body=format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n--{boundary}\r\nContent-Type: application/gzip\r\n\r\n").into_bytes();
            body.extend(bytes);
            body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
            self.json(
                self.client
                    .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
                    .header(
                        "Content-Type",
                        format!("multipart/related; boundary={boundary}"),
                    )
                    .body(body),
            )
            .await?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_folder_links() {
        assert_eq!(
            folder_id("https://drive.google.com/drive/folders/abc_123?usp=sharing").unwrap(),
            Some("abc_123".into())
        );
        assert!(folder_id("https://evil.example/folders/test").is_err());
        assert!(folder_id("x' or '1'= '1").is_err());
        assert_eq!(folder_id("").unwrap(), None);
    }
}
