//! Network access to the topic space (SilverBullet) and the agent (ZeroClaw),
//! both reached over the tailnet. Every call blocks; the native UI runs them on
//! worker threads.
use margin_editor_core::topics::{Message, Session, SpaceFile};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, String>;

/// Where the space and the agent live. Read from
/// `~/.config/margin/topics.json`, overridden by `MARGIN_SPACE_URL`,
/// `MARGIN_ZEROCLAW_URL` and `MARGIN_ZEROCLAW_AGENT`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Endpoints {
    /// SilverBullet, e.g. `https://host.tailnet.ts.net:8443`.
    pub space: String,
    /// ZeroClaw gateway, e.g. `https://host.tailnet.ts.net`.
    pub zeroclaw: String,
    /// Agent alias that answers topic chats.
    #[serde(default)]
    pub agent: String,
    /// Read-only agent alias that answers comments on pages.
    #[serde(default = "default_comment_agent")]
    pub comment_agent: String,
}

fn default_comment_agent() -> String {
    "scribe".into()
}

/// `Space::write`'s error when the page changed since it was read.
pub const CONFLICT: &str = "The page changed on the server since it was opened. Reload it before saving.";

impl Endpoints {
    /// Base URLs without a trailing `/`, so paths can be appended.
    pub fn new(space: &str, zeroclaw: &str, agent: &str) -> Self {
        Self {
            space: space.trim_end_matches('/').into(),
            zeroclaw: zeroclaw.trim_end_matches('/').into(),
            agent: agent.into(),
            comment_agent: default_comment_agent(),
        }
    }

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

    /// The Keychain account for this gateway's token.
    pub fn account(&self) -> &str {
        self.zeroclaw.split("://").nth(1).unwrap_or(&self.zeroclaw)
    }

    /// The gateway, signed in with this Mac's stored token when it has one.
    pub fn zeroclaw(&self) -> ZeroClaw {
        ZeroClaw::new(&self.zeroclaw, token::load(self.account()))
    }
}

/// Exchange a one-time pairing code for a device token and keep it in the
/// Keychain. Returns the gateway, signed in.
pub fn pair(endpoints: &Endpoints, code: &str) -> Result<ZeroClaw> {
    let token = ZeroClaw::new(&endpoints.zeroclaw, None).pair_code(code)?;
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

fn http(timeout: Duration) -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .user_agent("margin")
        // Pages, transcripts and comment files go to two hosts in bursts:
        // keep enough connections, long enough, to skip new TLS handshakes.
        .max_idle_connections_per_host(8)
        .max_idle_age(Duration::from_secs(120))
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

struct Reply {
    status: u16,
    etag: Option<String>,
    body: String,
}

fn reply(response: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Reply> {
    let mut r = response.map_err(|e| e.to_string())?;
    let status = r.status().as_u16();
    let etag = r
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = r
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(Reply { status, etag, body })
}

fn ok(r: Reply) -> Result<Reply> {
    if (200..300).contains(&r.status) {
        Ok(r)
    } else {
        let detail: String = r.body.chars().take(240).collect();
        Err(format!("HTTP {}: {}", r.status, detail.trim()))
    }
}

/// A page and the version it was read at.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub path: String,
    pub text: String,
    pub etag: Option<String>,
}

/// SilverBullet's file API over the space folder.
#[derive(Clone)]
pub struct Space {
    base: String,
    http: ureq::Agent,
}

impl Space {
    /// `base` comes from `Endpoints`, without a trailing `/`.
    pub fn new(base: &str) -> Self {
        Self {
            base: base.into(),
            http: http(Duration::from_secs(20)),
        }
    }

    pub fn list(&self) -> Result<Vec<SpaceFile>> {
        let r = ok(reply(
            self.http
                .get(format!("{}/.fs", self.base))
                .header("Accept", "application/json")
                .call(),
        )?)?;
        serde_json::from_str(&r.body).map_err(|e| format!("space listing: {e}"))
    }

    pub fn read(&self, path: &str) -> Result<Page> {
        let r = ok(reply(
            self.http
                .get(format!("{}/.fs/{}", self.base, encode_path(path)))
                .call(),
        )?)?;
        Ok(Page {
            path: path.into(),
            text: r.body,
            etag: r.etag,
        })
    }

    /// A page that may not exist yet: `None` when the server has no such file.
    pub fn read_optional(&self, path: &str) -> Result<Option<Page>> {
        let r = reply(
            self.http
                .get(format!("{}/.fs/{}", self.base, encode_path(path)))
                .call(),
        )?;
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

    /// Write a page if nobody changed it since `etag`. Returns the new version.
    pub fn write(&self, path: &str, text: &str, etag: Option<&str>) -> Result<Option<String>> {
        let mut req = self
            .http
            .put(format!("{}/.fs/{}", self.base, encode_path(path)))
            .header("Content-Type", "text/markdown")
            .header("X-Source", "external")
            .header("X-Permission", "rw");
        if let Some(tag) = etag {
            req = req.header("If-Match", tag);
        }
        let r = reply(req.send(text))?;
        if r.status == 412 {
            return Err(CONFLICT.into());
        }
        Ok(ok(r)?.etag)
    }

    pub fn delete(&self, path: &str) -> Result<()> {
        ok(reply(
            self.http
                .delete(format!("{}/.fs/{}", self.base, encode_path(path)))
                .call(),
        )?)?;
        Ok(())
    }

    /// The page in SilverBullet's own web app.
    pub fn page_url(&self, path: &str) -> String {
        format!("{}/{}", self.base, encode_path(path.strip_suffix(".md").unwrap_or(path)))
    }
}

/// Something the agent did during a chat turn.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    Chunk(String),
    Tool(String),
    /// The agent asks before running a tool; the chat's `decide` answers.
    Approval { tool: String, summary: String },
    /// The whole reply; the last event of a turn that worked.
    Done(String),
    /// Why the turn failed; the last event of a turn that did not.
    Error(String),
    /// Nothing happened for a while; lets the listener stop a quiet turn.
    Tick,
}

/// One message to the agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub agent: String,
    pub session: String,
    /// Shown for the session in ZeroClaw's dashboard.
    pub name: String,
    pub content: String,
    /// Sent ahead of `content` when the session is new.
    pub preamble: Option<String>,
}

