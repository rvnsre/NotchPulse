#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod feed;
mod habits;
mod platform;
mod position;
mod storage;

use pulse_core::{recipes::{self, Recipe}, Settings, Snapshot};
use std::{path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Arc}};
use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::{Mutex, Notify, RwLock};

pub struct AppState {
    snapshot: RwLock<Snapshot>,
    cache: Mutex<feed::Cache>,
    client: reqwest::Client,
    data_dir: PathBuf,
    wake: Notify,
    update: Mutex<()>,
    last_execution: Mutex<Option<tokio::time::Instant>>,
    profile_revision: AtomicU64,
    layout_generation: AtomicU64,
}

#[tauri::command]
async fn snapshot(app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<Snapshot, String> {
    let mut snapshot = state.snapshot.read().await.clone();
    snapshot.monitors = position::monitors(&app);
    #[cfg(target_os="linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        snapshot.status.push_str(" · Wayland placement is compositor-controlled");
    }
    Ok(snapshot)
}

async fn selected_recipe(state: &AppState, id: u64) -> Result<Recipe, String> {
    let s = state.snapshot.read().await;
    let repo = s.repositories.iter().find(|r| r.id == id && r.eligible()).ok_or("This card is no longer in the feed")?;
    recipes::recipe(&repo.full_name, &s.host)
}

#[tauri::command]
async fn command_for(id: u64, state: tauri::State<'_, Arc<AppState>>) -> Result<Recipe, String> {
    selected_recipe(&state, id).await
}

#[tauri::command]
async fn copy_command(id: u64, app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    app.clipboard().write_text(selected_recipe(&state, id).await?.display).map_err(|e| e.to_string())
}

