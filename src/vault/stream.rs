//! One signed document record per Loro batch. Margin's text frame carries its
//! device signature and delivery cursor. The following binary frames are %ELO.
use super::{
    b64, is_hex, keys,
    record::{DOCUMENT, MAX_RECORD, Packet},
    unb64,
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use loro_protocol::{
    BatchId, BytesReader, BytesWriter, CrdtType, MAX_MESSAGE_SIZE, MessageType, ProtocolMessage,
    UpdateStatusCode, decode, decode_elo_container, encode,
};
use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

const FRAGMENT_BYTES: usize = MAX_MESSAGE_SIZE - 256;
const FRAGMENT_TIMEOUT: Duration = Duration::from_secs(10);
// One count byte and a varint length for a record of at most 16 MiB.
const MAX_CONTAINER: usize = 1 + 4 + MAX_RECORD;

pub(super) async fn json<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
    value: &impl Serialize,
) -> Result<()> {
    ws.send(Message::Text(serde_json::to_string(value)?.into()))
        .await?;
    Ok(())
}

pub(super) async fn frame<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
) -> Result<Message> {
    loop {
        match ws.next().await.context("relay disconnected")?? {
            Message::Text(s) if s == "ping" => ws.send(Message::Text("pong".into())).await?,
            Message::Text(s) if s == "pong" => {}
            m @ (Message::Text(_) | Message::Binary(_)) => return Ok(m),
            Message::Close(_) => bail!("relay disconnected"),
            _ => {}
        }
    }
}

async fn binary<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
    message: ProtocolMessage,
) -> Result<()> {
    let bytes = encode(&message).map_err(anyhow::Error::msg)?;
    ws.send(Message::Binary(bytes.into())).await?;
    Ok(())
}

// Bound the record count before the upstream decoder allocates its update vector.
pub(super) fn decode_frame(bytes: &[u8]) -> Result<ProtocolMessage> {
    ensure!(bytes.len() <= MAX_MESSAGE_SIZE, "oversized Loro frame");
    let mut r = BytesReader::new(bytes);
    let err = anyhow::Error::msg;
    ensure!(
        r.read_bytes(4).map_err(err)? == b"%ELO",
        "expected %ELO frame"
    );
    ensure!(
        is_hex(&r.read_var_string().map_err(err)?, 32),
        "invalid Loro room"
    );
    let kind = r.read_byte().map_err(err)?;
    if kind == MessageType::DocUpdate as u8 {
        ensure!(
            r.read_uleb128().map_err(err)? == 1,
            "expected one signed record"
        );
        r.read_var_bytes().map_err(err)?;
        ensure!(r.remaining() == 8, "invalid Loro update batch ID");
    } else if kind == MessageType::Ack as u8 {
        ensure!(r.remaining() == 9, "invalid Loro acknowledgement");
    }
    decode(bytes).map_err(err)
}

pub(super) async fn write_packet<S, T>(
    ws: &mut WebSocketStream<S>,
    mut packet: Packet,
    header: impl FnOnce(Packet) -> T,
) -> Result<Option<(String, BatchId)>>
where
    S: AsyncRead + AsyncWrite + Unpin,
    T: Serialize,
{
    if packet.part != DOCUMENT {
        json(ws, &header(packet)).await?;
        return Ok(None);
    }
    let room_id = packet.room.clone();
    let record = unb64(&std::mem::take(&mut packet.data))?;
    ensure!(
        !record.is_empty() && record.len() <= MAX_RECORD,
        "invalid document record size"
    );
    let batch_id = BatchId(keys::random());
    let crdt = CrdtType::Elo;
    json(ws, &header(packet)).await?;
    if record.len() <= FRAGMENT_BYTES {
        binary(
            ws,
            ProtocolMessage::DocUpdate {
                crdt,
                room_id: room_id.clone(),
                batch_id,
                updates: vec![record],
            },
        )
        .await?;
    } else {
        let mut payload = BytesWriter::new();
        payload.push_uleb128(1);
        payload.push_var_bytes(&record);
        drop(record);
        let payload = payload.finalize();
        let fragments = payload.chunks(FRAGMENT_BYTES);
        binary(
            ws,
            ProtocolMessage::DocUpdateFragmentHeader {
                crdt,
                room_id: room_id.clone(),
                batch_id,
                fragment_count: fragments.len() as u64,
                total_size_bytes: payload.len() as u64,
            },
        )
        .await?;
        for (index, fragment) in fragments.enumerate() {
            binary(
                ws,
                ProtocolMessage::DocUpdateFragment {
                    crdt,
                    room_id: room_id.clone(),
                    batch_id,
                    index: index as u64,
                    fragment: fragment.into(),
                },
            )
            .await?;
        }
    }
    Ok(Some((room_id, batch_id)))
}

