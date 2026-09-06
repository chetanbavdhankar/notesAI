mod backup;
mod db;
mod discovery;
mod drive;
mod google_auth;
mod ingest;
mod llm;
mod streaming;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{ipc::Channel, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

struct AppState {
    backup: Arc<backup::Service>,
    db: PathBuf,
    settings: PathBuf,
    embeddings: Arc<ingest::Embeddings>,
    client: reqwest::Client,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    settings_lock: Mutex<()>,
}
type ApiResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[tauri::command]
async fn list_notes(state: State<'_, Arc<AppState>>) -> ApiResult<Vec<db::Note>> {
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::list(&path))
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
async fn capture(
    text: String,
    tags: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<String> {
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::capture(&path, &text, tags))
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
async fn edit_note(
    id: String,
    title: String,
    body: String,
    tags: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<()> {
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::edit(&path, &id, &title, &body, tags))
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
async fn delete_note(id: String, state: State<'_, Arc<AppState>>) -> ApiResult<()> {
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::remove(&path, &id))
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
fn retry_note(id: String, state: State<'_, Arc<AppState>>) -> ApiResult<()> {
    let note = db::get(&state.db, &id).map_err(error)?;
    db::status(
        &state.db,
        &id,
        if note.source_url.as_deref() == Some(note.body.trim()) {
            "queued"
        } else {
            "indexing"
        },
        None,
    )
    .map_err(error)
}
#[tauri::command]
fn get_settings(state: State<'_, Arc<AppState>>) -> ApiResult<llm::Settings> {
    let _guard = state.settings_lock.lock().map_err(error)?;
    llm::load(&state.settings).map_err(error)
}
#[tauri::command]
fn save_settings(settings: llm::Settings, state: State<'_, Arc<AppState>>) -> ApiResult<()> {
    let _guard = state.settings_lock.lock().map_err(error)?;
    llm::save(&state.settings, &settings).map_err(error)
}
#[tauri::command]
async fn discover_models(
    profile: llm::Profile,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<discovery::Discovery> {
    discovery::discover(&state.client, &profile)
        .await
        .map_err(error)
}
#[tauri::command]
fn backup_status(state: State<'_, Arc<AppState>>) -> ApiResult<backup::Status> {
    state.backup.status().map_err(error)
}
#[tauri::command]
async fn google_connect(
    client_id: String,
    client_secret: String,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<backup::Status> {
    state
        .backup
        .connect(client_id, client_secret)
        .await
        .map_err(error)
}
#[tauri::command]
async fn google_disconnect(state: State<'_, Arc<AppState>>) -> ApiResult<backup::Status> {
    state.backup.disconnect().await.map_err(error)
}
#[tauri::command]
fn google_cancel_connect(state: State<'_, Arc<AppState>>) {
    state.backup.oauth_cancel.store(true, Ordering::SeqCst);
}
#[tauri::command]
async fn backup_now(
    config: backup::Config,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<backup::Status> {
    state.backup.backup(&config, true).await.map_err(error)
}
#[tauri::command]
async fn backup_export(state: State<'_, Arc<AppState>>) -> ApiResult<Option<backup::Status>> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Export NotesAI backup")
        .set_file_name("notesai-backup.json.gz")
        .save_file()
        .await;
    match file {
        Some(file) => state
            .backup
            .export(file.path())
            .await
            .map(Some)
            .map_err(error),
        None => Ok(None),
    }
}
#[tauri::command]
async fn backup_restore(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<Option<backup::Restored>> {
    let file = rfd::AsyncFileDialog::new()
        .set_title("Restore NotesAI backup — existing notes are kept")
        .add_filter("NotesAI backup", &["gz"])
        .pick_file()
        .await;
    match file {
        Some(file) => {
            let restored = state.backup.import(file.path()).await.map_err(error)?;
            let _ = app.emit("notes-changed", ());
            Ok(Some(restored))
        }
        None => Ok(None),
    }
}
async fn retrieve(state: Arc<AppState>, query: String, k: usize) -> anyhow::Result<Vec<db::Hit>> {
    let path = state.db.clone();
    let q = query.clone();
    let lex = tauri::async_runtime::spawn_blocking(move || db::lexical(&path, &q, k * 4));
    let s = state.clone();
    let vec = tauri::async_runtime::spawn_blocking(move || {
        let embedding = s.embeddings.query(&query)?;
        db::vector(&s.db, &embedding, k * 4)
    });
    let (lex, vec) = tokio::join!(lex, vec);
    let ranks = db::rrf(&[lex??, vec??], k);
    let path = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db::hits(&path, ranks)).await?
}
#[tauri::command]
async fn search(
    query: String,
    top_k: usize,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<Vec<db::Hit>> {
    retrieve(state.inner().clone(), query, top_k.clamp(3, 15))
        .await
        .map_err(error)
}
#[tauri::command]
fn cancel_chat(request_id: String, state: State<'_, Arc<AppState>>) -> ApiResult<()> {
    if let Some(cancel) = state.cancellations.lock().map_err(error)?.get(&request_id) {
        cancel.store(true, Ordering::Relaxed)
    }
    Ok(())
}
#[tauri::command]
async fn chat(
    question: String,
    history: Vec<llm::Message>,
    request_id: String,
    on_event: Channel<llm::ChatEvent>,
    state: State<'_, Arc<AppState>>,
) -> ApiResult<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut map = state.cancellations.lock().map_err(error)?;
        if !map.is_empty() {
            return Err("Another answer is already running".into());
        }
        map.insert(request_id.clone(), cancel.clone());
    }
    let s = state.inner().clone();
    let result=async{
        let settings=llm::load(&s.settings)?;llm::validate(&settings)?;
        let profile=settings.profiles.iter().find(|p|p.id==settings.active_profile_id).ok_or_else(||anyhow::anyhow!("Choose a model profile"))?;
        anyhow::ensure!(!profile.model_name.trim().is_empty(),"Choose a model in Settings");
        let hits=retrieve(s.clone(),question.clone(),settings.top_k).await?;
        anyhow::ensure!(!cancel.load(Ordering::Relaxed),"Generation stopped");
        let (messages,sources,output)=llm::assemble(profile,&question,&history,hits)?;
        let count=sources.len();llm::emit(&on_event,&request_id,"sources",None,Some(sources))?;
        if count==0{llm::emit(&on_event,&request_id,"token",Some("I couldn't find indexed sources for that question. Capture relevant material, wait for indexing, and try again.".into()),None)?;llm::emit(&on_event,&request_id,"done",None,None)?;return Ok(())}
        llm::stream(&s.client,profile,messages,output,&request_id,&on_event,cancel,count).await
    }.await;
    state
        .cancellations
        .lock()
        .map_err(error)?
        .remove(&request_id);
    if let Err(e) = result {
        let message = error(e);
        let _ = llm::emit(&on_event, &request_id, "error", Some(message.clone()), None);
        return Err(message);
    }
    Ok(())
}
#[tauri::command]
fn open_source(url: String) -> ApiResult<()> {
    let parsed = url::Url::parse(&url).map_err(error)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only HTTP(S) links can be opened".into());
    }
    // Pass a URL as a process argument, never interpolate it into shell code.
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(parsed.as_str())
            .spawn()
            .map_err(error)?;
    }
    Ok(())
}
async fn worker(app: tauri::AppHandle, state: Arc<AppState>) {
    loop {
        let path = state.db.clone();
        let next=tauri::async_runtime::spawn_blocking(move||->anyhow::Result<Option<(String,String,i64)>>{let db=db::open(&path)?;use rusqlite::OptionalExtension;Ok(db.query_row("SELECT id,status,revision FROM notes WHERE status IN ('queued','hydrating','indexing') ORDER BY created_at LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)}).await;
        if let Ok(Ok(Some((id, status, revision)))) = next {
            let result=async{
                let note=db::get(&state.db,&id)?;
                if matches!(status.as_str(),"queued"|"hydrating"){
                    db::status(&state.db,&id,"hydrating",None)?;let _=app.emit("notes-changed",());
                    if let Some(url)=note.source_url.as_deref(){
                        let hydrated=ingest::hydrate(&state.client,url).await?;
                        db::open(&state.db)?.execute("UPDATE notes SET title=?,body=?,kind=?,error=?,status='indexing' WHERE id=? AND revision=?",rusqlite::params![hydrated.title,hydrated.body,hydrated.kind,hydrated.warning,id,revision])?;
                    }else{db::status(&state.db,&id,"indexing",None)?}
                }
                let _=app.emit("notes-changed",());let s=state.clone();let key=id.clone();
                tauri::async_runtime::spawn_blocking(move||s.embeddings.index(&s.db,&key)).await??;Ok::<_,anyhow::Error>(())
            }.await;
            if let Err(e) = result {
                let _ = db::open(&state.db).and_then(|db| {
                    Ok(db.execute(
                        "UPDATE notes SET status='failed',error=? WHERE id=? AND revision=?",
                        rusqlite::params![e.to_string(), id, revision],
                    )?)
                });
            }
            let _ = app.emit("notes-changed", ());
        } else {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await
        }
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<Arc<AppState>>();
                        let result =
                            app.clipboard().read_text().map_err(error).and_then(|text| {
                                db::capture(&state.db, &text, vec![]).map_err(error)
                            });
                        let (title, body) = match result {
                            Ok(_) => (
                                "Captured",
                                "Saved locally. Indexing in the background.".into(),
                            ),
                            Err(e) => ("Capture failed", e),
                        };
                        let _ = app.notification().builder().title(title).body(body).show();
                        let _ = app.emit("notes-changed", ());
                    });
                })
                .build(),
        )
        .setup(|app| {
            let dir = std::env::var_os("NOTESAI_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("knowledge.db");
            db::init(&path)?;
            let state = Arc::new(AppState {
                backup: Arc::new(backup::Service::new(dir.clone(), path.clone())?),
                db: path,
                settings: dir.join("settings.json"),
                embeddings: Arc::new(ingest::Embeddings::new(
                    std::env::var_os("NOTESAI_MODEL_CACHE")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| dir.join("models")),
                )),
                client: reqwest::Client::builder()
                    .user_agent("NotesAI/0.1 personal knowledge reader")
                    .timeout(std::time::Duration::from_secs(120))
                    .redirect(reqwest::redirect::Policy::limited(5))
                    .build()?,
                cancellations: Mutex::new(HashMap::new()),
                settings_lock: Mutex::new(()),
            });
            app.manage(state.clone());
            for shortcut in ["Super+Alt+S", "Ctrl+Shift+C"] {
                if let Err(e) = app.global_shortcut().register(shortcut) {
                    let _ = app
                        .notification()
                        .builder()
                        .title("Shortcut unavailable")
                        .body(format!("{shortcut}: {e}"))
                        .show();
                }
            }
            use tauri::menu::{Menu, MenuItem};
            let show = MenuItem::with_id(app, "show", "Open NotesAI", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit NotesAI", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = tauri::tray::TrayIconBuilder::new();
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.tooltip("NotesAI — capture is ready")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            tauri::async_runtime::spawn(backup::scheduled(
                state.backup.clone(),
                state.settings.clone(),
            ));
            tauri::async_runtime::spawn(worker(app.handle().clone(), state));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            backup_status,
            google_connect,
            google_disconnect,
            google_cancel_connect,
            backup_now,
            backup_export,
            backup_restore,
            list_notes,
            capture,
            edit_note,
            delete_note,
            retry_note,
            get_settings,
            save_settings,
            discover_models,
            search,
            chat,
            cancel_chat,
            open_source
        ])
        .run(tauri::generate_context!())
        .expect("Unable to start NotesAI");
}
