//! A single-owner, keyless relay. The owner verification key is pinned at provisioning.
use super::{
    database, is_hex,
    keys::Grant,
    record::{self, DOCUMENT, Packet},
    stream, unb64,
};
use anyhow::{Context, Result, anyhow, ensure};
use futures_util::{SinkExt, StreamExt};
use loro_protocol::{EloHeader, UpdateStatusCode};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
};
use tokio_tungstenite::{
    WebSocketStream, accept_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

const MAX_WIRE: usize = 24 * 1024 * 1024;
const RECOVERY_SECONDS: i64 = 7 * 24 * 3600;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Request {
    Hello {
        vault: String,
        grant: Grant,
        token: String,
        #[serde(default)]
        streaming: bool,
    },
    Push {
        packet: Packet,
    },
    Next {
        after: i64,
    },
    Subscribe {
        after: i64,
    },
    CatchUp,
    Continue,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Reply {
    Ready {
        #[serde(default)]
        streaming: bool,
    },
    Ack,
    Changed,
    Event {
        seq: i64,
        packet: Packet,
    },
    Idle {
        checkpoint: Option<String>,
        #[serde(default)]
        caught_up: bool,
    },
    Window,
    Error {
        reason: String,
    },
}

#[derive(Default)]
struct Subscription {
    after: i64,
    catching_up: bool,
    draining: bool,
    paused: bool,
    events: usize,
    bytes: usize,
}

impl Subscription {
    async fn deliver(&mut self, ws: &mut WebSocketStream<TcpStream>, reply: Reply) -> Result<()> {
        match reply {
            Reply::Event { seq, packet } => {
                self.after = seq;
                self.events += 1;
                self.bytes += packet.data.len();
                stream::write_packet(ws, packet, |packet| Reply::Event { seq, packet }).await?;
                // A receiver persists each window before the next one is sent.
                if self.events >= 16 || self.bytes >= 1024 * 1024 {
                    self.paused = true;
                    stream::json(ws, &Reply::Window).await?;
                }
            }
            Reply::Idle { checkpoint, .. } => {
                self.draining = false;
                let caught_up = std::mem::take(&mut self.catching_up);
                stream::json(
                    ws,
                    &Reply::Idle {
                        checkpoint,
                        caught_up,
                    },
                )
                .await?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Relay {
    owner: String,
    db: Arc<Mutex<Connection>>,
    notify: watch::Sender<u64>,
    threshold: usize,
}

impl Relay {
    pub fn open(path: &Path, owner: &str, threshold: usize) -> Result<Self> {
        ensure!(
            is_hex(owner, 64) && threshold > 0,
            "invalid owner key or checkpoint interval"
        );
        let c = database(path)?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS config(owner TEXT PRIMARY KEY);
          CREATE TABLE IF NOT EXISTS revoked(device TEXT PRIMARY KEY);
          CREATE TABLE IF NOT EXISTS epochs(epoch INTEGER PRIMARY KEY);
          CREATE TABLE IF NOT EXISTS coverage(room TEXT,peer TEXT,end_counter INTEGER NOT NULL,PRIMARY KEY(room,peer));
          CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY AUTOINCREMENT, room TEXT NOT NULL,
            part INTEGER NOT NULL,digest BLOB NOT NULL UNIQUE,packet TEXT NOT NULL,peer TEXT,end_counter INTEGER,
            snapshot INTEGER NOT NULL, retired INTEGER);
          CREATE INDEX IF NOT EXISTS active_tail ON events(room,peer,end_counter) WHERE retired IS NULL;
          CREATE UNIQUE INDEX IF NOT EXISTS blob_parts ON events(room,part) WHERE part>=0;")?;
        c.execute(
            "INSERT INTO config(owner) SELECT ? WHERE NOT EXISTS(SELECT 1 FROM config)",
            [owner],
        )?;
        let existing: String = c.query_row("SELECT owner FROM config", [], |r| r.get(0))?;
        ensure!(existing == owner, "relay database belongs to another vault");
        let (notify, _) = watch::channel(0);
        Ok(Self {
            owner: owner.into(),
            db: Arc::new(Mutex::new(c)),
            notify,
            threshold,
        })
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| anyhow!("relay database lock failed"))
    }

    pub fn revoke(&self, device: &str) -> Result<()> {
        ensure!(is_hex(device, 64), "invalid device id");
        self.conn()?
            .execute("INSERT OR IGNORE INTO revoked VALUES(?)", [device])?;
        Ok(())
    }

    fn authorized(&self, g: &Grant) -> Result<()> {
        let c = self.conn()?;
        ensure!(
            !c.prepare("SELECT 1 FROM revoked WHERE device=?")?
                .exists([&g.device])?
                && !c
                    .prepare("SELECT 1 FROM epochs WHERE epoch>?")?
                    .exists([g.epoch])?,
            "device needs new credentials"
        );
        Ok(())
    }

    fn login(&self, vault: &str, g: &Grant, token: &str) -> Result<()> {
        ensure!(vault == self.owner, "unknown vault");
        g.verify(vault)?;
        let token = unb64(token)?;
        ensure!(
            token.len() == 32 && hex::encode(Sha256::digest(token)) == g.token_hash,
            "invalid credential"
        );
        self.authorized(g)?;
        self.conn()?
            .execute("INSERT OR IGNORE INTO epochs VALUES(?)", [g.epoch])?;
        Ok(())
    }

    fn push(&self, device: &str, p: Packet) -> Result<()> {
        ensure!(p.grant.device == device, "invalid writer");
        let data = p.verify(&self.owner)?;
        let hash = Packet::digest(&p.room, p.part, &data);
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        ensure!(
            !tx.prepare("SELECT 1 FROM revoked WHERE device=?")?
                .exists([device])?
                && !tx
                    .prepare("SELECT 1 FROM epochs WHERE epoch>?")?
                    .exists([p.grant.epoch])?,
            "device revoked"
        );
        if tx
            .prepare("SELECT 1 FROM events WHERE digest=?")?
            .exists([hash.as_slice()])?
        {
            return Ok(());
        }
        let mut peer = None;
        let mut end = None;
        let mut snapshot = false;
        if p.part == DOCUMENT {
            ensure!(
                record::epoch(&data)? == p.grant.epoch,
                "obsolete document key"
            );
            match record::parse(&data)?.header {
                EloHeader::Delta(h) => {
                    let writer = String::from_utf8(h.peer_id)?;
                    ensure!(
                        writer.parse::<u64>()?.to_string() == writer && h.end <= i32::MAX as u64,
                        "invalid delta coverage"
                    );
                    let covered = tx
                        .prepare(
                            "SELECT 1 FROM coverage WHERE room=? AND peer=? AND end_counter>=?",
                        )?
                        .exists(params![p.room, writer, i64::try_from(h.end)?])?;
                    if covered {
                        return Ok(());
                    }
                    peer = Some(writer);
                    end = Some(i64::try_from(h.end)?);
                }
                EloHeader::Snapshot(h) => {
                    let vv = record::from_header(h.vv)?;
                    // A racing checkpoint may cover a different branch. Keep the current base
                    // and all uncovered deltas until a device has integrated both.
                    let mut previous =
                        tx.prepare("SELECT peer,end_counter FROM coverage WHERE room=?")?;
                    let mut rows = previous.query([&p.room])?;
                    while let Some(row) = rows.next()? {
                        if vv.get(&row.get::<_, String>(0)?).copied().unwrap_or(0)
                            < u64::try_from(row.get::<_, i64>(1)?)?
                        {
                            return Ok(());
                        }
                    }
                    drop(rows);
                    drop(previous);
                    snapshot = true;
                    tx.execute("UPDATE events SET retired=unixepoch() WHERE room=? AND snapshot=1 AND retired IS NULL",[&p.room])?;
                    for (writer, n) in vv {
                        tx.execute(
                            "INSERT OR REPLACE INTO coverage VALUES(?,?,?)",
                            params![p.room, writer, i64::try_from(n)?],
                        )?;
                        tx.execute("UPDATE events SET retired=unixepoch() WHERE room=? AND peer=? AND end_counter<=? AND retired IS NULL",
                            params![p.room,writer,i64::try_from(n)?])?;
                    }
                }
            }
        }
        tx.execute("INSERT INTO events(room,part,digest,packet,peer,end_counter,snapshot) VALUES(?,?,?,?,?,?,?)",
            params![p.room,p.part,hash.as_slice(),serde_json::to_string(&p)?,peer,end,snapshot])?;
        tx.commit()?;
        self.notify.send_modify(|n| *n = n.wrapping_add(1));
        Ok(())
    }

    fn next(&self, after: i64) -> Result<Reply> {
        ensure!(after >= 0, "invalid cursor");
        let c = self.conn()?;
        let next:Option<(i64,String)>=c.query_row("SELECT seq,packet FROM events WHERE retired IS NULL AND seq>? ORDER BY seq LIMIT 1",[after],
            |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((seq, s)) = next {
            return Ok(Reply::Event {
                seq,
                packet: serde_json::from_str(&s)?,
            });
        }
        let head: i64 = c.query_row("SELECT coalesce(max(seq),0) FROM events", [], |r| r.get(0))?;
        ensure!(
            head >= after,
            "relay is behind the last durable device cursor"
        );
        let checkpoint=c.query_row("SELECT room FROM events WHERE retired IS NULL AND peer IS NOT NULL GROUP BY room HAVING count(*)>=? ORDER BY min(seq) LIMIT 1",
            [self.threshold as i64],|r|r.get(0)).optional()?;
        Ok(Reply::Idle {
            checkpoint,
            caught_up: false,
        })
    }

    pub async fn serve(self, listener: TcpListener) -> Result<()> {
        ensure!(
            listener.local_addr()?.ip().is_loopback(),
            "bind the relay to loopback behind a TLS reverse proxy"
        );
        let slots = Arc::new(Semaphore::new(128));
        let mut gc = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tokio::select! {
                _=gc.tick()=>{ self.conn()?.execute("DELETE FROM events WHERE retired < unixepoch()-?",[RECOVERY_SECONDS])?; }
                accepted=listener.accept() => {
                    let (socket,_)=accepted?;
                    if let Ok(slot)=slots.clone().try_acquire_owned() {
                        let relay=self.clone();
                        tokio::spawn(async move{let _slot=slot; let _=relay.session(socket).await;});
                    }
                }
            }
        }
    }

    async fn session(&self, socket: TcpStream) -> Result<()> {
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_WIRE))
            .max_frame_size(Some(MAX_WIRE));
        let mut ws = tokio::time::timeout(
            Duration::from_secs(10),
            accept_async_with_config(socket, Some(config)),
        )
        .await??;
        let hello = tokio::time::timeout(Duration::from_secs(10), stream::frame(&mut ws)).await??;
        let Request::Hello {
            vault,
            grant,
            token,
            streaming,
        } = serde_json::from_slice(&hello.into_data())?
        else {
            anyhow::bail!("authentication required")
        };
        if self.login(&vault, &grant, &token).is_err() {
            stream::json(
                &mut ws,
                &Reply::Error {
                    reason: "enrollment rejected".into(),
                },
            )
            .await?;
            return Ok(());
        }
        let mut changes = self.notify.subscribe();
        stream::json(&mut ws, &Reply::Ready { streaming }).await?;
        let mut subscription: Option<Subscription> = None;
        let mut last_input = tokio::time::Instant::now();
        let period = Duration::from_secs(30);
        let mut heartbeat = tokio::time::interval_at(last_input + period, period);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let sending = subscription
                .as_ref()
                .is_some_and(|s| s.draining && !s.paused);
            let frame = tokio::select! {
                incoming = ws.next() => {
                    let Some(frame) = incoming else { return Ok(()) };
                    last_input = tokio::time::Instant::now();
                    match frame? {
                        Message::Text(s) if s == "ping" => {
                            ws.send(Message::Text("pong".into())).await?;
                            continue;
                        }
                        Message::Text(s) if s == "pong" => continue,
                        m @ (Message::Text(_) | Message::Binary(_)) => m,
                        Message::Close(_) => return Ok(()),
                        _ => continue,
                    }
                }
                _ = heartbeat.tick() => {
                    ensure!(last_input.elapsed() < 3 * period, "idle connection timed out");
                    self.authorized(&grant)?;
                    ws.send(Message::Ping(Default::default())).await?;
                    continue;
                }
                changed = changes.changed() => {
                    changed?;
                    self.authorized(&grant)?;
                    if let Some(s) = &mut subscription {
                        s.draining = true;
                    } else if !streaming {
                        stream::json(&mut ws, &Reply::Changed).await?;
                    }
                    continue;
                }
                () = std::future::ready(()), if sending => {
                    self.authorized(&grant)?;
                    let s = subscription.as_mut().expect("active subscription");
                    s.deliver(&mut ws, self.next(s.after)?).await?;
                    continue;
                }
            };
            self.authorized(&grant)?;
            let request: Request = serde_json::from_slice(&frame.into_data())?;
            match request {
                Request::Subscribe { after } if streaming && subscription.is_none() => {
                    ensure!(after >= 0, "invalid stream cursor");
                    subscription = Some(Subscription {
                        after,
                        draining: true,
                        catching_up: true,
                        ..Default::default()
                    });
                }
                Request::CatchUp if streaming => {
                    let s = subscription.as_mut().context("subscription required")?;
                    s.catching_up = true;
                    s.draining = true;
                }
                Request::Continue if streaming => {
                    let s = subscription.as_mut().context("subscription required")?;
                    if s.paused {
                        s.paused = false;
                        s.events = 0;
                        s.bytes = 0;
                    }
                }
                Request::Next { after } if !streaming => {
                    stream::json(&mut ws, &self.next(after)?).await?
                }
                Request::Push { mut packet } => {
                    if streaming && packet.part == DOCUMENT {
                        let batch = stream::read_document(&mut ws, &mut packet).await?;
                        last_input = tokio::time::Instant::now();
                        let room = packet.room.clone();
                        let result = self.push(&grant.device, packet);
                        let status = if result.is_ok() {
                            UpdateStatusCode::Ok
                        } else {
                            UpdateStatusCode::InvalidUpdate
                        };
                        stream::ack(&mut ws, room, batch, status).await?;
                        result?;
                    } else {
                        self.push(&grant.device, packet)?;
                        stream::json(&mut ws, &Reply::Ack).await?;
                    }
                }
                _ => anyhow::bail!("unexpected relay request"),
            }
        }
    }

    pub async fn listen(self, addr: SocketAddr) -> Result<()> {
        ensure!(
            addr.ip().is_loopback(),
            "use a loopback listener behind TLS"
        );
        let listener = TcpListener::bind(addr).await?;
        eprintln!("Listening on {}", listener.local_addr()?);
        self.serve(listener).await
    }
}
