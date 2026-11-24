// macOS pieces: WKWebView data stores, AppleScript dialogs and notifications, WebKit memory.

use crate::{webcrypto, UserEvent, APP_NAME};
use muda::{accelerator::Modifiers, Menu};
use std::{path::Path, path::PathBuf, process::Command};
use tao::{
    event_loop::{EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtDarwin, WebViewExtDarwin, WebViewExtMacOS};

pub const CMD: Modifiers = Modifiers::META;
pub const SIDEBAR_INIT: &str = "";

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                  (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

pub fn default_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Library/Application Support/MultiWhatsApp")
}

fn id_bytes(id: &str) -> [u8; 16] {
    let mut b = [0u8; 16];
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = u8::from_str_radix(id.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0);
    }
    b
}

fn osascript(script: &str) -> Option<String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn as_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn alert(title: &str, msg: &str) {
    osascript(&format!("display alert {} message {}", as_str(title), as_str(msg)));
}

pub fn check_runtime() -> Result<(), String> {
    Ok(())
}

pub fn before_start(_dir: &Path) {}

pub fn hook_menu_keys(_b: &mut EventLoopBuilder<UserEvent>, _menu: &Menu) {}

pub fn window_extras(b: WindowBuilder) -> WindowBuilder {
    b
}

pub fn attach_menu(menu: &Menu, _window: &Window) {
    menu.init_for_nsapp();
}

pub fn after_sidebar(dir: &Path) {
    if std::env::var_os("MWA_NO_HOOK").is_some() {
        // test override: behave like the old build (Keychain-backed master key)
    } else if !webcrypto::install(webcrypto::load_or_create(dir)) {
        eprintln!("warning: could not install WebCrypto master key hook; falling back to Keychain");
    }
}

pub fn web_context(_dir: &Path) -> WebContext {
    WebContext::new(None) // unused on macOS: accounts are separated by data store ids
}

pub fn account_builder<'a>(_ctx: &'a mut WebContext, id: &str) -> WebViewBuilder<'a> {
    WebViewBuilder::new().with_data_store_identifier(id_bytes(id)).with_user_agent(UA)
}

pub fn remove_account_data(_dir: &Path, id: &str) {
    WebView::remove_data_store(&id_bytes(id), |_| {});
}

pub fn open_external(url: &str) {
    let _ = Command::new("/usr/bin/open").arg(url).spawn();
}

pub fn notify(account: &str, title: &str, body: &str) {
    let script = format!(
        "display notification {} with title {} subtitle {}",
        as_str(body),
        as_str(account),
        as_str(title)
    );
    std::thread::spawn(move || {
        osascript(&script);
    });
}

pub fn edit_dialog(_sidebar: &WebView, proxy: &EventLoopProxy<UserEvent>, id: &str, name: &str) {
    let (id, name, proxy) = (id.to_string(), name.to_string(), proxy.clone());
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
pub fn close_page(view: &WebView) {
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
pub fn footprint_mb(view: &WebView) -> Option<u64> {
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
