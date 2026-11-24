# Whatsapp by TagLinc

![Multi Whatsapp by TagLinc](marketing/poster.png)

যত খুশি WhatsApp অ্যাকাউন্ট, এক অ্যাপে। Mac-এর জন্য ছোট্ট একটি অ্যাপ (মাত্র ~১ MB)।

Run as many WhatsApp accounts as you like in one small Mac app.

## ডাউনলোড / Download

**[সর্বশেষ ভার্সন ডাউনলোড করুন → Releases](https://github.com/mhmehedi/whatsapp-by-taglinc/releases/latest)**

`Whatsapp-by-TagLinc.dmg` ফাইলটি ডাউনলোড করুন।

- macOS 14 (Sonoma) বা নতুন
- Apple Silicon (M1/M2/M3/M4) ও Intel, দুটোতেই চলে

## ইনস্টল / Install

1. DMG খুলে **Whatsapp by TagLinc** কে **Applications** ফোল্ডারে টেনে আনুন।
2. Applications থেকে অ্যাপটি খুলুন। macOS বলবে অ্যাপটি খোলা যাচ্ছে না, কারণ এটি App Store থেকে নয়। **Done** চাপুন।
3. **System Settings → Privacy & Security**-এ যান। নিচে "Whatsapp by TagLinc was blocked"-এর পাশে **Open Anyway** চাপুন, তারপর Mac-এর পাসওয়ার্ড বা Touch ID দিন। এটা একবারই করতে হবে।

   অথবা Terminal-এ চালান:
   ```bash
   xattr -dr com.apple.quarantine "/Applications/Whatsapp by TagLinc.app"
   ```
4. ফোনের WhatsApp থেকে QR কোড স্ক্যান করুন (**Settings → Linked devices → Link a device**)।

## ব্যবহার / Using it

| | |
|---|---|
| **+** বা **⌘N** | নতুন WhatsApp অ্যাকাউন্ট যোগ করুন |
| **⌘1 … ⌘9** | অ্যাকাউন্ট বদলান |
| অ্যাকাউন্টে **right-click** | নাম বদলান বা অ্যাকাউন্ট সরান |
| **⌘R** | বর্তমান অ্যাকাউন্ট রিলোড |
| চ্যাট লিস্টের ডান কিনারা টানুন | চ্যাট লিস্ট ছোট-বড় করুন (ডাবল-ক্লিকে আগের মতো) |
| **Account → Sleep When Not Used** | ৩০ মিনিট ব্যবহার না করলে অ্যাকাউন্টটি বন্ধ হয়ে যায়, RAM খরচ হয় না (ঘুমন্ত অবস্থায় নোটিফিকেশন আসে না) |

- প্রতিটি অ্যাকাউন্ট আলাদা লগইনে থাকে, একটার ডেটা আরেকটার সাথে মেশে না।
- সব অ্যাকাউন্ট একসাথে খোলা থাকে; নতুন মেসেজ এলে ব্যাজ আর নোটিফিকেশন দেখায়।
- কোনো অ্যাকাউন্ট চলতে চলতে অনেক বেশি RAM নিয়ে ফেললে (৭০০ MB-এর বেশি, আর খোলার সময়ের চেয়ে ৪০০ MB বেশি), আপনি সেটা না দেখার সময় অ্যাপ নিজেই সেটা রিলোড করে। লগআউট হয় না।
- একবারে অ্যাপের একটাই কপি চালান। দুটো কপি একসাথে চললে WhatsApp লগআউট হয়ে যেতে পারে, তাই দ্বিতীয় কপি নিজে থেকেই বন্ধ হয়ে যায়।

## How it works

A Rust app ([tao](https://github.com/tauri-apps/tao) + [wry](https://github.com/tauri-apps/wry)) that shows [WhatsApp Web](https://web.whatsapp.com) in the Mac's built-in WebKit, so there is no bundled browser. Each account gets its own WebKit data store (macOS 14+), which keeps logins separate.

WebKit protects stored WebCrypto keys with a master key that normally lives in the Keychain and is tied to the exact app build. The app supplies its own master key instead (see [`src/webcrypto.rs`](src/webcrypto.rs)), so updating the app does not log anyone out.

Data is stored in `~/Library/Application Support/MultiWhatsApp` and `~/Library/WebKit/local.multiwhatsapp.app`.

## Build from source

Needs Rust (via [rustup](https://rustup.rs)) and the Xcode Command Line Tools.

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./build-dmg.sh
```

The universal app and DMG are written to `dist/`.

## License

The source code is released under the [MIT License](LICENSE).

The MIT License does not cover the logos and icons in this repository: the WhatsApp icon (`assets/AppIcon.icns`, `assets/DmgIcon.icns`) belongs to Meta Platforms, Inc., and the FlyNode, TagLinc, Doze Host and Hyperlink logos (`marketing/brand/`) belong to their owners.

## Disclaimer

This project is not affiliated with, endorsed by, or sponsored by WhatsApp or Meta Platforms, Inc. WhatsApp is a trademark of Meta Platforms, Inc. The app only displays the official WhatsApp Web site.
