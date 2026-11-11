use gloo_events::EventListener;
use gloo_render::{request_animation_frame, AnimationFrame};
use gloo_timers::callback::Timeout;
use pulse_core::{recipes::Recipe, spring::Spring, Settings, Snapshot};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    fn invoke_raw(command: &str, args: JsValue) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    fn listen_raw(event: &str, handler: &js_sys::Function) -> Result<js_sys::Promise, JsValue>;
}

async fn invoke<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, String> {
    let args = ipc_args(&args)?;
    let promise = invoke_raw(command,args).map_err(|_|"Desktop bridge is unavailable. Launch NotchPulse through Tauri.".to_owned())?;
    let value = JsFuture::from(promise).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

fn ipc_args(args: &Value) -> Result<JsValue, String> {
    // Tauri expects a plain JS object. The default serializer maps JSON object
    // values to JS Map, which JSON.stringify would silently turn into {}.
    args.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map_err(|e|e.to_string())
}

#[derive(Default)]
struct Ui {
    snapshot: Option<Snapshot>,
    recipe: Option<Recipe>,
    index: usize,
    settings_open: bool,
    confirm: bool,
    hovered: bool,
    expanded: bool,
    motion_reduced: bool,
    spring: Spring,
    last_frame: f64,
    frame: Option<AnimationFrame>,
    timeout: Option<Timeout>,
    listeners: Vec<EventListener>,
    message: String,
    busy: bool,
    generation: u64,
    snapshot_request: u64,
    recipe_request: u64,
}
type App = Rc<RefCell<Ui>>;

fn document() -> web_sys::Document { web_sys::window().expect("window").document().expect("document") }
fn el(id: &str) -> web_sys::Element { document().get_element_by_id(id).expect("UI element") }
fn escape(s: &str) -> String { s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;") }
fn action(name: &str, label: &str, class: &str) -> String {
    let accessible = match name { "prev" => " aria-label=\"Previous repository\"", "next" => " aria-label=\"Next repository\"", _ => "" };
    format!("<button data-action=\"{name}\"{accessible} class=\"{class}\">{label}</button>")
}

#[cfg_attr(not(test), wasm_bindgen(start))]
pub fn start() {
    let app = Rc::new(RefCell::new(Ui { generation: js_sys::Date::now() as u64, ..Ui::default() }));
    app.borrow_mut().motion_reduced = web_sys::window().and_then(|w|w.match_media("(prefers-reduced-motion: reduce)").ok().flatten()).is_some_and(|m|m.matches());
    render(&app);
    let enter = app.clone();
    let leave = app.clone();
    let click = app.clone();
    let key = app.clone();
    let focus = app.clone();
    let blur = app.clone();
    let listeners = vec![
        EventListener::new(&el("island"),"mouseenter",move |_| { enter.borrow_mut().hovered = true; expand(&enter,true); }),
        EventListener::new(&el("island"),"mouseleave",move |_| {
            leave.borrow_mut().hovered = false;
            delay_collapse(&leave,450);
        }),
        EventListener::new(&el("island"),"focusin",move |_| expand(&focus,true)),
        EventListener::new(&el("island"),"focusout",move |_| delay_collapse(&blur,450)),
        EventListener::new(&el("island"),"click",move |event| {
            let Some(target) = event.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()) else { return; };
            if target.id()=="compact" || target.closest("#compact").ok().flatten().is_some() { expand(&click,true); return; }
            if let Ok(Some(button)) = target.closest("[data-action]") {
                if let Some(name) = button.get_attribute("data-action") { handle_action(&click,&name); }
            }
        }),
        EventListener::new(&document(),"keydown",move |event| {
            let Some(e) = event.dyn_ref::<web_sys::KeyboardEvent>() else { return; };
            if e.key()=="Escape" {
                { let mut s = key.borrow_mut(); s.settings_open=false; s.confirm=false; }
                render(&key); expand(&key,false); load(&key,false); e.prevent_default();
                return;
            }
            let input = e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|t|["INPUT","SELECT"].contains(&t.tag_name().as_str()));
            if key.borrow().expanded && !input && ["ArrowLeft","ArrowRight"].contains(&e.key().as_str()) {
                handle_action(&key,if e.key()=="ArrowLeft" { "prev" } else { "next" }); e.prevent_default();
            }
        }),
    ];
    app.borrow_mut().listeners = listeners;
    for event in ["pulse:feed","pulse:error"] {
        let state = app.clone();
        let is_error = event=="pulse:error";
        let callback = Closure::<dyn FnMut(JsValue)>::new(move |value: JsValue| {
            if is_error {
                let payload = js_sys::Reflect::get(&value,&JsValue::from_str("payload")).ok().and_then(|v|v.as_string());
                show_message(&state,&payload.unwrap_or("Display positioning failed".into()));
            } else { load(&state,true); }
        });
        let promise = match listen_raw(event,callback.as_ref().unchecked_ref()) {
            Ok(promise)=>promise,
            Err(_)=>{ show_message(&app,"Desktop event bridge is unavailable"); continue; }
        };
        let state = app.clone();
        spawn_local(async move {
            if JsFuture::from(promise).await.is_err() { show_message(&state,"Could not subscribe to feed updates"); }
        });
        // One listener per app lifetime; no listeners accumulate on renders.
        callback.forget();
    }
    load(&app,false);
    // Permanent event closures retain the one application instance.
}

