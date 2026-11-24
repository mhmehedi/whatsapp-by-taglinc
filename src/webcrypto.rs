// WebKit encrypts persisted WebCrypto keys (WhatsApp Web stores its session keys this way)
// with a "master key". By default WebKit keeps that key in the login Keychain, and macOS
// only lets the exact build that created it read it. Every rebuild/update of an ad-hoc
// signed app is a "new app", so the user gets a Keychain prompt; if it is denied, WebKit
// can no longer decrypt the stored keys and WhatsApp shows "database error" and logs out.
//
// Fix: answer WebKit's `_webCryptoMasterKeyForWebView:` navigation-delegate request with a
// key this app stores itself, so the Keychain is never involved again.

use base64::Engine;
use objc2::{
    ffi,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Imp, Sel},
    sel,
};
use objc2_foundation::{NSBundle, NSData};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::Command,
    sync::OnceLock,
};

/// WebKit names its Keychain item after the bundle id (or the process name when unbundled).
fn keychain_account() -> String {
    let id = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|s| s.to_string())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .unwrap_or_default();
    format!("com.apple.WebKit.WebCrypto.master+{id}")
}
static KEY: OnceLock<Vec<u8>> = OnceLock::new();

/// Decodes the value `security find-generic-password -w` prints for WebKit's item.
/// WebKit stores the key base64-encoded; `security` prints non-text data as hex.
fn decode_keychain_value(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() >= 32 && s.len() % 2 == 0 && s.bytes().all(|c| c.is_ascii_hexdigit()) {
        let raw: Vec<u8> = (0..s.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect();
        // Hex of base64 text (e.g. if stored with a trailing newline) → decode again.
        if let Ok(text) = std::str::from_utf8(&raw) {
            if let Some(b) = decode_keychain_value(text).filter(|_| text.trim() != s) {
                return Some(b);
            }
        }
        return Some(raw);
    }
    match base64::engine::general_purpose::STANDARD.decode(s) {
        Ok(b) if b.len() >= 16 => Some(b),
        _ => None,
    }
}

/// One-time migration: reuse WebKit's existing Keychain key so current logins keep working.
fn migrate_from_keychain() -> Option<Vec<u8>> {
    let out = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-a", &keychain_account(), "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    decode_keychain_value(&String::from_utf8_lossy(&out.stdout))
}

fn random_key() -> Vec<u8> {
    let mut b = vec![0u8; 16];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b
}

/// Loads the app's own master key, creating it (migrating from Keychain if possible) on first run.
pub fn load_or_create(dir: &Path) -> Vec<u8> {
    let path = dir.join("webcrypto-master.key");
    if let Ok(k) = fs::read(&path) {
        if k.len() >= 16 {
            return k;
        }
    }
    let key = migrate_from_keychain().unwrap_or_else(random_key);
    if let Ok(mut f) = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
    {
        let _ = f.write_all(&key);
    }
    key
}

extern "C-unwind" fn master_key_for_webview(
    _this: *mut AnyObject,
    _cmd: Sel,
    _webview: *mut AnyObject,
) -> *mut NSData {
    let key = KEY.get().map(|k| k.as_slice()).unwrap_or(&[]);
    if std::env::var_os("MWA_DEBUG").is_some() {
        eprintln!("MWA_DEBUG: WebKit requested WebCrypto master key ({} bytes)", key.len());
    }
    Retained::autorelease_return(NSData::with_bytes(key))
}

/// Must run after wry has registered `WryNavigationDelegate` (i.e. after the first webview is
/// built) and before the WhatsApp webviews are created: WebKit checks which delegate methods
/// exist at the moment a navigation delegate is assigned.
pub fn install(key: Vec<u8>) -> bool {
    let _ = KEY.set(key);
    // objc2 auto-names wry's class "<module path>::WryNavigationDelegate<version>".
    let classes = AnyClass::classes();
    let Some(cls) = classes.iter().copied().find(|c| {
        let n = c.name().to_string_lossy();
        n == "WryNavigationDelegate" || n.contains("::WryNavigationDelegate")
    }) else {
        return false;
    };
    unsafe {
        let imp: Imp = std::mem::transmute(
            master_key_for_webview
                as extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut NSData,
        );
        let added = ffi::class_addMethod(
            cls as *const AnyClass as *mut AnyClass,
            sel!(_webCryptoMasterKeyForWebView:),
            imp,
            c"@@:@".as_ptr(),
        );
        added.as_bool() || cls.responds_to(sel!(_webCryptoMasterKeyForWebView:))
    }
}

#[cfg(test)]
mod tests {
    use super::decode_keychain_value;

    #[test]
    fn decodes_base64() {
        // 16 bytes 0x00..0x0f
        let v = decode_keychain_value("AAECAwQFBgcICQoLDA0ODw==\n").unwrap();
        assert_eq!(v, (0u8..16).collect::<Vec<_>>());
    }

    #[test]
    fn decodes_hex_of_raw_bytes() {
        let v = decode_keychain_value("000102030405060708090a0b0c0d0e0f").unwrap();
        assert_eq!(v, (0u8..16).collect::<Vec<_>>());
    }

    #[test]
    fn decodes_hex_of_base64_text() {
        let hex: String = "AAECAwQFBgcICQoLDA0ODw==\n".bytes().map(|b| format!("{b:02x}")).collect();
        let v = decode_keychain_value(&hex).unwrap();
        assert_eq!(v, (0u8..16).collect::<Vec<_>>());
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_keychain_value("nope").is_none());
    }
}
