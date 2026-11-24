// Whatsapp by TagLinc — a tiny macOS app that hosts several isolated WhatsApp Web sessions.
// Each account gets its own WKWebsiteDataStore (macOS 14+), so logins never mix.

mod webcrypto;

use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    NewWindowResponse, Rect, WebView, WebViewBuilder, WebViewBuilderExtDarwin, WebViewExtDarwin,
    WebViewExtMacOS,
};

const SIDEBAR_W: f64 = 64.0;
const APP_NAME: &str = "Whatsapp by TagLinc";
const WA_URL: &str = "https://web.whatsapp.com/";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                  (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

// Memory saving. WhatsApp Web keeps growing while it runs (chats, media); a reload brings an
// account back to ~350 MB. Accounts marked "sleep" are closed entirely after a while unused.
const RELOAD_ABOVE_MB: u64 = 700;
// Some accounts (big chat histories) need 1 GB+ right after loading; only reload when an
// account has grown this much past its own fresh-load size, so a reload actually helps.
const RELOAD_GROWTH_MB: u64 = 400;
const SETTLE: Duration = Duration::from_secs(2 * 60);
const MIN_RELOAD_GAP: Duration = Duration::from_secs(15 * 60);
const IDLE_BEFORE_RELOAD_ACTIVE: Duration = Duration::from_secs(10 * 60);
const SLEEP_AFTER: Duration = Duration::from_secs(30 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// Test overrides: MWA_RELOAD_MB, MWA_RELOAD_GROWTH_MB, MWA_RELOAD_GAP_SECS, MWA_SLEEP_SECS, MWA_TICK_SECS, MWA_START.
fn env_num(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

#[derive(Serialize, Deserialize, Clone)]
struct Account {
    id: String, // 32 hex chars = 16-byte data store identifier
    name: String,
    /// Close this account's WhatsApp when it hasn't been used for a while (saves memory,
    /// but it gets no notifications while asleep).
    #[serde(default)]
    sleep: bool,
}

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Sidebar(String),
    Title(String, String),
    Notify(String, String, String),
    Rename(String, String),
    Remove(String),
    Tick,
}

struct Slot {
    acc: Account,
    view: Option<WebView>, // None while asleep
    unread: u32,
    last_used: Instant,
    last_reload: Instant,
    /// Smallest memory use seen since the last (re)load, once the page settled.
    baseline_mb: Option<u64>,
}

impl Slot {
    fn new(acc: Account, view: Option<WebView>) -> Self {
        let now = Instant::now();
        Slot { acc, view, unread: 0, last_used: now, last_reload: now, baseline_mb: None }
    }
}

/// Process id of the WebKit content process that runs this webview's page.
fn web_pid(view: &WebView) -> Option<i32> {
    use objc2::{msg_send, sel};
    let wk = view.webview();
    let pid: i32 = unsafe {
        let ok: bool = msg_send![&*wk, respondsToSelector: sel!(_webProcessIdentifier)];
        if !ok {
            return None;
        }
        msg_send![&*wk, _webProcessIdentifier]
    };
    (pid > 0).then_some(pid)
}

/// Fully closes a webview's page so WebKit frees its memory. Dropping the WebView alone
/// leaves the page (and ~100+ MB) alive in WebKit's content process.
fn close_page(view: &WebView) {
    use objc2::{msg_send, sel};
    let wk = view.webview();
    unsafe {
        let ok: bool = msg_send![&*wk, respondsToSelector: sel!(_close)];
        if ok {
            let _: () = msg_send![&*wk, _close];
        } else {
            let _ = view.load_url("about:blank");
        }
    }
}

/// Physical memory (MB) of the WebKit process behind a webview, as Activity Monitor shows it.
fn footprint_mb(view: &WebView) -> Option<u64> {
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    }
    let pid = web_pid(view)?;
    // rusage_info_v2: uuid (2 words), then user/system time, wakeups x2, pageins,
    // wired, resident, phys_footprint (word 9).
    let mut buf = [0u64; 64];
    if unsafe { proc_pid_rusage(pid, 2, buf.as_mut_ptr()) } != 0 {
        return None;
    }
    Some(buf[9] / (1024 * 1024))
}

// ---------- persistence ----------

fn data_dir() -> PathBuf {
    let p = match std::env::var_os("MWA_DATA_DIR") {
        Some(d) => PathBuf::from(d), // test override
        None => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join("Library/Application Support/MultiWhatsApp")
        }
    };
    let _ = fs::create_dir_all(&p);
    p
}

