use pulse_core::{geometry::{anchor, MonitorRect}, MonitorInfo, Settings};
use tauri::{AppHandle, Manager, PhysicalPosition};

pub fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let Some(w) = app.get_webview_window("main") else { return vec![]; };
    let primary = w.primary_monitor().ok().flatten();
    w.available_monitors().unwrap_or_default().into_iter().map(|m| {
        let p = m.position();
        let id = format!("{}@{},{}", m.name().cloned().unwrap_or_default(), p.x, p.y);
        MonitorInfo { id, label: format!("{} · {}×{} · {:.0}%", m.name().map(String::as_str).unwrap_or("Display"), m.size().width, m.size().height, m.scale_factor()*100.0),
            primary: primary.as_ref().is_some_and(|s| s.position() == p) }
    }).collect()
}

pub fn snap(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or("Window unavailable")?;
    let all = w.available_monitors().map_err(|e| e.to_string())?;
    let selected = all.into_iter().find(|m| {
        let p = m.position();
        settings.monitor.as_ref().is_some_and(|id| id == &format!("{}@{},{}", m.name().cloned().unwrap_or_default(), p.x, p.y))
    });
    let primary = w.primary_monitor().map_err(|e| e.to_string())?;
    let m = selected.or_else(|| primary.clone()).ok_or("No monitor detected")?;
    let is_primary = primary.is_some_and(|p| p.position() == m.position());
    let inset = settings.top_inset + if is_primary { native_primary_inset() } else { 0.0 };
    let p = m.position();
    let size = w.inner_size().map_err(|e| e.to_string())?;
    let current_scale = w.scale_factor().map_err(|e| e.to_string())?;
    let (x,y) = anchor(MonitorRect { x:p.x, y:p.y, width:m.size().width, height:m.size().height, scale:m.scale_factor() }, f64::from(size.width)/current_scale, f64::from(size.height)/current_scale,settings.camera_offset_x,inset);
    if w.outer_position().ok() != Some(PhysicalPosition::new(x,y)) {
        w.set_position(PhysicalPosition::new(x,y)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_primary_inset() -> f64 {
    // NSScreen's first screen contains the menu bar. Read AppKit on the main
    // thread only. min macOS is 12, where safeAreaInsets was introduced.
    let Some(mtm) = objc2::MainThreadMarker::new() else { return 0.0; };
    let screens = objc2_app_kit::NSScreen::screens(mtm);
    screens.firstObject().map(|s| s.safeAreaInsets().top).unwrap_or(0.0)
}
#[cfg(not(target_os = "macos"))]
fn native_primary_inset() -> f64 { 0.0 }