fn load(app: &App, incoming: bool) {
    let request = { let mut s=app.borrow_mut(); s.snapshot_request+=1; s.snapshot_request };
    let app = app.clone();
    spawn_local(async move {
        match invoke::<Snapshot>("snapshot",json!({})).await {
            Ok(snapshot) => {
                let blocked = { let s=app.borrow(); s.snapshot_request!=request || s.confirm || s.settings_open };
                if blocked { return; }
                let changed;
                {
                    let mut s = app.borrow_mut();
                    changed = incoming && snapshot.repositories.first().map(|r|r.id) != s.snapshot.as_ref().and_then(|s|s.repositories.first()).map(|r|r.id);
                    let selected = s.snapshot.as_ref().and_then(|old|old.repositories.get(s.index)).map(|r|r.id);
                    s.index = snapshot.repositories.iter().position(|r|Some(r.id)==selected).unwrap_or(0);
                    s.snapshot = Some(snapshot);
                    s.recipe = None;
                }
                // Preserve unsaved settings and confirmation while live updates arrive.
                if !app.borrow().settings_open && !app.borrow().confirm { render(&app); load_recipe(&app); }
                if changed && !app.borrow().snapshot.as_ref().is_some_and(|s|s.settings.paused) {
                    el("announcement").set_text_content(Some("New open-source signal received"));
                    expand(&app,true); delay_collapse(&app,6500);
                }
            }
            Err(e) => { if app.borrow().snapshot_request==request { show_message(&app,&e); } },
        }
    });
}

fn load_recipe(app: &App) {
    let (id, request) = {
        let mut s=app.borrow_mut();
        s.recipe_request+=1;
        s.recipe=None;
        (s.snapshot.as_ref().and_then(|snap|snap.repositories.get(s.index)).map(|r|r.id),s.recipe_request)
    };
    render(app);
    let Some(id) = id else { return; };
    let app = app.clone();
    spawn_local(async move {
        match invoke::<Recipe>("command_for",json!({"id":id})).await {
            Ok(recipe) => {
                let accepted = {
                    let s=app.borrow();
                    s.recipe_request==request && !s.settings_open && !s.confirm
                        && s.snapshot.as_ref().and_then(|snap|snap.repositories.get(s.index)).is_some_and(|r|r.id==id)
                };
                if accepted { app.borrow_mut().recipe=Some(recipe); render(&app); }
            }
            Err(e) => { if app.borrow().recipe_request==request { show_message(&app,&e); } },
        }
    });
}

