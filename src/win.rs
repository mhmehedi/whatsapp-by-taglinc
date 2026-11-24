// Windows pieces: one WebView2 environment, one WebView2 profile per account, toasts.

use crate::{UserEvent, APP_NAME};
use muda::{accelerator::Modifiers, Menu};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tao::{
    event_loop::{EventLoopBuilder, EventLoopProxy},
    platform::windows::{EventLoopBuilderExtWindows, IconExtWindows, WindowExtWindows},
    window::{Icon, Window, WindowBuilder},
};
use windows_sys::Win32::UI::{
    Shell::{SetCurrentProcessExplicitAppUserModelID, ShellExecuteW},
    WindowsAndMessaging::{MessageBoxW, TranslateAcceleratorW, MB_ICONWARNING, MB_OK, MSG, SW_SHOWNORMAL},
};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

pub const CMD: Modifiers = Modifiers::CONTROL;
pub const SIDEBAR_INIT: &str = "window.MOD='Ctrl+';";

/// Must match the AppUserModelID the installer puts on the Start menu shortcut, or Windows
/// won't show the app's toasts.
const APP_ID: &str = "TagLinc.WhatsappByTagLinc";
/// Profile folders of removed accounts that were still in use; deleted on next start.
const PENDING: &str = "pending-delete.txt";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into());
    base.join("WhatsappByTagLinc")
}

fn webview2_dir(dir: &Path) -> PathBuf {
    dir.join("WebView2")
}

fn profile_dir(dir: &Path, id: &str) -> PathBuf {
    webview2_dir(dir).join("EBWebView").join(id)
}

pub fn alert(title: &str, msg: &str) {
    let (t, m) = (wide(title), wide(msg));
    unsafe {
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
    }
}

/// Per-account profiles need WebView2 Runtime 101 or newer (any up-to-date Windows 10/11).
/// Without them every account would silently share one login.
pub fn check_runtime() -> Result<(), String> {
    const GET: &str = "Install the Microsoft Edge WebView2 Runtime from \
                       https://go.microsoft.com/fwlink/p/?LinkId=2124703 and open the app again.";
    match wry::webview_version() {
        Ok(v) => {
            let major: u32 = v.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0);
            if major >= 101 {
                Ok(())
            } else {
                Err(format!("Your WebView2 Runtime ({v}) is too old. {GET}"))
            }
        }
        Err(_) => Err(format!("{APP_NAME} needs the Microsoft Edge WebView2 Runtime. {GET}")),
    }
}

pub fn before_start(dir: &Path) {
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(wide(APP_ID).as_ptr());
    }
    // Finish deleting removed accounts whose files were locked last time.
    let list = dir.join(PENDING);
    if let Ok(s) = fs::read_to_string(&list) {
        let left: Vec<&str> = s
            .lines()
            .filter(|id| is_account_id(id) && fs::remove_dir_all(profile_dir(dir, id)).is_err())
            .filter(|id| profile_dir(dir, id).exists())
            .collect();
        let _ = if left.is_empty() { fs::remove_file(&list) } else { fs::write(&list, left.join("\n")) };
    }
}

fn is_account_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn hook_menu_keys(b: &mut EventLoopBuilder<UserEvent>, menu: &Menu) {
    let haccel = menu.haccel();
    b.with_msg_hook(move |msg| unsafe {
        let msg = msg as *const MSG;
        TranslateAcceleratorW((*msg).hwnd, haccel as _, msg) == 1
    });
}

pub fn window_extras(b: WindowBuilder) -> WindowBuilder {
    b.with_window_icon(Icon::from_resource(1, None).ok())
}

pub fn attach_menu(menu: &Menu, window: &Window) {
    unsafe {
        let _ = menu.init_for_hwnd(window.hwnd());
    }
}

pub fn after_sidebar(_dir: &Path) {}

pub fn web_context(dir: &Path) -> WebContext {
    WebContext::new(Some(webview2_dir(dir)))
}

pub fn account_builder<'a>(ctx: &'a mut WebContext, id: &str) -> WebViewBuilder<'a> {
    WebViewBuilder::new_with_web_context(ctx).with_profile_name(id)
}

pub fn remove_account_data(dir: &Path, id: &str) {
    if !is_account_id(id) {
        return;
    }
    // WebView2 may still hold the profile's files open; retry on next start if so.
    if fs::remove_dir_all(profile_dir(dir, id)).is_err() && profile_dir(dir, id).exists() {
        let list = dir.join(PENDING);
        let mut s = fs::read_to_string(&list).unwrap_or_default();
        s.push_str(id);
        s.push('\n');
        let _ = fs::write(list, s);
    }
}

pub fn open_external(url: &str) {
    let (op, u) = (wide("open"), wide(url));
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            u.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn notify(account: &str, title: &str, body: &str) {
    let (account, title, body) = (account.to_string(), title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let _ = tauri_winrt_notification::Toast::new(APP_ID)
            .title(&account)
            .text1(&title)
            .text2(&body)
            .show();
    });
}

pub fn edit_dialog(sidebar: &WebView, _proxy: &EventLoopProxy<UserEvent>, id: &str, name: &str) {
    let args = serde_json::json!([id, name]);
    let _ = sidebar.evaluate_script(&format!("window.editAccount(...{args})"));
}

pub fn close_page(_view: &WebView) {
    // Dropping a WebView2 controller closes its page and frees its renderer.
}

pub fn footprint_mb(_view: &WebView) -> Option<u64> {
    None // memory-based reloads are macOS-only for now
}