/// Something that answers a chat turn: ZeroClaw, or a stand-in.
///
/// `chat` reports the reply through `on` and always ends with exactly one
/// `Done` (carrying the whole reply) or `Error`, unless `on` returned `false`
/// to stop the turn early (the listener is gone). `decide` answers tool
/// approval prompts; it may block until someone chooses.
pub trait AgentChat: Send + Sync {
    fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, decide: &mut dyn FnMut() -> bool);

    /// Answers at once, without a network round trip: callers may run it
    /// on their own thread.
    fn inline(&self) -> bool {
        false
    }
}

/// A stand-in agent that answers every turn with the text `reply` builds,
/// word by word. `pause` spaces the words out (the demo); without it the
/// answer is immediate (tests, the sample note).
pub struct Canned {
    pub reply: Box<dyn Fn(&Ask) -> String + Send + Sync>,
    pub pause: Option<Duration>,
}

impl AgentChat for Canned {
    fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, _decide: &mut dyn FnMut() -> bool) {
        let reply = (self.reply)(ask);
        for word in reply.split_inclusive(' ') {
            if let Some(pause) = self.pause {
                std::thread::sleep(pause);
            }
            if !on(ChatEvent::Chunk(word.into())) {
                return;
            }
        }
        on(ChatEvent::Done(reply));
    }

    fn inline(&self) -> bool {
        self.pause.is_none()
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
    http: ureq::Agent,
}

impl ZeroClaw {
    /// `base` comes from `Endpoints`, without a trailing `/`.
    pub fn new(base: &str, token: Option<String>) -> Self {
        Self {
            base: base.into(),
            token,
            http: http(Duration::from_secs(30)),
        }
    }

    pub fn paired(&self) -> bool {
        self.token.is_some()
    }

    fn token(&self) -> Result<&str> {
        self.token
            .as_deref()
            .ok_or_else(|| "This Mac is not paired with ZeroClaw yet".into())
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let r = ok(reply(
            self.http
                .get(format!("{}{path}", self.base))
                .header("Authorization", format!("Bearer {}", self.token()?))
                .call(),
        )?)?;
        serde_json::from_str(&r.body).map_err(|e| format!("{path}: {e}"))
    }

    fn pair_code(&self, code: &str) -> Result<String> {
        let r = ok(reply(
            self.http
                .post(format!("{}/pair", self.base))
                .header("X-Pairing-Code", code.trim())
                .send_empty(),
        )?)?;
        let v: Value = serde_json::from_str(&r.body).map_err(|e| e.to_string())?;
        v["token"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("pairing refused: {}", v["message"]))
    }

    pub fn sessions(&self) -> Result<Vec<Session>> {
        Ok(self.get::<SessionList>("/api/sessions")?.sessions)
    }

