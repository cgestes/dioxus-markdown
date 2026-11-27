//! Network access to the topic space (SilverBullet) and the agent (ZeroClaw),
//! using one shared async HTTP (`reqwest`) and WebSocket (`/ws/chat`) client
//! on both native and wasm32.
pub use margin_core::agent::{AgentChat, Ask, Canned, ChatEvent, ChatFuture};
use margin_core::files::{FileFuture, PageFiles, Versioned, WriteError};
use osom_topics::{Message, Session, SpaceFile};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

mod storage;
mod ws;

pub use storage::token;
#[cfg(not(target_arch = "wasm32"))]
pub use ws::block_on;

pub type Result<T> = std::result::Result<T, String>;

/// Where the space and the agent live. On native, read from
/// `~/.config/margin/topics.json`, overridden by `MARGIN_SPACE_URL`,
/// `MARGIN_ZEROCLAW_URL` and `MARGIN_ZEROCLAW_AGENT`. On wasm32, read from
/// `localStorage` or `config.json` (defaulting to `/_space` and `/_zeroclaw`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Endpoints {
    /// SilverBullet, e.g. `https://host.tailnet.ts.net:8443` or `/_space`.
    pub space: String,
    /// ZeroClaw gateway, e.g. `https://host.tailnet.ts.net` or `/_zeroclaw`.
    pub zeroclaw: String,
    /// Agent alias that answers topic chats.
    #[serde(default)]
    pub agent: String,
    /// Read-only agent alias that answers comments on pages.
    #[serde(default = "default_comment_agent")]
    pub comment_agent: String,
    /// Optional SilverBullet web URL for "Open in SilverBullet" links when
    /// `space` is a same-origin proxy path like `/_space`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub space_web: String,
}

fn default_comment_agent() -> String {
    "scribe".into()
}

impl Endpoints {
    /// Base URLs without a trailing `/`, so paths can be appended.
    pub fn new(space: &str, zeroclaw: &str, agent: &str) -> Self {
        Self {
            space: space.trim_end_matches('/').into(),
            zeroclaw: zeroclaw.trim_end_matches('/').into(),
            agent: agent.into(),
            comment_agent: default_comment_agent(),
            space_web: String::new(),
        }
    }

    /// The Keychain / storage account for this gateway's token.
    pub fn account(&self) -> &str {
        self.zeroclaw.split("://").nth(1).unwrap_or(&self.zeroclaw)
    }

    /// The gateway, signed in with this device's stored token when it has one.
    pub fn zeroclaw(&self) -> ZeroClaw {
        ZeroClaw::new(&self.zeroclaw, token::load(self.account()))
    }
}

/// Exchange a one-time pairing code for a device token and store it locally.
/// Returns the gateway, signed in.
pub async fn pair(endpoints: &Endpoints, code: &str) -> Result<ZeroClaw> {
    let token = ZeroClaw::new(&endpoints.zeroclaw, None).pair_code(code).await?;
    token::store(endpoints.account(), &token)?;
    Ok(ZeroClaw::new(&endpoints.zeroclaw, Some(token)))
}

/// Everything but unreserved characters (RFC 3986).
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Percent-encode one URL component.
pub fn encode(s: &str) -> String {
    utf8_percent_encode(s, COMPONENT).to_string()
}

/// Percent-encode a path, keeping `/` between segments.
pub fn encode_path(path: &str) -> String {
    path.split('/').map(encode).collect::<Vec<_>>().join("/")
}

/// A page and the version it was read at.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub path: String,
    pub text: String,
    pub etag: Option<String>,
}

pub(crate) struct Reply {
    pub status: u16,
    pub etag: Option<String>,
    pub body: String,
}

pub(crate) fn ok(r: Reply) -> Result<Reply> {
    if (200..300).contains(&r.status) {
        Ok(r)
    } else {
        let detail: String = r.body.chars().take(240).collect();
        Err(format!("HTTP {}: {}", r.status, detail.trim()))
    }
}