#[tauri::command]
async fn execute_command(id: u64, confirmed: bool, expected_command: String, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    if !confirmed { return Err("Review and confirm this command first".into()); }
    let recipe = selected_recipe(&state, id).await?;
    if recipe.display != expected_command { return Err("This command changed. Review it again before running.".into()); }
    {
        let mut last = state.last_execution.lock().await;
        if last.as_ref().is_some_and(|time|time.elapsed() < std::time::Duration::from_secs(3)) {
            return Err("Please wait before opening another terminal".into());
        }
        *last=Some(tokio::time::Instant::now());
    }
    let host = state.snapshot.read().await.host.clone();
    tauri::async_runtime::spawn_blocking(move || platform::execute(&recipe, &host)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_repository(id: u64, app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let s = state.snapshot.read().await;
    let repo = s.repositories.iter().find(|r| r.id == id && r.eligible()).ok_or("Card is no longer available")?;
    app.opener().open_url(repo.url(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn refresh_feed(state: tauri::State<'_, Arc<AppState>>) { state.wake.notify_one(); }

#[tauri::command]
async fn save_settings(settings: Settings, app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    settings.validate()?;
    let _guard = state.update.lock().await;
    let previous = state.snapshot.read().await.clone();
    let pause_changed = previous.settings.paused != settings.paused;
    let profile_changed = previous.settings.habits_enabled != settings.habits_enabled
        || previous.settings.project_root != settings.project_root;
    let languages = if !profile_changed {
        previous.languages
    } else if settings.habits_enabled {
        let root = settings.project_root.clone();
        tauri::async_runtime::spawn_blocking(move || habits::languages(&root)).await.map_err(|e| e.to_string())??
    } else { vec![] };
    let path = state.data_dir.join("settings.json");
    let save = settings.clone();
    tauri::async_runtime::spawn_blocking(move || storage::write(&path, &save)).await.map_err(|e| e.to_string())??;
    if profile_changed { state.profile_revision.fetch_add(1, Ordering::AcqRel); }
    {
        let mut s = state.snapshot.write().await;
        s.settings = settings.clone(); s.languages = languages;
    }
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = position::snap(&app2, &settings) { let _ = app2.emit("pulse:error", e); }
    }).map_err(|e| e.to_string())?;
    if profile_changed || pause_changed { state.wake.notify_one(); }
    let _ = app.emit("pulse:feed", ());
    Ok(())
}

#[tauri::command]
async fn resize_island(expanded: bool, generation: u64, app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let settings = state.snapshot.read().await.settings.clone();
    let app2 = app.clone();
    let state = state.inner().clone();
    // Complete the native resize before the UI starts its transform animation.
    let (tx,rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let result = (|| {
            // Resize messages may arrive out of order after rapid hover reversal.
            if generation < state.layout_generation.load(Ordering::Acquire) { return Ok(()); }
            state.layout_generation.store(generation, Ordering::Release);
            let window = app2.get_webview_window("main").ok_or("Window unavailable")?;
            let (w,h) = if expanded { (460.0,380.0) } else { (228.0,60.0) };
            window.set_size(tauri::LogicalSize::new(w,h)).map_err(|e| e.to_string())?;
            position::snap(&app2, &settings)
        })();
        let _ = tx.send(result);
    }).map_err(|e| e.to_string())?;
    rx.await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) { app.exit(0); }

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![snapshot, command_for, copy_command, execute_command, open_repository, refresh_feed, save_settings, resize_island, quit_app])
        .setup(|app| {
            #[cfg(target_os="macos")]
            app.handle().set_activation_policy(tauri::ActivationPolicy::Accessory)?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let settings = storage::read::<Settings>(&data_dir.join("settings.json"))
                .filter(|s| s.validate().is_ok()).unwrap_or_default();
            let mut cache = storage::read::<feed::Cache>(&data_dir.join("feed.json")).unwrap_or_default();
            cache.repositories = pulse_core::curate(cache.repositories, &[]);
            // Defer optional filesystem inspection until after the island is
            // visible. A slow project volume must not block window creation.
            let startup_root = settings.habits_enabled.then(|| settings.project_root.clone());
            let state = Arc::new(AppState {
                snapshot: RwLock::new(Snapshot { repositories: cache.repositories.clone(), host: platform::detect(), settings: settings.clone(), monitors: vec![], languages: vec![],
                    status: if cache.updated_at>0 { "Cached · connecting to GitHub" } else { "Connecting to GitHub…" }.into(), updated_at:cache.updated_at }),
                cache: Mutex::new(cache), client:feed::client()?, data_dir, wake:Notify::new(), update:Mutex::new(()), last_execution:Mutex::new(None),
                profile_revision:AtomicU64::new(0), layout_generation:AtomicU64::new(0),
            });
            app.manage(state.clone());
            let window = app.get_webview_window("main").ok_or("Main window missing")?;
            window.set_size(tauri::LogicalSize::new(228.0,60.0))?;
            position::snap(app.handle(), &settings).map_err(std::io::Error::other)?;
            window.show()?;
            let show = MenuItem::with_id(app,"show","Show NotchPulse",true,None::<&str>)?;
            let refresh = MenuItem::with_id(app,"refresh","Refresh feed",true,None::<&str>)?;
            let quit = MenuItem::with_id(app,"quit","Quit NotchPulse",true,None::<&str>)?;
            let menu = Menu::with_items(app,&[&show,&refresh,&quit])?;
            TrayIconBuilder::new().icon(tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))?)
                .tooltip("NotchPulse · extraordinary open source").menu(&menu)
                .on_menu_event(|app,event| match event.id.as_ref() {
                    "show" => { if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); } },
                    "refresh" => app.state::<Arc<AppState>>().wake.notify_one(),
                    "quit" => app.exit(0), _ => {}
                }).build(app)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Some(root) = startup_root {
                    let scan_root = root.clone();
                    if let Ok(Ok(languages)) = tauri::async_runtime::spawn_blocking(move || habits::languages(&scan_root)).await {
                        let _guard = state.update.lock().await;
                        let mut snapshot = state.snapshot.write().await;
                        if state.profile_revision.load(Ordering::Acquire) == 0
                            && snapshot.settings.habits_enabled && snapshot.settings.project_root == root {
                            snapshot.languages = languages;
                        }
                    }
                }
                feed::worker(handle,state).await;
            });
            Ok(())
        })
        .on_window_event(|w,event| {
            if matches!(event, tauri::WindowEvent::ScaleFactorChanged { .. } | tauri::WindowEvent::Moved(_)) {
                let app = w.app_handle().clone();
                let Some(state) = app.try_state::<Arc<AppState>>().map(|s| s.inner().clone()) else { return; };
                tauri::async_runtime::spawn(async move {
                    let settings = state.snapshot.read().await.settings.clone();
                    let a = app.clone(); let _ = app.run_on_main_thread(move || { let _ = position::snap(&a,&settings); });
                });
            }
        })
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|_,_| {}),
        Err(e) => { eprintln!("NotchPulse could not start: {e}"); std::process::exit(1); }
    }
}
