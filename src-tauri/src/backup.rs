use crate::{db, drive, google_auth};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub automatic: bool,
    pub interval_minutes: u64,
    pub client_id: String,
    pub folder_link: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            automatic: false,
            interval_minutes: 15,
            client_id: String::new(),
            folder_link: String::new(),
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupState {
    pub device_id: String,
    pub local_hash: String,
    pub uploaded_hash: String,
    pub folder_id: String,
    pub next_slot: u8,
    pub last_local_at: Option<String>,
    pub last_upload_at: Option<String>,
    pub compressed_bytes: u64,
    pub note_count: usize,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct Status {
    pub connected: bool,
    pub account: Option<String>,
    pub local_path: String,
    #[serde(flatten)]
    pub state: BackupState,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    pub kind: String,
    pub tags: Vec<String>,
    pub created_at: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub organized: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub format: String,
    pub schema_version: u32,
    pub device_id: String,
    pub captured_at: String,
    pub notes: Vec<Record>,
}
#[derive(Serialize)]
pub struct Restored {
    pub imported: usize,
    pub skipped: usize,
    pub conflicts: usize,
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, path)?;
    Ok(())
}
pub fn records(path: &Path) -> Result<Vec<Record>> {
    let mut notes = db::list(path)?
        .into_iter()
        .map(|n| Record {
            id: n.id,
            title: n.title,
            body: n.body,
            source_url: n.source_url,
            kind: n.kind,
            tags: n.tags,
            created_at: n.created_at,
            topics: n.topics,
            organized: n.organized,
        })
        .collect::<Vec<_>>();
    notes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(notes)
}
pub fn compress(snapshot: &Snapshot) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(snapshot)?;
    anyhow::ensure!(
        json.len() <= 64_000_000,
        "This backup exceeds the current 64 MB uncompressed limit"
    );
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&json)?;
    Ok(encoder.finish()?)
}
pub fn decode(bytes: &[u8]) -> Result<Snapshot> {
    anyhow::ensure!(bytes.len() <= 32_000_000, "Backup file exceeds 32 MB");
    let mut json = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(64_000_001)
        .read_to_end(&mut json)?;
    anyhow::ensure!(
        json.len() <= 64_000_000,
        "Backup expands beyond the 64 MB limit"
    );
    let snapshot: Snapshot = serde_json::from_slice(&json)?;
    anyhow::ensure!(
        snapshot.format == "notesai.snapshot" && (1..=2).contains(&snapshot.schema_version),
        "Unsupported NotesAI backup version"
    );
    anyhow::ensure!(
        snapshot.notes.len() <= 100_000,
        "Too many records in backup"
    );
    for note in &snapshot.notes {
        crate::organize::clean(note.topics.clone())?;
        uuid::Uuid::parse_str(&note.id)?;
        anyhow::ensure!(
            !note.body.trim().is_empty() && note.body.len() <= 5_000_000,
            "Invalid note in backup"
        );
        if let Some(source) = &note.source_url {
            anyhow::ensure!(
                matches!(url::Url::parse(source)?.scheme(), "http" | "https"),
                "Unsafe source URL in backup"
            );
        }
    }
    Ok(snapshot)
}
pub fn restore(db_path: &Path, bytes: &[u8]) -> Result<Restored> {
    let snapshot = decode(bytes)?;
    let mut db = db::open(db_path)?;
    let tx = db.transaction()?;
    let mut result = Restored {
        imported: 0,
        skipped: 0,
        conflicts: 0,
    };
    for n in snapshot.notes {
        use rusqlite::OptionalExtension;
        let existing: Option<(String, String, String, Option<String>, String)> = tx
            .query_row(
                "SELECT title,body,tags,source_url,kind FROM notes WHERE id=?",
                [&n.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let tags = serde_json::to_string(&n.tags)?;
        let mut id = n.id;
        let mut title = n.title;
        if let Some((t, b, g, u, k)) = existing {
            let existing_topics: Option<(String,bool)>=tx.query_row("SELECT topics,reviewed_revision=(SELECT revision FROM notes WHERE id=?) FROM note_topics WHERE note_id=?",[&id,&id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (existing_topics, organized) = existing_topics.unwrap_or(("[]".into(), false));
            if t == title
                && b == n.body
                && g == tags
                && u == n.source_url
                && k == n.kind
                && existing_topics == serde_json::to_string(&n.topics)?
                && organized == n.organized
            {
                result.skipped += 1;
                continue;
            }
            id = uuid::Uuid::new_v4().to_string();
            title.push_str(" (restored copy)");
            result.conflicts += 1;
        }
        let status = if n.source_url.as_deref() == Some(n.body.trim()) {
            "queued"
        } else {
            "indexing"
        };
        tx.execute("INSERT INTO notes(id,title,body,source_url,kind,tags,created_at,status) VALUES(?,?,?,?,?,?,?,?)",rusqlite::params![id,title,n.body,n.source_url,n.kind,tags,n.created_at,status])?;
        if n.organized || !n.topics.is_empty() {
            tx.execute("INSERT INTO note_topics(note_id,topics,reviewed_revision,reviewed_at) VALUES(?,?,?,?)",rusqlite::params![id,serde_json::to_string(&n.topics)?,if n.organized{0}else{-1},chrono::Utc::now().to_rfc3339()])?;
        }
        result.imported += 1;
    }
    tx.commit()?;
    Ok(result)
}
pub struct Service {
    pub dir: PathBuf,
    pub db: PathBuf,
    pub lock: tokio::sync::Mutex<()>,
    pub oauth_cancel: Arc<AtomicBool>,
    pub oauth_active: AtomicBool,
    client: reqwest::Client,
}
impl Service {
    pub fn new(dir: PathBuf, db: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(dir.join("backups"))?;
        Ok(Self {
            dir,
            db,
            lock: tokio::sync::Mutex::new(()),
            oauth_cancel: Arc::new(AtomicBool::new(false)),
            oauth_active: AtomicBool::new(false),
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
        })
    }
    fn auth_path(&self) -> PathBuf {
        self.dir.join("google-auth.bin")
    }
    fn state(&self) -> Result<BackupState> {
        let path = self.dir.join("backup-state.json");
        if path.exists() {
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        } else {
            Ok(BackupState {
                device_id: uuid::Uuid::new_v4().to_string(),
                ..Default::default()
            })
        }
    }
    fn save_state(&self, state: &BackupState) -> Result<()> {
        atomic(
            &self.dir.join("backup-state.json"),
            &serde_json::to_vec(state)?,
        )
    }
    pub fn status(&self) -> Result<Status> {
        let state = self.state()?;
        let auth = google_auth::load(&self.auth_path())?;
        Ok(Status {
            connected: auth.is_some(),
            account: auth.and_then(|a| a.account),
            local_path: self
                .dir
                .join("backups/latest.notesai.json.gz")
                .display()
                .to_string(),
            state,
        })
    }
    pub async fn connect(&self, id: String, secret: String) -> Result<Status> {
        anyhow::ensure!(
            !self.oauth_active.swap(true, Ordering::SeqCst),
            "Google sign-in is already open"
        );
        self.oauth_cancel.store(false, Ordering::SeqCst);
        let result =
            google_auth::connect(&self.client, id, secret, self.oauth_cancel.clone()).await;
        self.oauth_active.store(false, Ordering::SeqCst);
        let auth = result?;
        let _guard = self.lock.lock().await;
        anyhow::ensure!(
            !self.oauth_cancel.load(Ordering::SeqCst),
            "Google sign-in cancelled"
        );
        google_auth::save(&self.auth_path(), &auth)?;
        let mut state = self.state()?;
        state.uploaded_hash.clear();
        state.folder_id.clear();
        state.last_upload_at = None;
        state.error = None;
        self.save_state(&state)?;
        self.status()
    }
    pub async fn disconnect(&self) -> Result<Status> {
        self.oauth_cancel.store(true, Ordering::SeqCst);
        let _guard = self.lock.lock().await;
        let path = self.auth_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let mut state = self.state()?;
        state.uploaded_hash.clear();
        state.folder_id.clear();
        self.save_state(&state)?;
        self.status()
    }
    fn local_snapshot(&self, state: &mut BackupState) -> Result<Vec<u8>> {
        let notes = records(&self.db)?;
        let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&notes)?));
        let latest = self.dir.join("backups/latest.notesai.json.gz");
        if hash == state.local_hash && latest.exists() {
            return Ok(std::fs::read(latest)?);
        }
        let snapshot = Snapshot {
            format: "notesai.snapshot".into(),
            schema_version: 2,
            device_id: state.device_id.clone(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            notes,
        };
        let bytes = compress(&snapshot)?;
        if latest.exists() {
            atomic(
                &self.dir.join("backups/previous.notesai.json.gz"),
                &std::fs::read(&latest)?,
            )?;
        }
        atomic(&latest, &bytes)?;
        state.local_hash = hash;
        state.last_local_at = Some(snapshot.captured_at);
        state.note_count = snapshot.notes.len();
        state.compressed_bytes = bytes.len() as u64;
        self.save_state(state)?;
        Ok(bytes)
    }
    pub async fn backup(&self, config: &Config, upload: bool) -> Result<Status> {
        let _guard = self.lock.lock().await;
        let mut state = self.state()?;
        let result = async {
            let bytes = self.local_snapshot(&mut state)?;
            if upload {
                let token = google_auth::access_token(&self.client, &self.auth_path()).await?;
                let drive = drive::Drive {
                    client: &self.client,
                    token: &token,
                };
                let folder = drive.folder(&config.folder_link).await?;
                if state.uploaded_hash != state.local_hash || state.folder_id != folder {
                    let slot = state.next_slot % 2;
                    drive
                        .upload(&folder, &state.device_id, slot, bytes, &state.local_hash)
                        .await?;
                    state.next_slot = 1 - slot;
                    state.uploaded_hash = state.local_hash.clone();
                    state.folder_id = folder;
                    state.last_upload_at = Some(chrono::Utc::now().to_rfc3339());
                }
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        state.error = result.as_ref().err().map(ToString::to_string);
        self.save_state(&state)?;
        result?;
        self.status()
    }
    pub async fn export(&self, path: &Path) -> Result<Status> {
        let _guard = self.lock.lock().await;
        let mut state = self.state()?;
        let bytes = self.local_snapshot(&mut state)?;
        atomic(path, &bytes)?;
        self.status()
    }
    pub async fn import(&self, path: &Path) -> Result<Restored> {
        let _guard = self.lock.lock().await;
        let file = std::fs::File::open(path)?;
        anyhow::ensure!(file.metadata()?.len() <= 32_000_000, "Backup exceeds 32 MB");
        let mut bytes = vec![];
        file.take(32_000_001).read_to_end(&mut bytes)?;
        restore(&self.db, &bytes)
    }
}
pub async fn scheduled(service: Arc<Service>, settings_path: PathBuf) {
    let mut last_attempt = std::time::Instant::now();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        if let Ok(settings) = crate::llm::load(&settings_path) {
            let config = settings.backup;
            if config.automatic
                && last_attempt.elapsed()
                    >= std::time::Duration::from_secs(config.interval_minutes.clamp(5, 1440) * 60)
            {
                last_attempt = std::time::Instant::now();
                let _ = service.backup(&config, true).await;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn compact_backup_restores_without_indices_or_secrets() -> Result<()> {
        let d = tempfile::tempdir()?;
        let source = d.path().join("one.db");
        db::init(&source)?;
        let id = db::capture(
            &source,
            &"Portable personal note. ".repeat(400),
            vec!["work".into()],
        )?;
        let service = Service::new(d.path().into(), source.clone())?;
        crate::organize::apply(&source, &id, 0, vec!["Research".into()])?;
        service.backup(&Config::default(), false).await?;
        let bytes = std::fs::read(d.path().join("backups/latest.notesai.json.gz"))?;
        assert!(bytes.len() < 1500);
        let snapshot = decode(&bytes)?;
        assert_eq!(snapshot.notes[0].id, id);
        let json = serde_json::to_string(&snapshot)?;
        assert!(!json.contains("api_key"));
        assert!(!json.contains("embedding"));
        let target = d.path().join("two.db");
        db::init(&target)?;
        assert_eq!(restore(&target, &bytes)?.imported, 1);
        assert_eq!(db::get(&target, &id)?.topics, vec!["Research"]);
        assert!(db::get(&target, &id)?.organized);
        assert_eq!(restore(&target, &bytes)?.skipped, 1);
        db::edit(&target, &id, "Newer local text", "Do not overwrite", vec![])?;
        assert_eq!(restore(&target, &bytes)?.conflicts, 1);
        assert_eq!(db::get(&target, &id)?.body, "Do not overwrite");
        Ok(())
    }
    #[test]
    fn invalid_archive_is_rejected() {
        assert!(decode(b"not gzip").is_err());
    }
}
