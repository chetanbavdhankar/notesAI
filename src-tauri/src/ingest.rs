use crate::db;
use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use scraper::{Html, Selector};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tokenizers::Tokenizer;

pub struct Embeddings {
    pub cache: PathBuf,
    inner: Mutex<Option<(TextEmbedding, Tokenizer)>>,
}
impl Embeddings {
    pub fn new(cache: PathBuf) -> Self {
        Self {
            cache,
            inner: Mutex::new(None),
        }
    }
    fn ready(&self, guard: &mut Option<(TextEmbedding, Tokenizer)>) -> Result<()> {
        if guard.is_none() {
            let model = TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::BGESmallENV15)
                    .with_max_length(512)
                    .with_cache_dir(self.cache.clone())
                    .with_show_download_progress(true),
            )?;
            let path = find_tokenizer(&self.cache).context("Downloaded BGE tokenizer not found")?;
            let mut tokenizer =
                Tokenizer::from_file(path).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            tokenizer
                .with_truncation(None)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            tokenizer.with_padding(None);
            *guard = Some((model, tokenizer));
        }
        Ok(())
    }
    pub fn query(&self, text: &str) -> Result<Vec<f32>> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Embedding lock poisoned"))?;
        self.ready(&mut guard)?;
        let (model, _) = guard.as_mut().unwrap();
        model
            .embed(
                vec![format!(
                    "Represent this sentence for searching relevant passages: {text}"
                )],
                Some(1),
            )?
            .pop()
            .context("No embedding returned")
    }
    pub fn index(&self, path: &Path, id: &str) -> Result<()> {
        let db = db::open(path)?;
        let (body, revision): (String, i64) =
            db.query_row("SELECT body,revision FROM notes WHERE id=?", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("Embedding lock poisoned"))?;
        self.ready(&mut guard)?;
        let (model, tokenizer) = guard.as_mut().unwrap();
        let encoded = tokenizer
            .encode(body.as_str(), false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let chunks = chunk_offsets(&body, encoded.get_offsets(), 510, 51);
        anyhow::ensure!(!chunks.is_empty(), "No readable text to index");
        let embeddings = model.embed(chunks.clone(), Some(32))?;
        anyhow::ensure!(embeddings.len() == chunks.len(), "Embedding count mismatch");
        let mut db = db::open(path)?;
        let tx = db.transaction()?;
        let current: Option<i64> = tx
            .query_row("SELECT revision FROM notes WHERE id=?", [id], |r| r.get(0))
            .ok();
        if current != Some(revision) {
            return Ok(());
        }
        db::clear_chunks(&tx, id)?;
        for (i, (text, embedding)) in chunks.iter().zip(embeddings).enumerate() {
            anyhow::ensure!(
                embedding.len() == 384 && embedding.iter().all(|x| x.is_finite()),
                "Invalid embedding"
            );
            tx.execute(
                "INSERT INTO chunks(note_id,ordinal,text) VALUES(?,?,?)",
                rusqlite::params![id, i as i64, text],
            )?;
            tx.execute(
                "INSERT INTO chunk_vectors(rowid,embedding) VALUES(?,?)",
                rusqlite::params![tx.last_insert_rowid(), db::bytes(&embedding)],
            )?;
        }
        tx.execute("UPDATE notes SET status='ready' WHERE id=?", [id])?;
        tx.commit()?;
        Ok(())
    }
}
fn find_tokenizer(dir: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.file_name()?.to_str() == Some("tokenizer.json") {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(found) = find_tokenizer(&p) {
                return Some(found);
            }
        }
    }
    None
}
pub fn chunk_offsets(
    text: &str,
    offsets: &[(usize, usize)],
    size: usize,
    overlap: usize,
) -> Vec<String> {
    assert!(size > overlap);
    let mut out = vec![];
    let mut start = 0;
    while start < offsets.len() {
        let end = (start + size).min(offsets.len());
        let a = offsets[start].0;
        let b = offsets[end - 1].1;
        if let Some(piece) = text.get(a..b) {
            if !piece.trim().is_empty() {
                out.push(piece.to_owned())
            }
        }
        if end == offsets.len() {
            break;
        }
        start = end - overlap;
    }
    out
}
pub struct Hydrated {
    pub title: String,
    pub body: String,
    pub kind: String,
    pub warning: Option<String>,
}
pub async fn hydrate(client: &reqwest::Client, url: &str) -> Result<Hydrated> {
    let parsed = url::Url::parse(url)?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "Only HTTP(S) sources are supported"
    );
    let host = parsed.host_str().unwrap_or("");
    if host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com") {
        return youtube(url).await;
    }
    let mut response = client.get(url).send().await?.error_for_status()?;
    if let Some(len) = response.content_length() {
        anyhow::ensure!(len <= 5_000_000, "Web page exceeds 5 MB")
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 5_000_000,
            "Web page exceeds 5 MB"
        );
        bytes.extend_from_slice(&chunk)
    }
    let html = String::from_utf8_lossy(&bytes);
    // Remove active content and common boilerplate before converting to markdown.
    let cleaned=regex::Regex::new(r"(?is)<(script|style|nav|header|footer|aside|noscript)\b[^>]*>.*?</(?:script|style|nav|header|footer|aside|noscript)\s*>")?.replace_all(&html,"");
    let doc = Html::parse_document(&cleaned);
    let meta = |key: &str| -> Option<String> {
        let s = Selector::parse(&format!("meta[property='{key}'],meta[name='{key}']")).ok()?;
        doc.select(&s)
            .next()?
            .value()
            .attr("content")
            .map(str::to_owned)
    };
    let title = meta("og:title")
        .or_else(|| {
            doc.select(&Selector::parse("title").unwrap())
                .next()
                .map(|e| e.text().collect())
        })
        .unwrap_or_else(|| host.into());
    let content = ["article", "main", "[role=main]", "body"]
        .iter()
        .find_map(|s| {
            doc.select(&Selector::parse(s).unwrap())
                .next()
                .map(|e| e.html())
        })
        .unwrap_or_else(|| cleaned.into_owned());
    let markdown = html2md::parse_html(&content);
    let social = host == "x.com"
        || host.ends_with(".x.com")
        || host == "twitter.com"
        || host.ends_with(".twitter.com")
        || host == "reddit.com"
        || host.ends_with(".reddit.com");
    let description = meta("og:description")
        .or_else(|| meta("description"))
        .unwrap_or_default();
    let body = format!("# {title}\n\n{description}\n\n{markdown}");
    anyhow::ensure!(
        body.trim().len() > title.len() + 10,
        "Source has no readable content"
    );
    Ok(Hydrated {
        title,
        body,
        kind: if social { "social" } else { "link" }.into(),
        warning: if social {
            Some("Social extraction is best effort; login-only content may be unavailable.".into())
        } else {
            None
        },
    })
}
async fn youtube(url: &str) -> Result<Hydrated> {
    // yt-dlp handles changing YouTube player/caption signatures without embedding a browser.
    let bundled = std::env::current_exe()?.with_file_name("yt-dlp.exe");
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vendor/yt-dlp.exe");
    let reader = if bundled.exists() {
        bundled
    } else if development.exists() {
        development
    } else {
        PathBuf::from("yt-dlp")
    };
    let mut cmd = tokio::process::Command::new(reader);
    cmd.args([
        "--dump-single-json",
        "--skip-download",
        "--no-playlist",
        "--no-warnings",
        "--ignore-no-formats-error",
        "--",
        url,
    ])
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .context("YouTube metadata timed out")?
        .context(
            "YouTube reader could not start. Reinstall the bundled reader or put yt-dlp on PATH, then retry.",
        )?;
    anyhow::ensure!(
        output.status.success(),
        "YouTube metadata unavailable: {}",
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(500)
            .collect::<String>()
    );
    let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let title = data["title"].as_str().unwrap_or("YouTube video").to_owned();
    let mut body = format!(
        "# {title}\n\n{}",
        data["description"].as_str().unwrap_or("")
    );
    let mut warning =
        Some("No accessible English transcript; indexed title and description.".into());
    let captions = data["subtitles"]
        .as_object()
        .and_then(|m| m.get("en"))
        .or_else(|| {
            data["automatic_captions"]
                .as_object()
                .and_then(|m| m.get("en").or_else(|| m.get("en-orig")))
        });
    if let Some(track) = captions
        .and_then(|v| v.as_array())
        .and_then(|a| a.iter().find(|v| v["ext"] == "json3"))
    {
        if let Some(caption_url) = track["url"].as_str() {
            let result = async {
                let value: serde_json::Value = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()?
                    .get(caption_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                Ok::<_, anyhow::Error>(value)
            }
            .await;
            if let Ok(value) = result {
                let mut lines = vec![];
                if let Some(events) = value["events"].as_array() {
                    for event in events {
                        if let Some(segs) = event["segs"].as_array() {
                            let line = segs
                                .iter()
                                .filter_map(|s| s["utf8"].as_str())
                                .collect::<String>();
                            if !line.trim().is_empty() && lines.last() != Some(&line) {
                                lines.push(line)
                            }
                        }
                    }
                }
                if !lines.is_empty() {
                    body.push_str("\n\n## Transcript\n\n");
                    body.push_str(&lines.join("\n"));
                    warning = None
                }
            }
        }
    }
    Ok(Hydrated {
        title,
        body,
        kind: "video".into(),
        warning,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_preserve_unicode_and_overlap() {
        let text = "a é b c d";
        let offsets = vec![(0, 1), (2, 4), (5, 6), (7, 8), (9, 10)];
        assert_eq!(chunk_offsets(text, &offsets, 3, 1), vec!["a é b", "b c d"])
    }
    #[test]
    fn empty_text_has_no_chunks() {
        assert!(chunk_offsets("", &[], 510, 51).is_empty())
    }
    #[test]
    #[ignore = "Downloads the BGE model on first run; requires network and ONNX Runtime"]
    fn real_embedding_roundtrip() -> Result<()> {
        let d = tempfile::tempdir()?;
        let path = d.path().join("real.db");
        db::init(&path)?;
        let cache = std::env::var_os("NOTESAI_TEST_MODEL_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| d.path().join("models"));
        let engine = Embeddings::new(cache);
        let astronomy = db::capture(
            &path,
            "Planets orbit a star because of gravity. Orbital angular momentum is conserved.",
            vec![],
        )?;
        let cooking = db::capture(
            &path,
            "Bread dough rises when yeast produces carbon dioxide. Knead the flour and water.",
            vec![],
        )?;
        engine.index(&path, &astronomy)?;
        engine.index(&path, &cooking)?;
        let q = engine.query("Why do planets move around the sun?")?;
        assert_eq!(q.len(), 384);
        let vec = db::vector(&path, &q, 3)?;
        let lex = db::lexical(&path, "planets orbit", 3)?;
        let hits = db::hits(&path, db::rrf(&[lex, vec], 3))?;
        assert_eq!(hits[0].note_id, astronomy);
        assert_eq!(db::get(&path, &astronomy)?.status, "ready");
        Ok(())
    }
}
