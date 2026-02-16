use bcp_types::block::{Block, BlockContent};
use bcp_types::block_type::BlockType;
use bcp_types::summary::Summary;
use bcp_wire::block_frame::BlockFlags;
use bcp_wire::header::{LcpHeader, HEADER_SIZE};
use bcp_wire::varint::decode_varint;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::error::DecodeError;

/// Events emitted by the streaming decoder.
///
/// The stream yields a `Header` event first (once the 8-byte file
/// header has been read and validated), then a sequence of `Block`
/// events for each decoded block, terminating when the END sentinel
/// is encountered.
///
/// ```text
///   Header(LcpHeader)
///   Block(Block)
///   Block(Block)
///   Block(Block)
///   ... (stream ends at END sentinel)
/// ```
#[derive(Clone, Debug)]
pub enum DecoderEvent {
  /// The file header has been parsed and validated.
  Header(LcpHeader),

  /// A block has been fully decoded.
  Block(Block),
}

/// Asynchronous streaming decoder — yields blocks one at a time
/// without buffering the entire payload.
///
/// This is the primary API for large payloads or network streams.
/// The decoder reads the header first, then yields blocks as they
/// are fully received. Backpressure is handled naturally: the stream
/// only reads the next block when the caller awaits the next item.
///
/// Unlike the synchronous [`LcpDecoder`](crate::LcpDecoder) which
/// requires the entire payload in memory, `StreamingDecoder` reads
/// incrementally from any `AsyncRead` source (files, TCP sockets,
/// HTTP response bodies, etc.).
///
/// # Example
///
/// ```rust,no_run
/// use bcp_decoder::StreamingDecoder;
/// use tokio::io::AsyncRead;
///
/// async fn decode_from_reader(reader: impl AsyncRead + Unpin) {
///     let mut stream = StreamingDecoder::new(reader);
///     while let Some(event) = stream.next().await.transpose().unwrap() {
///         // Process each DecoderEvent...
///     }
/// }
/// ```
pub struct StreamingDecoder<R> {
  reader: R,
  state: StreamState,
  /// Internal read buffer. Block bodies are read into this buffer
  /// before being parsed. The buffer is reused across blocks to
  /// avoid repeated allocations.
  buf: Vec<u8>,
}

/// Internal state machine for the streaming decoder.
///
/// The decoder progresses through three states:
///
/// ```text
///   ReadHeader → ReadBlocks → Done
/// ```
///
/// `ReadHeader` is the initial state. After the header is read, the
/// decoder transitions to `ReadBlocks` and stays there until the END
/// sentinel is encountered, at which point it transitions to `Done`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamState {
  ReadHeader,
  ReadBlocks,
  Done,
}

impl<R: AsyncRead + Unpin> StreamingDecoder<R> {
  /// Create a new streaming decoder over the given async reader.
  ///
  /// The decoder starts in `ReadHeader` state and will read the
  /// 8-byte file header on the first call to [`next`](Self::next).
  #[must_use]
  pub fn new(reader: R) -> Self {
    Self {
      reader,
      state: StreamState::ReadHeader,
      buf: Vec::with_capacity(4096),
    }
  }

  /// Read the next event from the stream.
  ///
  /// Returns `Ok(Some(event))` for each decoded event, `Ok(None)`
  /// when the stream is exhausted (END sentinel reached), or `Err`
  /// on any decode error.
  ///
  /// The first call always yields `DecoderEvent::Header`. Subsequent
  /// calls yield `DecoderEvent::Block` until the END sentinel.
  pub async fn next(&mut self) -> Option<Result<DecoderEvent, DecodeError>> {
    match self.state {
      StreamState::ReadHeader => Some(self.read_header().await),
      StreamState::ReadBlocks => self.read_next_block().await,
      StreamState::Done => None,
    }
  }

