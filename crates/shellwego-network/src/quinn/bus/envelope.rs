//! Bus message envelope framing for QUIC streams.
//!
//! Provides serialization/deserialization of `BusMessage` envelopes
//! with length-prefix encoding over QUIC bidirectional streams.

use anyhow::{Context, Result};
use bytes::{Buf, BufMut, BytesMut};
use shellwego_schema::{BusMessage, BusMessageEnvelope};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Wire format header for bus messages.
///
/// Layout:
///   [u32: total_len] [u8: version] [u16: topic_len] [topic_bytes] [envelope_bytes]
///
/// The version byte allows future wire format changes.
pub const WIRE_VERSION: u8 = 1;

/// Encode a `BusMessage` into wire format bytes.
///
/// Returns the complete frame ready to be written to a QUIC stream.
pub fn encode_bus_message(msg: &BusMessage) -> Result<Vec<u8>> {
    let envelope = msg
        .to_envelope()
        .context("Failed to create bus message envelope")?;
    let envelope_bytes = postcard::to_allocvec(&envelope).context("Failed to serialize envelope")?;

    let topic = msg.topic.as_str();
    let topic_bytes = topic.as_bytes();
    let topic_len = topic_bytes.len() as u16;

    // total_len = 1 (version) + 2 (topic_len) + topic_bytes.len() + envelope_bytes.len()
    let total_len = 1u32 + 2 + topic_bytes.len() as u32 + envelope_bytes.len() as u32;

    let mut buf = BytesMut::with_capacity(4 + total_len as usize);
    buf.put_u32_le(total_len);
    buf.put_u8(WIRE_VERSION);
    buf.put_u16_le(topic_len);
    buf.extend_from_slice(topic_bytes);
    buf.extend_from_slice(&envelope_bytes);

    Ok(buf.freeze().to_vec())
}

/// Decode a `BusMessage` from wire format bytes.
///
/// Parses the frame header, extracts the topic and envelope, then
/// deserializes into a full `BusMessage`.
pub fn decode_bus_message(data: &[u8]) -> Result<BusMessage> {
    let mut buf = bytes::Bytes::copy_from_slice(data);

    let total_len = buf.get_u32_le() as usize;
    if data.len() < 4 + total_len {
        anyhow::bail!(
            "Incomplete frame: expected {} bytes, got {}",
            4 + total_len,
            data.len()
        );
    }

    let version = buf.get_u8();
    if version != WIRE_VERSION {
        anyhow::bail!("Unsupported wire version: {}", version);
    }

    let topic_len = buf.get_u16_le() as usize;
    let topic_bytes = &buf[..topic_len];
    let topic_str =
        std::str::from_utf8(topic_bytes).context("Invalid UTF-8 in topic")?;
    buf.advance(topic_len);

    let envelope_bytes = &buf[..];
    let envelope: BusMessageEnvelope =
        postcard::from_bytes(envelope_bytes).context("Failed to deserialize envelope")?;

    let bus_msg = BusMessage::from_envelope(envelope).context("Failed to build bus message")?;

    // Verify topic consistency between header and envelope
    if bus_msg.topic.as_str() != topic_str {
        anyhow::bail!(
            "Topic mismatch: header says '{}', envelope says '{}'",
            topic_str,
            bus_msg.topic.as_str()
        );
    }

    Ok(bus_msg)
}

/// Async write a bus message frame to a QUIC send stream.
pub async fn write_bus_message(
    stream: &mut quinn::SendStream,
    msg: &BusMessage,
) -> Result<()> {
    let frame = encode_bus_message(msg)?;
    stream
        .write_all(&frame)
        .await
        .context("Failed to write bus message frame")?;
    stream.finish().context("Failed to finish stream")?;
    Ok(())
}

/// Async read a bus message frame from a QUIC recv stream.
pub async fn read_bus_message(
    stream: &mut quinn::RecvStream,
    max_size: usize,
) -> Result<BusMessage> {
    let data = stream
        .read_to_end(max_size)
        .await
        .context("Failed to read bus message frame")?;
    decode_bus_message(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellwego_schema::{ChannelPriority, Message, Topic};

    #[test]
    fn test_encode_decode_roundtrip() {
        let topic = Topic::new("agent.heartbeat").unwrap();
        let msg = BusMessage::new(
            topic.clone(),
            Message::Heartbeat {
                node_id: uuid::Uuid::new_v4(),
                cpu_usage: 0.5,
                memory_usage: 0.3,
            },
            ChannelPriority::Metrics,
        )
        .with_source(uuid::Uuid::new_v4());

        let encoded = encode_bus_message(&msg).unwrap();
        let decoded = decode_bus_message(&encoded).unwrap();

        assert_eq!(decoded.msg_id, msg.msg_id);
        assert_eq!(decoded.topic.as_str(), "agent.heartbeat");
        assert_eq!(decoded.priority, ChannelPriority::Metrics);
        assert_eq!(decoded.source_node, msg.source_node);
    }

    #[test]
    fn test_decode_rejects_wrong_version() {
        let topic = Topic::new("test.topic").unwrap();
        let msg = BusMessage::new(topic, Message::Ping { timestamp: chrono::Utc::now() }, ChannelPriority::Command);
        let mut encoded = encode_bus_message(&msg).unwrap();
        // Corrupt the version byte (at offset 4)
        encoded[4] = 99;
        assert!(decode_bus_message(&encoded).is_err());
    }

    #[test]
    fn test_decode_rejects_truncated_frame() {
        let data = [0x05, 0x00, 0x00, 0x00, 0x01]; // claims 5 bytes but only 1 after header
        assert!(decode_bus_message(&data).is_err());
    }
}
