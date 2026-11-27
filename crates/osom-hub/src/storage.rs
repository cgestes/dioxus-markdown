//! Platform persistence for `Endpoints` and the paired ZeroClaw device token:
//! `~/.config/margin/topics.json` + macOS Keychain on native, `localStorage` +
//! optional `config.json` on wasm32.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use crate::{Endpoints, Result};
    use std::path::PathBuf;

    impl Endpoints {
        pub fn path() -> PathBuf {
            if let Some(p) = std::env::var_os("MARGIN_TOPICS_CONFIG") {
                return p.into();
            }
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            home.join(".config/margin/topics.json")
        }

        pub fn load() -> Result<Self> {
            let path = Self::path();
            let mut e: Endpoints = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(|err| format!("{}: {err}", path.display()))?,
                Err(_) => Endpoints::default(),
            };
            for (var, field) in [
                ("MARGIN_SPACE_URL", &mut e.space),
                ("MARGIN_ZEROCLAW_URL", &mut e.zeroclaw),
                ("MARGIN_ZEROCLAW_AGENT", &mut e.agent),
            ] {
                if let Ok(v) = std::env::var(var)
                    && !v.is_empty()
                {
                    *field = v;
                }
            }
            if e.space.is_empty() || e.zeroclaw.is_empty() {
                return Err(format!("Set \"space\" and \"zeroclaw\" URLs in {}", path.display()));
            }
            e.space = e.space.trim_end_matches('/').into();
            e.zeroclaw = e.zeroclaw.trim_end_matches('/').into();
            e.space_web = e.space_web.trim_end_matches('/').into();
            Ok(e)
        }

        pub fn save(&self) -> Result<()> {
            let path = Self::path();
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
            std::fs::write(&path, text + "\n").map_err(|e| e.to_string())
        }
    }

    /// The device token. Release builds on macOS keep it in the Keychain. Dev
    /// builds (and other platforms) keep it in a file only you can read, next to
    /// `topics.json`: every rebuild is a new binary the Keychain no longer trusts,
    /// so it would ask again each time. `ZEROCLAW_TOKEN` overrides both.
    pub mod token {
        use crate::{Endpoints, Result, encode};

        const KEYCHAIN: bool = cfg!(all(target_os = "macos", not(debug_assertions)));

        #[cfg(target_os = "macos")]
        const SERVICE: &str = "Margin · ZeroClaw device token";

        pub fn load(account: &str) -> Option<String> {
            if let Ok(t) = std::env::var("ZEROCLAW_TOKEN")
                && !t.trim().is_empty()
            {
                return Some(t.trim().into());
            }
            if KEYCHAIN {
                #[cfg(target_os = "macos")]
                return security_framework::passwords::get_generic_password(SERVICE, account)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
            }
            std::fs::read_to_string(file(account))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }

        pub fn store(account: &str, token: &str) -> Result<()> {
            if KEYCHAIN {
                #[cfg(target_os = "macos")]
                return security_framework::passwords::set_generic_password(SERVICE, account, token.as_bytes())
                    .map_err(|e| e.to_string());
            }
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let path = file(account);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
                .map_err(|e| e.to_string())?;
            f.write_all(token.as_bytes()).map_err(|e| e.to_string())
        }

        /// Where it is kept, for messages.
        pub fn location(account: &str) -> String {
            if KEYCHAIN {
                "the Keychain".into()
            } else {
                file(account).display().to_string()
            }
        }

        fn file(account: &str) -> std::path::PathBuf {
            Endpoints::path().with_file_name(format!("token-{}", encode(account)))
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use crate::{Endpoints, Result, http_client, request};
    use reqwest::Method;
    use wasm_bindgen::JsValue;

    const ENDPOINTS_KEY: &str = "osom.endpoints";

    fn js_err(v: JsValue) -> String {
        v.as_string()
            .or_else(|| js_sys::JSON::stringify(&v).ok().and_then(|s| s.as_string()))
            .unwrap_or_else(|| format!("{v:?}"))
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    impl Endpoints {
        /// Default same-origin paths when served on the ZeroClaw host via `tailscale serve`.
        pub fn web_defaults() -> Self {
            Self::new("/_space", "/_zeroclaw", "home")
        }

        /// Saved browser settings from `localStorage`, if present.
        pub fn load_saved() -> Option<Self> {
            let raw = storage()?.get_item(ENDPOINTS_KEY).ok().flatten()?;
            let mut e: Self = serde_json::from_str(&raw).ok()?;
            if e.space.is_empty() || e.zeroclaw.is_empty() {
                return None;
            }
            e.space = e.space.trim_end_matches('/').into();
            e.zeroclaw = e.zeroclaw.trim_end_matches('/').into();
            e.space_web = e.space_web.trim_end_matches('/').into();
            Some(e)
        }

        /// Load endpoints for the browser: `localStorage` override, otherwise
        /// `config.json` served alongside the static bundle, otherwise same-origin
        /// `/_space` + `/_zeroclaw` defaults.
        pub async fn load_web() -> Result<Self> {
            if let Some(saved) = Self::load_saved() {
                return Ok(saved);
            }
            let client = http_client();
            if let Ok(r) = request(
                &client,
                Method::GET,
                "config.json".into(),
                vec![("Accept".into(), "application/json".into())],
                None,
            )
            .await
                && (200..300).contains(&r.status)
                && let Ok(mut e) = serde_json::from_str::<Self>(&r.body)
            {
                if e.space.is_empty() {
                    e.space = "/_space".into();
                }
                if e.zeroclaw.is_empty() {
                    e.zeroclaw = "/_zeroclaw".into();
                }
                if e.agent.is_empty() {
                    e.agent = "home".into();
                }
                e.space = e.space.trim_end_matches('/').into();
                e.zeroclaw = e.zeroclaw.trim_end_matches('/').into();
                e.space_web = e.space_web.trim_end_matches('/').into();
                return Ok(e);
            }
            Ok(Self::web_defaults())
        }

        pub fn save(&self) -> Result<()> {
            let s = storage().ok_or_else(|| "browser storage is unavailable".to_string())?;
            let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
            s.set_item(ENDPOINTS_KEY, &text).map_err(js_err)
        }

        pub fn clear_saved() {
            if let Some(s) = storage() {
                let _ = s.remove_item(ENDPOINTS_KEY);
            }
        }
    }

    /// Device token kept in `localStorage`, scoped by gateway account.
    pub mod token {
        use super::{Result, js_err, storage};

        fn key(account: &str) -> String {
            format!("osom.token.{}", crate::encode(account))
        }

        pub fn load(account: &str) -> Option<String> {
            storage()?
                .get_item(&key(account))
                .ok()
                .flatten()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }

        pub fn store(account: &str, token: &str) -> Result<()> {
            let s = storage().ok_or_else(|| "browser storage is unavailable".to_string())?;
            s.set_item(&key(account), token.trim()).map_err(js_err)
        }

        pub fn location(_account: &str) -> String {
            "browser storage".into()
        }
    }
}

pub use imp::token;
