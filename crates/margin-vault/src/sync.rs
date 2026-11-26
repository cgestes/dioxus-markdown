use super::{
    Vault, b64,
    record::DOCUMENT,
    relay::{Reply, Request},
    stream,
};
use anyhow::{Result, bail, ensure};
use loro_protocol::{BatchId, MAX_MESSAGE_SIZE, ProtocolMessage, UpdateStatusCode};
use std::{net::IpAddr, time::Duration};
use tokio::{net::TcpStream, sync::watch};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use url::Url;

pub fn validate_url(raw: &str) -> Result<()> {
    let u = Url::parse(raw)?;
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none(),
        "relay URL must not contain credentials, query or fragment"
    );
    let local = match u.host() {
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        _ => false,
    };
    ensure!(
        u.scheme() == "wss" || (u.scheme() == "ws" && local),
        "use wss:// for remote relays or ws:// with a loopback IP"
    );
    Ok(())
}

/// File pushes use a text ACK. Every document ACK must match its room and Loro batch.
enum Notice {
    Applied,
    Idle { caught_up: bool },
    Ack(Option<(String, BatchId)>),
}

struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    checkpoint: Option<String>,
}
impl Client {
    async fn connect(v: &Vault, url: &str) -> Result<Self> {
        validate_url(url)?;
        let cfg = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_SIZE))
            .max_frame_size(Some(MAX_MESSAGE_SIZE));
        tokio::time::timeout(Duration::from_secs(12), async {
            let (mut ws, _) = connect_async_with_config(url, Some(cfg), true).await?;
            let p = &v.id.paired;
            stream::json(
                &mut ws,
                &Request::Hello {
                    vault: p.vault.clone(),
                    grant: p.grant.clone(),
                    token: b64(&p.token),
                    streaming: true,
                },
            )
            .await?;
            let hello: Reply = serde_json::from_slice(&stream::frame(&mut ws).await?.into_data())?;
            ensure!(
                matches!(hello, Reply::Ready { streaming: true }),
                "relay did not accept encrypted streaming"
            );
            Ok(Self {
                ws,
                checkpoint: None,
            })
        })
        .await?
    }

    async fn receive(&mut self, v: &Vault, frame: Message) -> Result<Notice> {
        if let Message::Binary(bytes) = frame {
            let ProtocolMessage::Ack {
                room_id,
                ref_id,
                status,
                ..
            } = stream::decode_frame(&bytes)?
            else {
                bail!("unexpected binary relay response")
            };
            ensure!(
                status == UpdateStatusCode::Ok,
                "document rejected: {status:?}"
            );
            return Ok(Notice::Ack(Some((room_id, ref_id))));
        }
        match serde_json::from_slice(&frame.into_data())? {
            Reply::Event { seq, mut packet } => {
                ensure!(seq > 0, "invalid relay cursor");
                if packet.part == DOCUMENT {
                    stream::read_document(&mut self.ws, &mut packet).await?;
                }
                v.receive(seq, &packet)?;
                Ok(Notice::Applied)
            }
            Reply::Window => {
                stream::json(&mut self.ws, &Request::Continue).await?;
                Ok(Notice::Applied)
            }
            Reply::Idle {
                checkpoint,
                caught_up,
            } => {
                self.checkpoint = checkpoint;
                Ok(Notice::Idle { caught_up })
            }
            Reply::Ack => Ok(Notice::Ack(None)),
            Reply::Error { reason } => bail!("relay: {reason}"),
            _ => bail!("unexpected relay response"),
        }
    }

    async fn next(&mut self, v: &Vault) -> Result<Notice> {
        let frame =
            tokio::time::timeout(Duration::from_secs(30), stream::frame(&mut self.ws)).await??;
        self.receive(v, frame).await
    }

    async fn upload(&mut self, v: &Vault) -> Result<()> {
        while let Some(row) = v.pending()? {
            let expected = stream::write_packet(&mut self.ws, v.outbound(&row)?, |packet| {
                Request::Push { packet }
            })
            .await?;
            loop {
                if let Notice::Ack(id) = self.next(v).await? {
                    ensure!(id == expected, "acknowledgement for another upload");
                    break;
                }
            }
            v.ack(row.seq)?;
        }
        Ok(())
    }

    async fn catch_up(&mut self, v: &Vault) -> Result<()> {
        let mut attempted = std::collections::HashSet::new();
        self.upload(v).await?;
        stream::json(&mut self.ws, &Request::Subscribe { after: v.cursor()? }).await?;
        loop {
            loop {
                match self.next(v).await? {
                    Notice::Idle { caught_up: true } => break,
                    Notice::Ack(_) => bail!("unexpected upload acknowledgement"),
                    _ => {}
                }
            }
            let Some(room) = self.checkpoint.take() else {
                return Ok(());
            };
            if !attempted.insert(room.clone()) || !v.shared(&room)? || !v.checkpoint(&room)? {
                return Ok(());
            }
            self.upload(v).await?;
            // A one-shot sync waits for its checkpoint to reach the same stream.
            stream::json(&mut self.ws, &Request::CatchUp).await?;
        }
    }

    async fn follow(&mut self, v: &Vault, status: &impl Fn(&str)) -> Result<()> {
        self.catch_up(v).await?;
        status("Synced");
        loop {
            self.upload(v).await?;
            if let Some(room) = self.checkpoint.take()
                && v.shared(&room)?
                && v.checkpoint(&room)?
            {
                continue;
            }
            // Another process may write the same SQLite outbox without waking this process.
            // The timer only checks local storage. Incoming edits arrive on the socket.
            tokio::select! {
                frame = stream::frame(&mut self.ws) => {
                    // Finish any multi-frame record before listening for local wakeups again.
                    if let Notice::Ack(_) = self.receive(v, frame?).await? {
                        bail!("unexpected upload acknowledgement");
                    }
                }
                _ = v.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            }
        }
    }
}

pub async fn once(v: &Vault, url: &str) -> Result<()> {
    Client::connect(v, url).await?.catch_up(v).await
}

/// Reconnect from the local durable cursor and retry every unacknowledged upload.
pub async fn follow(v: Vault, url: String, mut stop: watch::Receiver<bool>, status: impl Fn(&str)) {
    let run = async {
        loop {
            let session =
                async { Client::connect(&v, &url).await?.follow(&v, &status).await }.await;
            if let Err(e) = session {
                status(&format!("Offline · {e}"));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    };
    tokio::select! {_=run=>{},_=stop.wait_for(|&v|v)=>{}}
}