fn render(app: &App) {
    let s = app.borrow();
    let Some(snapshot) = &s.snapshot else {
        el("content").set_inner_html(&format!("<header><span class=\"brand\">⌁ NotchPulse</span></header><div class=\"empty\"><h1>Tuning in.</h1><p>Finding your next great repository.</p><p>{}</p></div>",escape(&s.message)));
        return;
    };
    let header = format!("<header><span class=\"brand\"><i></i> NOTCHPULSE</span><div class=\"toolbar\">{}{}</div></header>",
        action("pause",if snapshot.settings.paused { "Resume" } else { "Pause" },"text-button"),
        action("settings",if s.settings_open { "Back" } else { "Settings" },"text-button"));
    let body = if s.settings_open { settings_html(&snapshot.settings,&snapshot.monitors) }
    else if let Some(repo) = snapshot.repositories.get(s.index) {
        let language = repo.language.as_deref().unwrap_or("Open source");
        let topics = repo.topics.iter().take(3).map(|t|format!("<span class=\"tag\">{}</span>",escape(t))).collect::<String>();
        let command = s.recipe.as_ref().map(|r|escape(&r.display)).unwrap_or("Preparing platform command…".into());
        let host = format!("{} / {}",snapshot.host.shell,snapshot.host.package_manager);
        let command_ui = if s.confirm {
            format!("<div class=\"review\"><b>Open a terminal and run this?</b><p>{}</p><code>{command}</code><div class=\"actions\">{}{}</div></div>",
                s.recipe.as_ref().map(|r|escape(&r.note)).unwrap_or_default(),action("cancel","Cancel","secondary"),action("run","Confirm &amp; run ↗","primary"))
        } else {
            format!("<div class=\"command-label\"><span>{}</span><span>LOCAL ENVIRONMENT</span></div><div class=\"command\"><code>{command}</code>{}</div><div class=\"actions\">{}{}</div>",
                escape(&host),action("copy","Copy","copy"),action("open","View repository ↗","secondary"),action("review","Run in terminal ↗","primary"))
        };
        format!("<div class=\"eyebrow\"><span class=\"live-dot\"></span> THE OPEN SOURCE SIGNAL <span class=\"count\">{:02} / {:02}</span></div><div class=\"repo-line\"><span class=\"repo-icon\">⌘</span><h1>{}</h1></div><p class=\"description\">{}</p><div class=\"metadata\"><span class=\"language\">● {}</span><span>☆ {:.1}k</span><div class=\"topics\">{topics}</div></div>{command_ui}",
            s.index+1,snapshot.repositories.len(),escape(&repo.full_name),escape(repo.description.as_deref().unwrap_or("Explore an extraordinary open-source project.")),escape(language),repo.stargazers_count as f64/1000.0)
    } else {
        format!("<div class=\"empty\"><span class=\"empty-icon\">⌁</span><h1>A little quiet here.</h1><p>{}</p><p>Only active projects with 20,000+ stars make the cut.</p>{}</div>",escape(&snapshot.status),action("refresh","Try again","secondary"))
    };
    let message = if s.message.is_empty() { &snapshot.status } else { &s.message };
    let footer = format!("<footer><span class=\"status\">{}</span><nav aria-label=\"Repository navigation\">{}{}</nav></footer>",escape(message),action("prev","‹","arrow"),action("next","›","arrow"));
    el("content").set_inner_html(&format!("{header}<div class=\"body\">{body}</div>{footer}"));
    let disabled = s.busy || s.recipe.is_none();
    if disabled {
        for name in ["copy","review","run"] {
            if let Ok(Some(e)) = el("content").query_selector(&format!("[data-action='{name}']")) { let _ = e.set_attribute("disabled",""); }
        }
    }
}

fn settings_html(s: &Settings, monitors: &[pulse_core::MonitorInfo]) -> String {
    let options = monitors.iter().map(|m| format!("<option value=\"{}\" {}>{}</option>",escape(&m.id),if s.monitor.as_ref()==Some(&m.id) { "selected" } else { "" },escape(&m.label))).collect::<String>();
    format!(r#"<div class="settings"><h2>Make yourself at home.</h2>
      <label>Display<select id="monitor"><option value="">Follow primary display</option>{options}</select></label>
      <div class="split"><label>Camera offset (points)<input id="offset" type="number" min="-4000" max="4000" value="{}"></label><label>Extra top inset<input id="inset" type="number" min="0" max="400" value="{}"></label></div>
      <label class="check"><input id="habits" type="checkbox" {}> Personalize using project manifest names</label>
      <input id="root" aria-label="Absolute project folder" placeholder="Absolute project folder" value="{}">
      <p class="privacy">Opt-in. Scans this folder and one level below. Only language labels are sent to GitHub; source and paths stay here.</p>
      <label class="check"><input id="motion" type="checkbox" {}> Reduce motion</label>
      <div class="actions">{}{}</div></div>"#,
      s.camera_offset_x,s.top_inset,if s.habits_enabled {"checked"} else {""},escape(&s.project_root),if s.reduced_motion {"checked"} else {""},action("quit","Quit","text-button"),action("save","Save preferences","primary"))
}

fn show_message(app: &App, message: &str) {
    app.borrow_mut().message=message.chars().take(220).collect();
    el("announcement").set_text_content(Some(message));
    // Do not replace form inputs while a user is typing.
    if app.borrow().settings_open {
        if let Ok(Some(status)) = el("content").query_selector(".status") { status.set_text_content(Some(message)); }
    } else { render(app); }
}

