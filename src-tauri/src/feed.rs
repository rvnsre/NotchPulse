use crate::{storage, AppState};
use pulse_core::{curate, schedule::RefreshSchedule, Repository};
use reqwest::{header::{HeaderMap, HeaderValue, AUTHORIZATION, ETAG, IF_NONE_MATCH}, StatusCode};
use serde::{Deserialize, Serialize};
use std::{sync::{atomic::Ordering, Arc}, time::{Duration, SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Emitter};

pub fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    pub repositories: Vec<Repository>,
    pub etag: Option<String>,
    pub query: String,
    pub updated_at: u64,
}

#[derive(Deserialize)]
struct Search {
    items: Vec<Repository>,
    #[serde(default)]
    incomplete_results: bool,
}

struct FetchError { message: String, retry_after: u64 }

pub fn client() -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert("Accept", HeaderValue::from_static("application/vnd.github+json"));
    headers.insert("X-GitHub-Api-Version", HeaderValue::from_static("2022-11-28"));
    if let Ok(token) = std::env::var("NOTCHPULSE_GITHUB_TOKEN") {
        if let Ok(mut value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
    }
    reqwest::Client::builder().user_agent("NotchPulse/0.1.0")
        .https_only(true).redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20)).default_headers(headers).build()
}

async fn fetch(client: &reqwest::Client, old: &Cache, languages: &[String]) -> Result<Cache, FetchError> {
    let language = languages.first().filter(|s| ["Rust", "Go", "Python", "TypeScript", "Java", "C++", "Swift"].contains(&s.as_str()));
    let query = format!("stars:>=20000 archived:false fork:false{}", language.map(|l| format!(" language:{l}")).unwrap_or_default());
    let mut request = client.get("https://api.github.com/search/repositories")
        .query(&[("q", query.as_str()), ("sort", "updated"), ("order", "desc"), ("per_page", "30")]);
    let conditional = old.query == query && old.etag.is_some();
    if conditional {
        if let Some(etag) = &old.etag { request = request.header(IF_NONE_MATCH, etag); }
    }
    let mut response = request.send().await.map_err(|_| FetchError {
        message: "Offline - showing last saved feed".into(), retry_after: 0,
    })?;
    if response.status() == StatusCode::NOT_MODIFIED && conditional {
        return Ok(Cache { updated_at: now(), ..old.clone() });
    }
    if !response.status().is_success() {
        let reset = response.headers().get("x-ratelimit-reset").and_then(|h| h.to_str().ok())
            .and_then(|h| h.parse::<u64>().ok()).unwrap_or(0);
        let retry = response.headers().get("retry-after").and_then(|h| h.to_str().ok())
            .and_then(|h| h.parse::<u64>().ok()).unwrap_or(0);
        let limited = matches!(response.status(), StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS);
        return Err(FetchError {
            message: format!("Cached - GitHub HTTP {}", response.status().as_u16()),
            retry_after: retry.max(if limited { reset.saturating_sub(now()) } else { 0 }).min(86_400),
        });
    }
    let etag = response.headers().get(ETAG).and_then(|h| h.to_str().ok()).map(str::to_owned);
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if bytes.len().saturating_add(chunk.len()) <= 1_048_576 => bytes.extend_from_slice(&chunk),
            Ok(None) => break,
            _ => return Err(FetchError { message: "Cached - response interrupted or oversized".into(), retry_after: 0 }),
        }
    }
    let search = serde_json::from_slice::<Search>(&bytes).map_err(|_| FetchError {
        message: "Cached - invalid GitHub response".into(), retry_after: 0,
    })?;
    if search.incomplete_results {
        return Err(FetchError { message: "Cached - GitHub returned incomplete results".into(), retry_after: 60 });
    }
    Ok(Cache { repositories: curate(search.items, languages), query, etag, updated_at: now() })
}

pub async fn worker(app: AppHandle, state: Arc<AppState>) {
    let epoch = tokio::time::Instant::now();
    let mut schedule = RefreshSchedule::default();
    loop {
        let (snapshot, profile_revision) = {
            let _guard = state.update.lock().await;
            (state.snapshot.read().await.clone(), state.profile_revision.load(Ordering::Acquire))
        };
        if snapshot.settings.paused {
            state.wake.notified().await;
            schedule.request(epoch.elapsed().as_secs());
            continue;
        }
        let wait = schedule.wait_seconds(epoch.elapsed().as_secs());
        if wait > 0 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait)) => {},
                _ = state.wake.notified() => schedule.request(epoch.elapsed().as_secs()),
            }
            continue;
        }
        let cache = state.cache.lock().await.clone();
        let result = fetch(&state.client, &cache, &snapshot.languages).await;
        let seconds = epoch.elapsed().as_secs();
        let (new_cache, mut status) = match result {
            Ok(cache) => {
                schedule.succeeded(seconds, now() % 61);
                (Some(cache), "Live - refreshed just now".to_owned())
            }
            Err(error) => {
                schedule.failed(seconds, error.retry_after);
                (None, error.message)
            }
        };
        let profile_guard = state.update.lock().await;
        if profile_revision != state.profile_revision.load(Ordering::Acquire) {
            schedule.request(seconds);
            continue;
        }
        if let Some(cache) = new_cache {
            let path = state.data_dir.join("feed.json");
            let save = cache.clone();
            let saved = tauri::async_runtime::spawn_blocking(move || storage::write(&path, &save)).await;
            if !matches!(saved, Ok(Ok(()))) { status = "Live - disk cache could not be saved".into(); }
            {
                let mut snapshot = state.snapshot.write().await;
                snapshot.repositories = cache.repositories.clone();
                snapshot.updated_at = cache.updated_at;
            }
            *state.cache.lock().await = cache;
        }
        state.snapshot.write().await.status = status;
        drop(profile_guard);
        let _ = app.emit("pulse:feed", ());
    }
}
