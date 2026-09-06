use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, sync::Once};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    pub kind: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub status: String,
    pub error: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub organized: bool,
    #[serde(default)]
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub chunk_id: i64,
    pub note_id: String,
    pub title: String,
    pub text: String,
    pub source_url: Option<String>,
    pub created_at: String,
    pub score: f64,
}
static INIT: Once = Once::new();
pub fn open(path: &Path) -> Result<Connection> {
    INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
    let db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(10))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
    Ok(db)
}
pub fn init(path: &Path) -> Result<()> {
    open(path)?.execute_batch(include_str!("schema.sql"))?;
    Ok(())
}
fn read_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        source_url: row.get(3)?,
        kind: row.get(4)?,
        tags: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        created_at: row.get(6)?,
        status: row.get(7)?,
        error: row.get(8)?,
        topics: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
        organized: row.get(10)?,
        revision: row.get(11)?,
    })
}
pub fn list(path: &Path) -> Result<Vec<Note>> {
    let db = open(path)?;
    let mut stmt=db.prepare("SELECT n.id,title,body,source_url,kind,tags,created_at,status,error,COALESCE(t.topics,'[]'),COALESCE(t.reviewed_revision=n.revision,0),n.revision FROM notes n LEFT JOIN note_topics t ON t.note_id=n.id ORDER BY created_at DESC")?;
    let notes = stmt
        .query_map([], read_note)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(notes)
}
pub fn get(path: &Path, id: &str) -> Result<Note> {
    Ok(open(path)?.query_row(
        "SELECT n.id,title,body,source_url,kind,tags,created_at,status,error,COALESCE(t.topics,'[]'),COALESCE(t.reviewed_revision=n.revision,0),n.revision FROM notes n LEFT JOIN note_topics t ON t.note_id=n.id WHERE n.id=?",
        [id],
        read_note,
    )?)
}
pub fn capture(path: &Path, text: &str, tags: Vec<String>) -> Result<String> {
    anyhow::ensure!(!text.trim().is_empty(), "Clipboard or capture is empty");
    anyhow::ensure!(text.len() <= 2_000_000, "Capture exceeds 2 MB");
    let id = uuid::Uuid::new_v4().to_string();
    let source = url::Url::parse(text.trim())
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .map(|u| u.to_string());
    let title = text
        .lines()
        .next()
        .unwrap_or("Untitled")
        .chars()
        .take(100)
        .collect::<String>();
    open(path)?.execute("INSERT INTO notes(id,title,body,source_url,kind,tags,created_at,status) VALUES(?,?,?,?,?,?,?,'queued')",
        params![id,title,text,source,if source.is_some(){"link"}else{"note"},serde_json::to_string(&tags)?,chrono::Utc::now().to_rfc3339()])?;
    Ok(id)
}
pub fn status(path: &Path, id: &str, status: &str, error: Option<&str>) -> Result<()> {
    open(path)?.execute(
        "UPDATE notes SET status=?,error=? WHERE id=?",
        params![status, error, id],
    )?;
    Ok(())
}
pub fn clear_chunks(db: &Connection, id: &str) -> Result<()> {
    db.execute(
        "DELETE FROM chunk_vectors WHERE rowid IN (SELECT id FROM chunks WHERE note_id=?)",
        [id],
    )?;
    db.execute("DELETE FROM chunks WHERE note_id=?", [id])?;
    Ok(())
}
pub fn edit(path: &Path, id: &str, title: &str, body: &str, tags: Vec<String>) -> Result<()> {
    anyhow::ensure!(
        !body.trim().is_empty() && body.len() <= 2_000_000,
        "Note must contain 1 byte to 2 MB of text"
    );
    let mut db = open(path)?;
    let tx = db.transaction()?;
    clear_chunks(&tx, id)?;
    tx.execute("UPDATE notes SET title=?,body=?,tags=?,status='indexing',error=NULL,revision=revision+1 WHERE id=?",params![title,body,serde_json::to_string(&tags)?,id])?;
    tx.commit()?;
    Ok(())
}
pub fn remove(path: &Path, id: &str) -> Result<()> {
    let mut db = open(path)?;
    let tx = db.transaction()?;
    clear_chunks(&tx, id)?;
    tx.execute("DELETE FROM notes WHERE id=?", [id])?;
    tx.commit()?;
    Ok(())
}
pub fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}
pub fn lexical(path: &Path, query: &str, k: usize) -> Result<Vec<i64>> {
    let tokens = query
        .split_whitespace()
        .take(64)
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(vec![]);
    }
    let db = open(path)?;
    let mut stmt=db.prepare("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY bm25(chunks_fts),rowid LIMIT ?")?;
    let rows = stmt
        .query_map(params![tokens.join(" OR "), k as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn vector(path: &Path, embedding: &[f32], k: usize) -> Result<Vec<i64>> {
    let db = open(path)?;
    let mut stmt = db.prepare(
        "SELECT rowid FROM chunk_vectors WHERE embedding MATCH ? AND k=? ORDER BY distance",
    )?;
    let rows = stmt
        .query_map(params![bytes(embedding), k as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn scoped_lexical(path: &Path, query: &str, k: usize, topic: &str) -> Result<Vec<i64>> {
    let terms = query
        .split_whitespace()
        .take(64)
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let db = open(path)?;
    let mut stmt=db.prepare("SELECT f.rowid FROM chunks_fts f JOIN chunks c ON c.id=f.rowid WHERE chunks_fts MATCH ? AND EXISTS(SELECT 1 FROM note_topics t,json_each(t.topics) j WHERE t.note_id=c.note_id AND j.value=?) ORDER BY bm25(chunks_fts),f.rowid LIMIT ?")?;
    let rows = stmt
        .query_map(params![terms, topic, k as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn scoped_vector(path: &Path, embedding: &[f32], k: usize, topic: &str) -> Result<Vec<i64>> {
    let db = open(path)?;
    let mut stmt=db.prepare("SELECT v.rowid FROM chunk_vectors v JOIN chunks c ON c.id=v.rowid WHERE EXISTS(SELECT 1 FROM note_topics t,json_each(t.topics) j WHERE t.note_id=c.note_id AND j.value=?) ORDER BY vec_distance_cosine(v.embedding,?),v.rowid LIMIT ?")?;
    let rows = stmt
        .query_map(params![topic, bytes(embedding), k as i64], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn rrf(lists: &[Vec<i64>], k: usize) -> Vec<(i64, f64)> {
    let mut scores = HashMap::<i64, f64>::new();
    for list in lists {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(*id).or_default() += 1.0 / (60.0 + rank as f64 + 1.0)
        }
    }
    let mut results = scores.into_iter().collect::<Vec<_>>();
    results.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    results.truncate(k);
    results
}
pub fn hits(path: &Path, ranks: Vec<(i64, f64)>) -> Result<Vec<Hit>> {
    let db = open(path)?;
    ranks.into_iter().map(|(id,score)|Ok(db.query_row("SELECT c.id,n.id,n.title,c.text,n.source_url,n.created_at FROM chunks c JOIN notes n ON c.note_id=n.id WHERE c.id=?",[id],|r|Ok(Hit{chunk_id:r.get(0)?,note_id:r.get(1)?,title:r.get(2)?,text:r.get(3)?,source_url:r.get(4)?,created_at:r.get(5)?,score}))?)).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fusion_rewards_agreement_and_is_stable() {
        let r = rrf(&[vec![1, 2, 3], vec![2, 4, 1]], 3);
        assert_eq!(r[0].0, 2);
        assert_eq!(r[1].0, 1);
        assert_eq!(r.len(), 3)
    }
    #[test]
    fn sqlite_indexes_and_delete_are_atomic() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.db");
        init(&path)?;
        let id = capture(&path, "a searchable note", vec!["test".into()])?;
        let db = open(&path)?;
        db.execute(
            "INSERT INTO chunks(note_id,ordinal,text) VALUES(?,0,'unique knowledge')",
            [&id],
        )?;
        let row = db.last_insert_rowid();
        let mut emb = vec![0f32; 384];
        emb[0] = 1.0;
        db.execute(
            "INSERT INTO chunk_vectors(rowid,embedding) VALUES(?,?)",
            params![row, bytes(&emb)],
        )?;
        assert_eq!(lexical(&path, "unique", 5)?, vec![row]);
        assert_eq!(vector(&path, &emb, 5)?, vec![row]);
        crate::organize::apply(&path, &id, 0, vec!["Science".into()])?;
        assert_eq!(scoped_lexical(&path, "unique", 5, "Science")?, vec![row]);
        assert_eq!(scoped_vector(&path, &emb, 5, "Science")?, vec![row]);
        assert!(scoped_lexical(&path, "unique", 5, "Travel")?.is_empty());
        assert!(scoped_vector(&path, &emb, 5, "Travel")?.is_empty());
        assert!(lexical(&path, "\" OR *", 5).is_ok());
        remove(&path, &id)?;
        assert!(lexical(&path, "unique", 5)?.is_empty());
        assert!(vector(&path, &emb, 5)?.is_empty());
        Ok(())
    }
}