fn load_accounts() -> Vec<Account> {
    fs::read_to_string(data_dir().join("accounts.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_accounts(slots: &[Slot]) {
    let accs: Vec<&Account> = slots.iter().map(|s| &s.acc).collect();
    if let Ok(s) = serde_json::to_string_pretty(&accs) {
        let _ = fs::write(data_dir().join("accounts.json"), s);
    }
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS random number generator unavailable");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Links clicked in WhatsApp go to the default browser/mail/phone app. Anything else
/// (file://, app-specific schemes, ...) is ignored so a chat message can't launch local files.
fn is_safe_external_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:", "tel:"].iter().any(|p| lower.starts_with(p))
}

fn id_bytes(id: &str) -> [u8; 16] {
    let mut b = [0u8; 16];
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = u8::from_str_radix(id.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0);
    }
    b
}

// ---------- helpers ----------

fn osascript(script: &str) -> Option<String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn as_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn content_rect(window: &Window) -> (f64, f64) {
    let size: LogicalSize<f64> = window.inner_size().to_logical(window.scale_factor());
    (size.width, size.height)
}

fn wa_bounds(window: &Window) -> Rect {
    let (w, h) = content_rect(window);
    Rect {
        position: LogicalPosition::new(SIDEBAR_W, 0.0).into(),
        size: LogicalSize::new((w - SIDEBAR_W).max(1.0), h).into(),
    }
}

fn sidebar_bounds(window: &Window) -> Rect {
    let (_, h) = content_rect(window);
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(SIDEBAR_W, h).into(),
    }
}

// Shim the Notification API (WKWebView lacks it) and forward to native notifications.
const NOTIFY_SHIM: &str = r#"
(function(){
  function N(title, opts){ opts = opts || {};
    try { window.ipc.postMessage(JSON.stringify({t:'notify', title:String(title||''), body:String(opts.body||'')})); } catch(e){}
    this.onclick = null; this.onclose = null; this.onerror = null; this.onshow = null; }
  N.prototype.close = function(){};
  N.prototype.addEventListener = function(){};
  N.permission = 'granted';
  N.requestPermission = function(cb){ if (cb) cb('granted'); return Promise.resolve('granted'); };
  try { Object.defineProperty(window, 'Notification', {value: N, writable: true, configurable: true}); } catch(e) { window.Notification = N; }
})();
"#;

fn wa_url() -> String {
    std::env::var("MWA_URL").unwrap_or_else(|_| WA_URL.into()) // test override
}