fn handle_action(app: &App, name: &str) {
    match name {
        "next" | "prev" => {
            if app.borrow().settings_open || app.borrow().confirm { return; }
            let mut s = app.borrow_mut();
            let len = s.snapshot.as_ref().map(|s|s.repositories.len()).unwrap_or(0);
            if len==0 { return; }
            s.index = (s.index+if name=="next" {1} else {len-1})%len;
            s.recipe=None; s.message.clear(); drop(s); render(app); load_recipe(app);
        }
        "settings" => {
            if app.borrow().busy { return; }
            let open={let mut s=app.borrow_mut(); s.settings_open=!s.settings_open; s.confirm=false; s.settings_open};
            render(app);
            if !open {load(app,false);}
        },
        "review" => { if app.borrow().recipe.is_some() { app.borrow_mut().confirm=true; render(app); } },
        "cancel" => { app.borrow_mut().confirm=false; render(app); load(app,false); },
        "save" => save(app),
        "pause" => {
            if app.borrow().busy || app.borrow().settings_open || app.borrow().confirm { return; }
            let Some(mut settings) = app.borrow().snapshot.as_ref().map(|s|s.settings.clone()) else { return; };
            settings.paused=!settings.paused;
            send_settings(app,settings);
        }
        "copy" | "open" | "run" | "refresh" | "quit" => {
            let id = { let s=app.borrow(); s.snapshot.as_ref().and_then(|snap|snap.repositories.get(s.index)).map(|r|r.id).unwrap_or(0) };
            if app.borrow().busy { return; }
            if name=="run" && !app.borrow().confirm { return; }
            let expected = app.borrow().recipe.as_ref().map(|r|r.display.clone()).unwrap_or_default();
            let (command,message) = match name {
                "copy" => ("copy_command","Command copied"),
                "open" => ("open_repository","Opened in browser"),
                "run" => ("execute_command","Terminal opened"),
                "refresh" => ("refresh_feed","Refresh requested · minimum interval 60s"),
                _ => ("quit_app","Closing…"),
            };
            app.borrow_mut().busy=true;
            let app=app.clone();
            spawn_local(async move {
                let result=invoke::<()>(command,json!({"id":id,"confirmed":true,"expectedCommand":expected})).await;
                { let mut s=app.borrow_mut(); s.busy=false; s.confirm=false; }
                match result { Ok(())=>show_message(&app,message),Err(e)=>show_message(&app,&e) }
            });
        }
        _=>{}
    }
}

fn input(id: &str) -> web_sys::HtmlInputElement { el(id).dyn_into().expect("input") }
fn save(app: &App) {
    if app.borrow().busy { return; }
    let Some(mut settings) = app.borrow().snapshot.as_ref().map(|s|s.settings.clone()) else { return; };
    let Ok(offset)=input("offset").value().parse() else { show_message(app,"Camera offset needs a number"); return; };
    let Ok(inset)=input("inset").value().parse() else { show_message(app,"Top inset needs a number"); return; };
    settings.camera_offset_x=offset; settings.top_inset=inset;
    settings.habits_enabled=input("habits").checked(); settings.project_root=input("root").value();
    settings.reduced_motion=input("motion").checked();
    let monitor: web_sys::HtmlSelectElement=el("monitor").dyn_into().expect("select");
    settings.monitor=if monitor.value().is_empty() {None} else {Some(monitor.value())};
    if let Err(e)=settings.validate() { show_message(app,&e); return; }
    send_settings(app,settings);
}
fn send_settings(app: &App, settings: Settings) {
    if app.borrow().busy {return;}
    app.borrow_mut().busy=true;
    if let Ok(Some(button))=el("content").query_selector("[data-action='save']") {let _=button.set_attribute("disabled","");}
    let app=app.clone();
    spawn_local(async move {
        let result=invoke::<()>("save_settings",json!({"settings":settings})).await;
        app.borrow_mut().busy=false;
        if let Ok(Some(button))=el("content").query_selector("[data-action='save']") {let _=button.remove_attribute("disabled");}
        match result {
            Ok(())=>{ app.borrow_mut().settings_open=false; show_message(&app,"Preferences saved"); load(&app,false); },
            Err(e)=>show_message(&app,&e),
        }
    });
}