  /// Read and validate the 8-byte file header.
  async fn read_header(&mut self) -> Result<DecoderEvent, DecodeError> {
    let mut header_buf = [0u8; HEADER_SIZE];
    self
      .reader
      .read_exact(&mut header_buf)
      .await
      .map_err(|_| {
        DecodeError::InvalidHeader(bcp_wire::WireError::UnexpectedEof {
          offset: 0,
        })
      })?;

    let header =
      LcpHeader::read_from(&header_buf).map_err(DecodeError::InvalidHeader)?;

    self.state = StreamState::ReadBlocks;
    Ok(DecoderEvent::Header(header))
  }

  /// Read the next block frame from the stream.
  ///
  /// Returns `None` when the END sentinel is encountered, transitioning
  /// the state to `Done`.
  async fn read_next_block(&mut self) -> Option<Result<DecoderEvent, DecodeError>> {
    // Read block_type varint
    let block_type_raw = match self.read_varint().await {
      Ok(v) => v,
      Err(e) => return Some(Err(e)),
    };

    #[allow(clippy::cast_possible_truncation)]
    let block_type_byte = block_type_raw as u8;

    // Check for END sentinel
    if block_type_byte == 0xFF {
      // Read and discard flags + content_len for the END frame
      match self.read_end_frame_tail().await {
        Ok(()) => {}
        Err(e) => return Some(Err(e)),
      }
      self.state = StreamState::Done;
      return None;
    }

    // Read flags (single byte)
    let mut flags_byte = [0u8; 1];
    if let Err(e) = self.reader.read_exact(&mut flags_byte).await {
      return Some(Err(DecodeError::Io(e)));
    }
    let flags = BlockFlags::from_raw(flags_byte[0]);

    // Read content_len varint
    #[allow(clippy::cast_possible_truncation)]
    let content_len = match self.read_varint().await {
      Ok(v) => v as usize,
      Err(e) => return Some(Err(e)),
    };

    // Read body bytes
    self.buf.clear();
    self.buf.resize(content_len, 0);
    if let Err(e) = self.reader.read_exact(&mut self.buf[..content_len]).await {
      return Some(Err(DecodeError::Io(e)));
    }

    // Decode the block
    let block_type = BlockType::from_wire_id(block_type_byte);
    let mut body = self.buf.as_slice();
    let mut summary = None;

    if flags.has_summary() {
      match Summary::decode(body) {
        Ok((sum, consumed)) => {
          summary = Some(sum);
          body = &body[consumed..];
        }
        Err(e) => return Some(Err(e.into())),
      }
    }

    let content = match BlockContent::decode_body(&block_type, body) {
      Ok(c) => c,
      Err(e) => return Some(Err(e.into())),
    };

    Some(Ok(DecoderEvent::Block(Block {
      block_type,
      flags,
      summary,
      content,
    })))
  }

  /// Read the trailing flags + `content_len` bytes of an END frame.
  ///
  /// The END sentinel has: `flags`=0x00 (1 byte) + `content_len`=0x00 (1 byte).
  /// We read and discard these to fully consume the END frame.
  async fn read_end_frame_tail(&mut self) -> Result<(), DecodeError> {
    // flags byte
    let mut byte = [0u8; 1];
    self.reader.read_exact(&mut byte).await.map_err(DecodeError::Io)?;

    // content_len varint (should be 0)
    let _content_len = self.read_varint().await?;
    Ok(())
  }