fn build_wa_view(window: &Window, acc: &Account, proxy: &EventLoopProxy<UserEvent>) -> WebView {
    let (p1, p2) = (proxy.clone(), proxy.clone());
    let (id1, id2) = (acc.id.clone(), acc.id.clone());
    WebViewBuilder::new()
        .with_data_store_identifier(id_bytes(&acc.id))
        .with_user_agent(UA)
        .with_url(wa_url())
        .with_bounds(wa_bounds(window))
        .with_visible(false)
        .with_initialization_script(NOTIFY_SHIM)
        .with_initialization_script(include_str!("resize.js"))
        .with_document_title_changed_handler(move |t| {
            let _ = p1.send_event(UserEvent::Title(id1.clone(), t));
        })
        .with_ipc_handler(move |req| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) {
                if v["t"] == "layout" {
                    // diagnostic from resize.js (element tags/classes/sizes only)
                    let _ = fs::write(
                        data_dir().join("layout-debug.json"),
                        serde_json::to_string_pretty(&v).unwrap_or_default(),
                    );
                } else if v["t"] == "notify" {
                    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                    let _ = p2.send_event(UserEvent::Notify(id2.clone(), s("title"), s("body")));
                }
            }
        })
        .with_new_window_req_handler(|url, _| {
            // External links (and "open in new tab") go to the default browser.
            if is_safe_external_url(&url) {
                let _ = Command::new("/usr/bin/open").arg(url.trim_start()).spawn();
            }
            NewWindowResponse::Deny
        })
        .build_as_child(window)
        .expect("failed to create WhatsApp webview")
}