    /// A session's messages; none when the session does not exist yet.
    pub fn messages(&self, key: &str) -> Result<Vec<Message>> {
        let r = reply(
            self.http
                .get(format!("{}/api/sessions/{}/messages", self.base, encode(key)))
                .header("Authorization", format!("Bearer {}", self.token()?))
                .call(),
        )?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        let r = ok(r)?;
        Ok(serde_json::from_str::<MessageList>(&r.body)
            .map_err(|e| format!("messages: {e}"))?
            .messages)
    }

    pub fn delete_session(&self, key: &str) -> Result<()> {
        ok(reply(
            self.http
                .delete(format!("{}/api/sessions/{}", self.base, encode(key)))
                .header("Authorization", format!("Bearer {}", self.token()?))
                .call(),
        )?)?;
        Ok(())
    }

    /// One turn over `/ws/chat`; see `AgentChat` for the protocol.
    fn turn(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, decide: &mut dyn FnMut() -> bool) -> Result<Option<String>> {
        use tungstenite::client::IntoClientRequest;
        use tungstenite::http::HeaderValue;
        use tungstenite::{Message as Frame, stream::MaybeTlsStream};
        let ws_base = self
            .base
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        let url = format!(
            "{ws_base}/ws/chat?agent={}&session_id={}&name={}",
            encode(&ask.agent),
            encode(&ask.session),
            encode(&ask.name)
        );
        let mut request = url.into_client_request().map_err(|e| e.to_string())?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_str(&format!("zeroclaw.v1, bearer.{}", self.token()?)).map_err(|e| e.to_string())?,
        );
        let (mut ws, _) = tungstenite::connect(request).map_err(|e| e.to_string())?;
        // Wake up regularly to notice a listener that went away; give up after
        // ten quiet minutes (a turn that runs tools can be quiet for a while).
        let tick = Some(Duration::from_secs(5));
        match ws.get_ref() {
            MaybeTlsStream::Plain(s) => s.set_read_timeout(tick),
            MaybeTlsStream::NativeTls(s) => s.get_ref().set_read_timeout(tick),
            _ => Ok(()),
        }
        .map_err(|e| e.to_string())?;
        let send = |ws: &mut tungstenite::WebSocket<_>, v: Value| {
            ws.send(Frame::Text(v.to_string().into())).map_err(|e| e.to_string())
        };
        send(&mut ws, json!({"type": "connect"}))?;
        // `session_start` says whether the session is new; only a new one gets the preamble.
        let mut fresh = false;
        let mut sent = false;
        let mut streamed = String::new();
        let mut quiet = Duration::ZERO;
        let result = loop {
            let frame = match ws.read() {
                Ok(Frame::Text(t)) => t,
                Ok(Frame::Close(_)) => break Err("The agent closed the connection".to_string()),
                Ok(_) => continue,
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
                {
                    quiet += tick.unwrap_or_default();
                    if !on(ChatEvent::Tick) {
                        break Ok(None);
                    }
                    if quiet >= Duration::from_secs(600) {
                        break Err("The agent stopped answering".into());
                    }
                    continue;
                }
                Err(e) => break Err(e.to_string()),
            };
            quiet = Duration::ZERO;
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
                    send(&mut ws, json!({"type": "message", "content": content}))?;
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
                    let decision = if keep && decide() { "approve" } else { "deny" };
                    send(
                        &mut ws,
                        json!({"type": "approval_response", "request_id": text("request_id"), "decision": decision}),
                    )?;
                    keep
                }
                "done" => {
                    let full = text("full_response");
                    break Ok(Some(if full.trim().is_empty() { streamed } else { full }));
                }
                "aborted" => break Err("The turn was aborted".into()),
                "error" => {
                    let message = text("message");
                    break Err(if message.is_empty() { frame.to_string() } else { message });
                }
                _ => true,
            };
            if !keep {
                break Ok(None);
            }
        };
        let _ = ws.close(None);
        result
    }
}

impl AgentChat for ZeroClaw {
    fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, decide: &mut dyn FnMut() -> bool) {
        match self.turn(ask, on, decide) {
            Ok(Some(full)) => {
                on(ChatEvent::Done(full));
            }
            // Stopped by the listener: nobody to tell.
            Ok(None) => {}
            Err(e) => {
                on(ChatEvent::Error(e));
            }
        }
    }
}

/// The device token, kept in the macOS Keychain (a 0600 file elsewhere).
/// `ZEROCLAW_TOKEN` overrides it.
/// The device token. Release builds on macOS keep it in the Keychain. Dev
/// builds (and other platforms) keep it in a file only you can read, next to
/// `topics.json`: every rebuild is a new binary the Keychain no longer trusts,
/// so it would ask again each time. `ZEROCLAW_TOKEN` overrides both.
pub mod token {
    use super::Result;

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
        super::Endpoints::path().with_file_name(format!("token-{}", super::encode(account)))
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