/// Shared HTTP client builder: `reqwest` uses `rustls` + platform verifier on
/// native and browser `fetch` on wasm32.
pub(crate) fn http_client() -> reqwest::Client {
    #[cfg(not(target_arch = "wasm32"))]
    {
        reqwest::Client::builder()
            .user_agent("margin")
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(8)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }
    #[cfg(target_arch = "wasm32")]
    {
        reqwest::Client::new()
    }
}

/// Resolve a URL that may be relative (e.g. `/_space/.fs` in a browser) into
/// an absolute URL for `reqwest` and `WebSocket`.
pub(crate) fn resolve_url(url: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let origin = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default();
            if url.starts_with('/') {
                return format!("{origin}{url}");
            }
            return format!("{origin}/{url}");
        }
    }
    url.to_string()
}

pub(crate) async fn request(
    client: &reqwest::Client,
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
) -> Result<Reply> {
    let client = client.clone();
    let url = resolve_url(&url);
    ws::io(async move {
        let mut req = client.request(method, &url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        if let Some(text) = body {
            req = req.body(text);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text().await.map_err(|e| e.to_string())?;
        Ok(Reply { status, etag, body })
    })
    .await
}

/// SilverBullet's file API over the space folder.
#[derive(Clone)]
pub struct Space {
    base: String,
    web_base: String,
    http: reqwest::Client,
}

impl Space {
    /// `base` comes from `Endpoints`, without a trailing `/`.
    pub fn new(base: &str) -> Self {
        Self::with_web_base(base, "")
    }

    pub fn with_web_base(base: &str, web_base: &str) -> Self {
        let base = base.trim_end_matches('/').to_string();
        let web_base = if web_base.trim().is_empty() {
            base.clone()
        } else {
            web_base.trim_end_matches('/').to_string()
        };
        Self {
            base,
            web_base,
            http: http_client(),
        }
    }

    pub async fn list(&self) -> Result<Vec<SpaceFile>> {
        let r = ok(request(
            &self.http,
            Method::GET,
            format!("{}/.fs", self.base),
            vec![("Accept".into(), "application/json".into())],
            None,
        )
        .await?)?;
        serde_json::from_str(&r.body).map_err(|e| format!("space listing: {e}"))
    }

    pub async fn read(&self, path: &str) -> Result<Page> {
        let r = ok(request(
            &self.http,
            Method::GET,
            format!("{}/.fs/{}", self.base, encode_path(path)),
            Vec::new(),
            None,
        )
        .await?)?;
        Ok(Page {
            path: path.into(),
            text: r.body,
            etag: r.etag,
        })
    }

    /// A page that may not exist yet: `None` when the server has no such file.
    pub async fn read_optional(&self, path: &str) -> Result<Option<Page>> {
        let r = request(
            &self.http,
            Method::GET,
            format!("{}/.fs/{}", self.base, encode_path(path)),
            Vec::new(),
            None,
        )
        .await?;
        if r.status == 404 {
            return Ok(None);
        }
        let r = ok(r)?;
        Ok(Some(Page {
            path: path.into(),
            text: r.body,
            etag: r.etag,
        }))
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        ok(request(
            &self.http,
            Method::DELETE,
            format!("{}/.fs/{}", self.base, encode_path(path)),
            Vec::new(),
            None,
        )
        .await?)?;
        Ok(())
    }

    /// The page in SilverBullet's own web app.
    pub fn page_url(&self, path: &str) -> String {
        format!("{}/{}", self.web_base, encode_path(path.strip_suffix(".md").unwrap_or(path)))
    }
}

/// The editor saves a page of the space, and its comment file, through this.
impl PageFiles for Space {
    fn read(&self, path: String) -> FileFuture<Result<Option<Versioned>>> {
        let space = self.clone();
        Box::pin(async move {
            Ok(space
                .read_optional(&path)
                .await?
                .map(|page| Versioned { text: page.text, etag: page.etag }))
        })
    }

    /// Write a page if nobody changed it since `etag`. Returns the new version.
    fn write(&self, path: String, text: String, etag: Option<String>) -> FileFuture<std::result::Result<Option<String>, WriteError>> {
        let space = self.clone();
        Box::pin(async move {
            let mut headers = vec![
                ("Content-Type".into(), "text/markdown".into()),
                ("X-Source".into(), "external".into()),
                ("X-Permission".into(), "rw".into()),
            ];
            if let Some(tag) = etag {
                headers.push(("If-Match".into(), tag));
            }
            let r = request(
                &space.http,
                Method::PUT,
                format!("{}/.fs/{}", space.base, encode_path(&path)),
                headers,
                Some(text),
            )
            .await
            .map_err(WriteError::Failed)?;
            if r.status == 412 {
                return Err(WriteError::Conflict);
            }
            Ok(ok(r).map_err(WriteError::Failed)?.etag)
        })
    }

    fn web_url(&self, path: &str) -> String {
        self.page_url(path)
    }
}

#[derive(Deserialize)]
struct SessionList {
    sessions: Vec<Session>,
}

#[derive(Deserialize)]
struct MessageList {
    messages: Vec<Message>,
}

/// The ZeroClaw gateway, authenticated with a paired device token.
#[derive(Clone)]
pub struct ZeroClaw {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl ZeroClaw {
    /// `base` comes from `Endpoints`, without a trailing `/`.
    pub fn new(base: &str, token: Option<String>) -> Self {
        Self {
            base: base.trim_end_matches('/').into(),
            token,
            http: http_client(),
        }
    }

    pub fn paired(&self) -> bool {
        self.token.is_some()
    }

    fn token(&self) -> Result<&str> {
        let device = if cfg!(target_arch = "wasm32") { "browser" } else { "Mac" };
        self.token
            .as_deref()
            .ok_or_else(|| format!("This {device} is not paired with ZeroClaw yet"))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let auth = format!("Bearer {}", self.token()?);
        let r = ok(request(
            &self.http,
            Method::GET,
            format!("{}{path}", self.base),
            vec![("Authorization".into(), auth)],
            None,
        )
        .await?)?;
        serde_json::from_str(&r.body).map_err(|e| format!("{path}: {e}"))
    }

    async fn pair_code(&self, code: &str) -> Result<String> {
        let r = ok(request(
            &self.http,
            Method::POST,
            format!("{}/pair", self.base),
            vec![("X-Pairing-Code".into(), code.trim().into())],
            None,
        )
        .await?)?;
        let v: Value = serde_json::from_str(&r.body).map_err(|e| e.to_string())?;
        v["token"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("pairing refused: {}", v["message"]))
    }

    pub async fn sessions(&self) -> Result<Vec<Session>> {
        Ok(self.get::<SessionList>("/api/sessions").await?.sessions)
    }

    /// A session's messages; none when the session does not exist yet.
    pub async fn messages(&self, key: &str) -> Result<Vec<Message>> {
        let auth = format!("Bearer {}", self.token()?);
        let r = request(
            &self.http,
            Method::GET,
            format!("{}/api/sessions/{}/messages", self.base, encode(key)),
            vec![("Authorization".into(), auth)],
            None,
        )
        .await?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        let r = ok(r)?;
        Ok(serde_json::from_str::<MessageList>(&r.body)
            .map_err(|e| format!("messages: {e}"))?
            .messages)
    }

    pub async fn delete_session(&self, key: &str) -> Result<()> {
        let auth = format!("Bearer {}", self.token()?);
        ok(request(
            &self.http,
            Method::DELETE,
            format!("{}/api/sessions/{}", self.base, encode(key)),
            vec![("Authorization".into(), auth)],
            None,
        )
        .await?)?;
        Ok(())
    }

    /// One turn over `/ws/chat`; shared protocol loop for both native and wasm32.
    async fn turn(
        &self,
        ask: &Ask,
        on: &mut (dyn FnMut(ChatEvent) -> bool + Send),
        decide: &mut (dyn FnMut() -> ChatFuture<bool> + Send),
    ) -> Result<Option<String>> {
        let token = self.token()?.to_string();
        let ws_base = resolve_url(&self.base)
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        let url = format!(
            "{ws_base}/ws/chat?agent={}&session_id={}&name={}",
            encode(&ask.agent),
            encode(&ask.session),
            encode(&ask.name)
        );
        let mut socket = ws::Socket::connect(&url, &token).await?;
        socket.send_text(json!({"type": "connect"}).to_string()).await?;
        let mut fresh = false;
        let mut sent = false;
        let mut streamed = String::new();
        #[cfg(not(target_arch = "wasm32"))]
        let mut quiet_ticks: u32 = 0;
        let result = loop {
            let frame = match socket.next_text().await {
                Ok(ws::Next::Text(t)) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        quiet_ticks = 0;
                    }
                    t
                }
                #[cfg(not(target_arch = "wasm32"))]
                Ok(ws::Next::Tick) => {
                    quiet_ticks += 1;
                    if !on(ChatEvent::Tick) {
                        break Ok(None);
                    }
                    // 120 × 5s = 10 quiet minutes (a turn running tools can pause for a while).
                    if quiet_ticks >= 120 {
                        break Err("The agent stopped answering".into());
                    }
                    continue;
                }
                Ok(ws::Next::Closed) => break Err("The agent closed the connection".into()),
                Err(e) => break Err(e),
            };
            let Ok(v) = serde_json::from_str::<Value>(&frame) else {
                continue;
            };
            let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let keep = match v["type"].as_str().unwrap_or_default() {
                "session_start" => {
                    fresh = v["message_count"].as_u64() == Some(0);
                    true
                }
                "connected" if !sent => {
                    sent = true;
                    let content = match &ask.preamble {
                        Some(p) if fresh => format!("{p}\n\n{}", ask.content),
                        _ => ask.content.clone(),
                    };
                    socket.send_text(json!({"type": "message", "content": content}).to_string()).await?;
                    true
                }
                "chunk" => {
                    let chunk = text("content");
                    streamed.push_str(&chunk);
                    on(ChatEvent::Chunk(chunk))
                }
                "tool_call" | "tool_use" => on(ChatEvent::Tool(text("name"))),
                "approval_request" => {
                    let keep = on(ChatEvent::Approval {
                        tool: text("tool"),
                        summary: text("arguments_summary"),
                    });
                    let decision = if keep && decide().await { "approve" } else { "deny" };
                    socket
                        .send_text(
                            json!({"type": "approval_response", "request_id": text("request_id"), "decision": decision})
                                .to_string(),
                        )
                        .await?;
                    keep
                }
                "done" => {
                    let full = text("full_response");
                    break Ok(Some(if full.trim().is_empty() { streamed } else { full }));
                }
                "aborted" => break Err("The turn was aborted".into()),
                "error" => {
                    let message = text("message");
                    break Err(if message.is_empty() { frame } else { message });
                }
                _ => true,
            };
            if !keep {
                break Ok(None);
            }
        };
        socket.close().await;
        result
    }
}

impl AgentChat for ZeroClaw {
    fn chat(
        self: Arc<Self>,
        ask: Ask,
        mut on: Box<dyn FnMut(ChatEvent) -> bool + Send>,
        mut decide: Box<dyn FnMut() -> ChatFuture<bool> + Send>,
    ) -> ChatFuture<()> {
        Box::pin(ws::io(async move {
            match self.turn(&ask, &mut *on, &mut *decide).await {
                Ok(Some(full)) => {
                    on(ChatEvent::Done(full));
                }
                Ok(None) => {}
                Err(e) => {
                    on(ChatEvent::Error(e));
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_keep_separators_and_encode_segments() {
        assert_eq!(encode_path("people/alex-sample.md"), "people/alex-sample.md");
        assert_eq!(encode_path("a b/é.md"), "a%20b/%C3%A9.md");
        assert_eq!(encode("telegram_-53/x"), "telegram_-53%2Fx");
        let e = Endpoints::new("https://h.ts.net:8443/", "https://h.ts.net/", "luc");
        assert_eq!(
            Space::new(&e.space).page_url("people/alex-sample.md"),
            "https://h.ts.net:8443/people/alex-sample"
        );
    }

    #[test]
    fn the_token_account_is_the_gateway_host() {
        let e = Endpoints::new("https://h.ts.net:8443", "https://h.ts.net/", "luc");
        assert_eq!(e.account(), "h.ts.net");
    }
}