  /// Read a single varint from the async reader.
  ///
  /// Varints are read byte-by-byte: each byte's MSB indicates whether
  /// more bytes follow (1 = more, 0 = last byte). Maximum 10 bytes
  /// for a 64-bit value.
  async fn read_varint(&mut self) -> Result<u64, DecodeError> {
    let mut varint_buf = [0u8; 10];
    let mut len = 0;

    loop {
      let mut byte = [0u8; 1];
      self.reader.read_exact(&mut byte).await.map_err(DecodeError::Io)?;
      varint_buf[len] = byte[0];
      len += 1;

      // MSB clear means this is the last byte
      if byte[0] & 0x80 == 0 {
        break;
      }

      if len >= 10 {
        return Err(DecodeError::Wire(bcp_wire::WireError::VarintTooLong));
      }
    }

    let (value, _) = decode_varint(&varint_buf[..len])?;
    Ok(value)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bcp_encoder::LcpEncoder;
  use bcp_types::enums::{Lang, Priority, Role, Status};

  /// Helper: encode a payload and decode it via the streaming decoder,
  /// collecting all events into a Vec.
  async fn stream_roundtrip(encoder: &LcpEncoder) -> Vec<DecoderEvent> {
    let payload = encoder.encode().unwrap();
    let cursor = std::io::Cursor::new(payload);
    let reader = tokio::io::BufReader::new(cursor);

    let mut decoder = StreamingDecoder::new(reader);
    let mut events = Vec::new();

    while let Some(result) = decoder.next().await {
      events.push(result.unwrap());
    }

    events
  }

  #[tokio::test]
  async fn streaming_produces_header_then_blocks() {
    let mut enc = LcpEncoder::new();
    enc
      .add_code(Lang::Rust, "main.rs", b"fn main() {}")
      .add_conversation(Role::User, b"hello");
    let events = stream_roundtrip(&enc).await;

    assert_eq!(events.len(), 3); // Header + 2 blocks

    assert!(matches!(&events[0], DecoderEvent::Header(h) if h.version_major == 1));
    assert!(matches!(&events[1], DecoderEvent::Block(b) if b.block_type == BlockType::Code));
    assert!(
      matches!(&events[2], DecoderEvent::Block(b) if b.block_type == BlockType::Conversation)
    );
  }

  #[tokio::test]
  async fn streaming_matches_sync_decoder() {
    let mut encoder = LcpEncoder::new();
    encoder
      .add_code(Lang::Rust, "lib.rs", b"pub fn x() {}")
      .with_summary("Function x.")
      .with_priority(Priority::High)
      .add_conversation(Role::User, b"What does x do?")
      .add_tool_result("docs", Status::Ok, b"x is a placeholder.");

    let payload = encoder.encode().unwrap();

    // Sync decode
    let sync_decoded = crate::LcpDecoder::decode(&payload).unwrap();

    // Streaming decode
    let events = stream_roundtrip(&encoder).await;

    // Extract blocks from events (skip the Header event)
    let stream_blocks: Vec<_> = events
      .into_iter()
      .filter_map(|e| match e {
        DecoderEvent::Block(b) => Some(b),
        _ => None,
      })
      .collect();

    // Same number of blocks
    assert_eq!(sync_decoded.blocks.len(), stream_blocks.len());

    // Same block types in same order
    for (sync_block, stream_block) in sync_decoded.blocks.iter().zip(stream_blocks.iter()) {
      assert_eq!(sync_block.block_type, stream_block.block_type);
      assert_eq!(sync_block.flags, stream_block.flags);
      assert_eq!(sync_block.summary, stream_block.summary);
    }
  }

  #[tokio::test]
  async fn streaming_handles_summary_blocks() {
    let mut enc = LcpEncoder::new();
    enc
      .add_code(Lang::Python, "app.py", b"print('hi')")
      .with_summary("Prints a greeting.");
    let events = stream_roundtrip(&enc).await;

    let block = match &events[1] {
      DecoderEvent::Block(b) => b,
      other => panic!("expected Block, got {other:?}"),
    };

    assert!(block.flags.has_summary());
    assert_eq!(block.summary.as_ref().unwrap().text, "Prints a greeting.");
  }

  #[tokio::test]
  async fn streaming_empty_body_blocks() {
    let mut enc = LcpEncoder::new();
    enc.add_extension("ns", "t", b"");
    let events = stream_roundtrip(&enc).await;

    assert_eq!(events.len(), 2); // Header + Extension
  }

  #[tokio::test]
  async fn streaming_terminates_at_end_sentinel() {
    let mut enc = LcpEncoder::new();
    enc.add_conversation(Role::User, b"hi");
    let events = stream_roundtrip(&enc).await;

    // After all events, decoder should return None
    assert_eq!(events.len(), 2); // Header + 1 block
  }
}