fn delay_collapse(app: &App, ms: u32) {
    let weak=Rc::downgrade(app);
    app.borrow_mut().timeout=Some(Timeout::new(ms,move || {
        if let Some(app)=weak.upgrade() {
            let s=app.borrow();
            let focused=document().active_element().is_some_and(|e|e.id()!="compact" && el("content").contains(Some(&e)));
            let close=!s.hovered && !s.settings_open && !s.confirm && !focused;
            drop(s); if close { expand(&app,false); }
        }
    }));
}

fn expand(app: &App, expanded: bool) {
    { let mut s=app.borrow_mut();
      if s.expanded==expanded {return;}
      s.expanded=expanded; s.generation+=1;
    }
    let app=app.clone();
    let generation=app.borrow().generation;
    spawn_local(async move {
        if expanded {
            if let Err(e)=invoke::<()>("resize_island",json!({"expanded":true,"generation":generation})).await {
                if app.borrow().generation==generation {
                    app.borrow_mut().expanded=false;
                    show_message(&app,&e);
                    apply_visual_state(&app,false);
                    { let mut s=app.borrow_mut(); s.spring.target=0.0; }
                    schedule_frame(&app);
                }
                return;
            }
        }
        if app.borrow().generation!=generation {return;}
        apply_visual_state(&app,expanded);
        let reduced={let s=app.borrow();s.motion_reduced || s.snapshot.as_ref().is_some_and(|s|s.settings.reduced_motion)};
        {
            let mut s=app.borrow_mut(); s.spring.target=if expanded {1.0} else {0.0};
            if reduced {s.frame=None;s.spring.value=s.spring.target;s.spring.velocity=0.0;}
            s.last_frame=0.0;
        }
        if reduced { paint(&app); finish(&app); } else if app.borrow().frame.is_none() { schedule_frame(&app); }
    });
}
fn apply_visual_state(_app: &App, expanded: bool) {
    let _=el("compact").set_attribute("aria-expanded",if expanded {"true"} else {"false"});
    let _=el("compact").set_attribute("tabindex",if expanded {"-1"} else {"0"});
    let _=el("content").set_attribute("aria-hidden",if expanded {"false"} else {"true"});
    if expanded {let _=el("content").remove_attribute("inert");} else {let _=el("content").set_attribute("inert","");}
    let _=el("island").set_attribute("class",if expanded {"expanded"} else {""});
}
fn schedule_frame(app: &App) {
    let state=app.clone();
    app.borrow_mut().frame=Some(request_animation_frame(move |time| {
        let moving={let mut s=state.borrow_mut();s.frame=None;
            let dt=if s.last_frame==0.0 {1.0/60.0} else {(time-s.last_frame)/1000.0};
            s.last_frame=time;s.spring.advance(dt)};
        paint(&state);
        if moving {schedule_frame(&state);} else {finish(&state);}
    }));
}
fn paint(app: &App) {
    let p=app.borrow().spring.value;
    let sx=(208.0+(428.0-208.0)*p)/428.0;
    let sy=(44.0+(340.0-44.0)*p)/340.0;
    let surface: web_sys::HtmlElement=el("surface").dyn_into().expect("surface");
    let _=surface.style().set_property("transform",&format!("translateX(-50%) scale({sx},{sy})"));
    let _=surface.style().set_property("border-radius",&format!("{}px / {}px",26.0/sx,26.0/sy));
}
fn finish(app: &App) {
    if !app.borrow().expanded {
        let state=app.clone();let generation=app.borrow().generation;
        spawn_local(async move {
            if state.borrow().generation==generation {
                if let Err(e)=invoke::<()>("resize_island",json!({"expanded":false,"generation":generation})).await {show_message(&state,&e);}
            }
        });
    }
}

#[cfg(all(test, target_arch="wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn ipc_arguments_are_plain_json_objects() {
        let value=ipc_args(&json!({"id":42,"confirmed":true,"settings":{"paused":true}})).unwrap();
        assert!(!value.is_instance_of::<js_sys::Map>());
        assert_eq!(js_sys::Reflect::get(&value,&JsValue::from_str("id")).unwrap().as_f64(),Some(42.0));
        let serialized=js_sys::JSON::stringify(&value).unwrap().as_string().unwrap();
        let parsed: Value=serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["settings"]["paused"],true);
    }

    #[wasm_bindgen_test]
    fn escapes_untrusted_repository_metadata() {
        assert_eq!(escape("<img src=x onerror='bad()'>&\""),"&lt;img src=x onerror=&#39;bad()&#39;&gt;&amp;&quot;");
    }
}