const SIDEBAR_HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><style>
:root{color-scheme:light dark;--bg:#f0f2f5;--fg:#111b21;--tile:#fff;--act:#25d366}
@media (prefers-color-scheme:dark){:root{--bg:#111b21;--fg:#e9edef;--tile:#202c33}}
html,body{margin:0;height:100%;background:var(--bg);font:600 15px -apple-system,system-ui;user-select:none;-webkit-user-select:none;overflow:hidden}
#l{display:flex;flex-direction:column;align-items:center;gap:10px;padding:12px 0}
.a{position:relative;width:44px;height:44px;border-radius:12px;background:var(--tile);color:var(--fg);display:flex;align-items:center;justify-content:center;cursor:pointer;border:2px solid transparent;box-sizing:border-box}
.a.on{border-color:var(--act)}
.a .b{position:absolute;top:-6px;right:-6px;min-width:18px;height:18px;padding:0 4px;box-sizing:border-box;border-radius:9px;background:#e53935;color:#fff;font-size:11px;display:flex;align-items:center;justify-content:center}
.add{color:var(--act);font-size:26px;font-weight:400}
.a.z{opacity:.45}.a.z:after{content:'z';position:absolute;right:3px;bottom:0;font-size:10px;opacity:.8}
</style></head><body><div id="l"></div><script>
const send=m=>window.ipc.postMessage(m);
window.render=function(list){
  const l=document.getElementById('l');l.innerHTML='';
  list.forEach((a,i)=>{const d=document.createElement('div');d.className='a'+(a.active?' on':'')+(a.asleep?' z':'');
    d.title=a.name+'  (⌘'+(i+1)+')'+(a.asleep?'  — asleep, click to open':'')+'  — right-click to rename/remove';
    d.textContent=(a.name.trim()[0]||'?').toUpperCase();
    if(a.unread>0){const b=document.createElement('span');b.className='b';b.textContent=a.unread>99?'99+':a.unread;d.appendChild(b);}
    d.onclick=()=>send('select:'+a.id);
    d.oncontextmenu=e=>{e.preventDefault();send('edit:'+a.id);};
    l.appendChild(d);});
  const p=document.createElement('div');p.className='a add';p.textContent='+';p.title='Add account (⌘N)';
  p.onclick=()=>send('add');l.appendChild(p);
};
send('ready');
</script></body></html>"#;

struct App {
    window: Window,
    sidebar: WebView,
    slots: Vec<Slot>,
    active: usize,
    proxy: EventLoopProxy<UserEvent>,
    sleep_item: CheckMenuItem,
    /// When the window last lost focus (None while it's in front).
    away_since: Option<Instant>,
}

impl App {
    fn refresh_sidebar(&self) {
        let list: Vec<serde_json::Value> = self
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| {
                serde_json::json!({"id": s.acc.id, "name": s.acc.name, "unread": s.unread,
                                   "active": i == self.active, "asleep": s.view.is_none()})
            })
            .collect();
        let _ = self.sidebar.evaluate_script(&format!(
            "window.render && window.render({})",
            serde_json::Value::Array(list)
        ));
        let total: u32 = self.slots.iter().map(|s| s.unread).sum();
        let title = match self.slots.get(self.active) {
            Some(s) if total > 0 => format!("{APP_NAME} — {} ({total})", s.acc.name),
            Some(s) => format!("{APP_NAME} — {}", s.acc.name),
            None => APP_NAME.into(),
        };
        self.window.set_title(&title);
    }

    fn select(&mut self, idx: usize) {
        if idx >= self.slots.len() {
            return;
        }
        if let Some(prev) = self.slots.get_mut(self.active) {
            prev.last_used = Instant::now();
        }
        self.active = idx;
        self.wake(idx);
        for (i, s) in self.slots.iter().enumerate() {
            if let Some(v) = &s.view {
                let _ = v.set_visible(i == idx);
            }
        }
        if let Some(v) = &self.slots[idx].view {
            let _ = v.focus();
        }
        self.slots[idx].last_used = Instant::now();
        self.sync_sleep_check();
        self.refresh_sidebar();
    }

    fn wake(&mut self, i: usize) {
        if self.slots[i].view.is_none() {
            if std::env::var_os("MWA_DEBUG").is_some() {
                eprintln!("MWA_DEBUG: wake {} (active {})", self.slots[i].acc.name, self.active);
            }
            let view = build_wa_view(&self.window, &self.slots[i].acc, &self.proxy);
            let s = &mut self.slots[i];
            s.view = Some(view);
            s.last_reload = Instant::now();
            s.baseline_mb = None;
        }
    }

    fn sync_sleep_check(&self) {
        let on = self.slots.get(self.active).is_some_and(|s| s.acc.sleep);
        self.sleep_item.set_checked(on);
    }

    fn toggle_sleep(&mut self) {
        let Some(s) = self.slots.get_mut(self.active) else { return };
        s.acc.sleep = !s.acc.sleep;
        save_accounts(&self.slots);
        self.sync_sleep_check();
    }

    /// Runs every minute: put idle "sleep" accounts to sleep, reload accounts that grew too big.
    fn tick(&mut self) {
        let debug = std::env::var_os("MWA_DEBUG").is_some();
        let limit = env_num("MWA_RELOAD_MB").unwrap_or(RELOAD_ABOVE_MB);
        let sleep_after = env_num("MWA_SLEEP_SECS").map(Duration::from_secs).unwrap_or(SLEEP_AFTER);
        let growth = env_num("MWA_RELOAD_GROWTH_MB").unwrap_or(RELOAD_GROWTH_MB);
        let gap = env_num("MWA_RELOAD_GAP_SECS").map(Duration::from_secs).unwrap_or(MIN_RELOAD_GAP);
        let away = self.away_since.is_some_and(|t| t.elapsed() >= IDLE_BEFORE_RELOAD_ACTIVE);
        let mut changed = false;
        for (i, s) in self.slots.iter_mut().enumerate() {
            let Some(view) = &s.view else { continue };
            let active = i == self.active;
            if !active && s.acc.sleep && s.last_used.elapsed() >= sleep_after {
                if debug {
                    eprintln!("MWA_DEBUG: sleep {}", s.acc.name);
                }
                if let Some(v) = s.view.take() {
                    close_page(&v);
                }
                s.unread = 0;
                changed = true;
                continue;
            }
            let mb = footprint_mb(view);
            if debug {
                eprintln!("MWA_DEBUG: {} uses {:?} MB (pid {:?}, baseline {:?})", s.acc.name, mb, web_pid(view), s.baseline_mb);
            }
            // The visible account is only reloaded once you've been away from the app a while.
            let idle = !active || away;
            let Some(mb) = mb else { continue };
            if s.last_reload.elapsed() >= SETTLE.min(gap) {
                s.baseline_mb = Some(s.baseline_mb.map_or(mb, |b| b.min(mb)));
            }
            let grown = s.baseline_mb.is_some_and(|b| mb >= b + growth);
            if mb > limit && grown && idle && s.last_reload.elapsed() >= gap {
                if debug {
                    eprintln!("MWA_DEBUG: reload {} ({:?} MB)", s.acc.name, mb);
                }
                let _ = view.load_url(&wa_url());
                s.last_reload = Instant::now();
                s.baseline_mb = None;
            }
        }
        if changed {
            self.refresh_sidebar();
        }
    }

    fn add(&mut self) {
        let acc = Account { id: new_id(), name: format!("Account {}", self.slots.len() + 1), sleep: false };
        let view = build_wa_view(&self.window, &acc, &self.proxy);
        self.slots.push(Slot::new(acc, Some(view)));
        save_accounts(&self.slots);
        self.select(self.slots.len() - 1);
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.acc.id == id)
    }

    fn edit(&self, id: &str) {
        let Some(i) = self.index_of(id) else { return };
        let name = self.slots[i].acc.name.clone();
        let (id, proxy) = (id.to_string(), self.proxy.clone());
        // Dialogs run in a thread so the UI stays responsive.
        std::thread::spawn(move || {
            let script = format!(
                "display dialog \"Account name:\" default answer {} buttons {{\"Remove…\", \"Cancel\", \"Save\"}} default button \"Save\" cancel button \"Cancel\" with title {}",
                as_str(&name),
                as_str(APP_NAME)
            );
            let Some(out) = osascript(&script) else { return };
            if out.starts_with("button returned:Remove") {
                let confirm = format!(
                    "display dialog {} buttons {{\"Cancel\", \"Remove\"}} default button \"Cancel\" cancel button \"Cancel\" with icon caution with title {}",
                    as_str(&format!("Remove \"{name}\"? Its WhatsApp login and local data will be deleted.")),
                    as_str(APP_NAME)
                );
                if osascript(&confirm).is_some() {
                    let _ = proxy.send_event(UserEvent::Remove(id));
                }
            } else if let Some(t) = out.split("text returned:").nth(1) {
                let t = t.trim();
                if !t.is_empty() {
                    let _ = proxy.send_event(UserEvent::Rename(id, t.to_string()));
                }
            }
        });
    }

    fn remove(&mut self, id: &str) {
        let Some(i) = self.index_of(id) else { return };
        let slot = self.slots.remove(i);
        let bytes = id_bytes(&slot.acc.id);
        if let Some(v) = &slot.view {
            close_page(v);
        }
        drop(slot.view); // must drop before removing the data store
        WebView::remove_data_store(&bytes, |_| {});
        save_accounts(&self.slots);
        if self.slots.is_empty() {
            self.add();
        } else {
            // the removed account must not count as "previously active"
            self.active = self.active.min(self.slots.len() - 1);
            self.select(self.active);
        }
    }

    fn layout(&self) {
        let _ = self.sidebar.set_bounds(sidebar_bounds(&self.window));
        let b = wa_bounds(&self.window);
        for v in self.slots.iter().filter_map(|s| s.view.as_ref()) {
            let _ = v.set_bounds(b);
        }
    }
}

fn build_menu() -> (Menu, MenuItem, MenuItem, MenuItem, CheckMenuItem, Vec<MenuItem>) {
    let menu = Menu::new();
    let app_m = Submenu::new(APP_NAME, true);
    let _ = app_m.append_items(&[
        &PredefinedMenuItem::about(None, None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::quit(None),
    ]);
    let edit_m = Submenu::new("Edit", true);
    let _ = edit_m.append_items(&[
        &PredefinedMenuItem::undo(None),
        &PredefinedMenuItem::redo(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
    ]);
    let cmd = Modifiers::META;
    let add = MenuItem::new("New Account", true, Some(Accelerator::new(cmd, Code::KeyN)));
    let reload = MenuItem::new("Reload", true, Some(Accelerator::new(cmd, Code::KeyR)));
    let rename = MenuItem::new("Rename / Remove…", true, None);
    let sleep = CheckMenuItem::new("Sleep When Not Used (saves memory, no notifications)", true, false, None);
    let digits = [
        Code::Digit1, Code::Digit2, Code::Digit3, Code::Digit4, Code::Digit5,
        Code::Digit6, Code::Digit7, Code::Digit8, Code::Digit9,
    ];
    let switch: Vec<MenuItem> = digits
        .iter()
        .enumerate()
        .map(|(i, c)| MenuItem::new(format!("Account {}", i + 1), true, Some(Accelerator::new(cmd, *c))))
        .collect();
    let acc_m = Submenu::new("Account", true);
    let _ = acc_m.append_items(&[&add, &reload, &rename, &sleep, &PredefinedMenuItem::separator()]);
    for s in &switch {
        let _ = acc_m.append(s);
    }
    let win_m = Submenu::new("Window", true);
    let _ = win_m.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::fullscreen(None),
        &PredefinedMenuItem::close_window(None),
    ]);
    let _ = menu.append_items(&[&app_m, &edit_m, &acc_m, &win_m]);
    (menu, add, reload, rename, sleep, switch)
}

fn main() {
    // Only one instance may touch the WhatsApp databases at a time; two writers corrupt them.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir().join("app.lock"))
        .expect("cannot open lock file");
    if lock.try_lock().is_err() {
        osascript(&format!("display alert {} message \"Close the other copy first. Running two copies at once can log you out of WhatsApp.\"", as_str(&format!("{APP_NAME} is already running."))));
        return;
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let (menu, m_add, m_reload, m_edit, m_sleep, m_switch) = build_menu();
    menu.init_for_nsapp();
    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(UserEvent::Menu(e));
        }));
    }

    let window = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(LogicalSize::new(1200.0, 820.0))
        .with_min_inner_size(LogicalSize::new(600.0, 400.0))
        .build(&event_loop)
        .expect("failed to create window");

    let sp = proxy.clone();
    let sidebar = WebViewBuilder::new()
        .with_html(SIDEBAR_HTML)
        .with_bounds(sidebar_bounds(&window))
        .with_ipc_handler(move |req| {
            let _ = sp.send_event(UserEvent::Sidebar(req.body().clone()));
        })
        .build_as_child(&window)
        .expect("failed to create sidebar");

    // The sidebar build registered wry's delegate class; hook the WebCrypto master key
    // before any WhatsApp webview exists (see webcrypto.rs).
    if std::env::var_os("MWA_NO_HOOK").is_some() {
        // test override: behave like the old build (Keychain-backed master key)
    } else if !webcrypto::install(webcrypto::load_or_create(&data_dir())) {
        eprintln!("warning: could not install WebCrypto master key hook; falling back to Keychain");
    }

    let mut accounts = load_accounts();
    if accounts.is_empty() {
        accounts.push(Account { id: new_id(), name: "Account 1".into(), sleep: false });
    }
    // "Sleep" accounts start asleep (except the first, which is shown) and wake on click.
    let slots: Vec<Slot> = accounts
        .into_iter()
        .enumerate()
        .map(|(i, acc)| {
            let view = (i == 0 || !acc.sleep).then(|| build_wa_view(&window, &acc, &proxy));
            Slot::new(acc, view)
        })
        .collect();

    {
        let p = proxy.clone();
        let every = env_num("MWA_TICK_SECS").map(Duration::from_secs).unwrap_or(CHECK_EVERY);
        std::thread::spawn(move || loop {
            std::thread::sleep(every);
            if p.send_event(UserEvent::Tick).is_err() {
                break;
            }
        });
    }

    let mut app = App { window, sidebar, slots, active: 0, proxy, sleep_item: m_sleep.clone(), away_since: None };
    save_accounts(&app.slots);
    let start = env_num("MWA_START").unwrap_or(0) as usize;
    app.select(if start < app.slots.len() { start } else { 0 });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit
            }
            Event::WindowEvent { event: WindowEvent::Resized(_), .. }
            | Event::WindowEvent { event: WindowEvent::ScaleFactorChanged { .. }, .. } => app.layout(),
            Event::WindowEvent { event: WindowEvent::Focused(f), .. } => {
                app.away_since = if f { None } else { Some(Instant::now()) };
            }
            Event::UserEvent(ev) => match ev {
                UserEvent::Sidebar(msg) => {
                    if msg == "ready" {
                        app.refresh_sidebar();
                    } else if msg == "add" {
                        app.add();
                    } else if let Some(id) = msg.strip_prefix("select:") {
                        if let Some(i) = app.index_of(id) {
                            app.select(i);
                        }
                    } else if let Some(id) = msg.strip_prefix("edit:") {
                        app.edit(id);
                    }
                }
                UserEvent::Menu(e) => {
                    if e.id == *m_add.id() {
                        app.add();
                    } else if e.id == *m_reload.id() {
                        if let Some(s) = app.slots.get_mut(app.active) {
                            if let Some(v) = &s.view {
                                let _ = v.load_url(&wa_url());
                            }
                            s.last_reload = Instant::now();
                            s.baseline_mb = None;
                        }
                    } else if e.id == *m_sleep.id() {
                        app.toggle_sleep();
                    } else if e.id == *m_edit.id() {
                        if let Some(s) = app.slots.get(app.active) {
                            app.edit(&s.acc.id.clone());
                        }
                    } else if let Some(i) = m_switch.iter().position(|m| *m.id() == e.id) {
                        app.select(i);
                    }
                }
                UserEvent::Title(id, title) => {
                    if std::env::var_os("MWA_DEBUG").is_some() {
                        eprintln!("MWA_DEBUG: title {title}");
                    }
                    // WhatsApp sets the title to "(N) WhatsApp" when there are unread chats.
                    let n = title
                        .strip_prefix('(')
                        .and_then(|r| r.split(')').next())
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(0);
                    if let Some(i) = app.index_of(&id) {
                        if app.slots[i].unread != n {
                            app.slots[i].unread = n;
                            app.refresh_sidebar();
                        }
                    }
                }
                UserEvent::Notify(id, title, body) => {
                    let Some(i) = app.index_of(&id) else { return };
                    if i == app.active && app.window.is_focused() {
                        return;
                    }
                    let script = format!(
                        "display notification {} with title {} subtitle {}",
                        as_str(&body),
                        as_str(&app.slots[i].acc.name),
                        as_str(&title)
                    );
                    std::thread::spawn(move || {
                        osascript(&script);
                    });
                }
                UserEvent::Rename(id, name) => {
                    if let Some(i) = app.index_of(&id) {
                        app.slots[i].acc.name = name;
                        save_accounts(&app.slots);
                        app.refresh_sidebar();
                    }
                }
                UserEvent::Remove(id) => app.remove(&id),
                UserEvent::Tick => app.tick(),
            },
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{is_safe_external_url, new_id};

    #[test]
    fn allows_web_mail_and_phone_links() {
        for u in ["https://example.com", "http://x.y/a?b=c", "HTTPS://EXAMPLE.COM", "mailto:a@b.c", "tel:+8801"] {
            assert!(is_safe_external_url(u), "{u}");
        }
    }

    #[test]
    fn rejects_local_and_other_schemes() {
        for u in ["file:///Applications/Calculator.app", "-a Calculator", "javascript:alert(1)",
                  "x-apple.systempreferences:", "smb://host/share", "", "ftp://x"] {
            assert!(!is_safe_external_url(u), "{u}");
        }
    }

    #[test]
    fn ids_are_random_hex() {
        let (a, b) = (new_id(), new_id());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