async fn read_binary<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
) -> Result<ProtocolMessage> {
    let Message::Binary(bytes) = frame(ws).await? else {
        bail!("expected binary %ELO update")
    };
    decode_frame(&bytes)
}

pub(super) async fn ack<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
    room_id: String,
    ref_id: BatchId,
    status: UpdateStatusCode,
) -> Result<()> {
    binary(
        ws,
        ProtocolMessage::Ack {
            crdt: CrdtType::Elo,
            room_id,
            ref_id,
            status,
        },
    )
    .await
}

pub(super) async fn read_document<S: AsyncRead + AsyncWrite + Unpin>(
    ws: &mut WebSocketStream<S>,
    packet: &mut Packet,
) -> Result<BatchId> {
    ensure!(
        packet.part == DOCUMENT && packet.data.is_empty() && is_hex(&packet.room, 32),
        "invalid document header"
    );
    let msg = tokio::time::timeout(FRAGMENT_TIMEOUT, read_binary(ws)).await??;
    match msg {
        ProtocolMessage::DocUpdate {
            room_id,
            batch_id,
            updates,
            ..
        } if room_id == packet.room => {
            let record = &updates[0];
            ensure!(!record.is_empty(), "empty document record");
            packet.data = b64(record);
            Ok(batch_id)
        }
        ProtocolMessage::DocUpdateFragmentHeader {
            room_id,
            batch_id,
            total_size_bytes,
            fragment_count,
            ..
        } if room_id == packet.room => {
            ensure!(
                (1..=MAX_CONTAINER as u64).contains(&total_size_bytes)
                    && (1..=total_size_bytes.min(4096)).contains(&fragment_count),
                "invalid Loro fragment bounds"
            );
            let mut payload = Vec::new();
            for next in 0..fragment_count {
                let message = match tokio::time::timeout(FRAGMENT_TIMEOUT, read_binary(ws)).await {
                    Ok(result) => result?,
                    Err(_) => {
                        ack(ws, room_id, batch_id, UpdateStatusCode::FragmentTimeout).await?;
                        bail!("Loro fragment timeout");
                    }
                };
                let ProtocolMessage::DocUpdateFragment {
                    room_id,
                    batch_id: id,
                    index,
                    fragment,
                    ..
                } = message
                else {
                    bail!("expected Loro fragment")
                };
                ensure!(
                    room_id == packet.room
                        && id == batch_id
                        && index == next
                        && !fragment.is_empty()
                        && (payload.len() + fragment.len()) as u64 <= total_size_bytes,
                    "invalid Loro fragment"
                );
                payload.extend_from_slice(&fragment);
            }
            ensure!(
                payload.len() as u64 == total_size_bytes && payload.first() == Some(&1),
                "incomplete Loro container"
            );
            let records = decode_elo_container(&payload).map_err(anyhow::Error::msg)?;
            ensure!(
                !records[0].is_empty() && records[0].len() <= MAX_RECORD,
                "invalid document record size"
            );
            packet.data = b64(records[0]);
            Ok(batch_id)
        }
        _ => bail!("unexpected Loro update or room"),
    }
}
