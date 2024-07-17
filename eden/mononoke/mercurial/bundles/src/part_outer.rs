/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! Codec to parse the bits that are the same for every bundle2, except for
//! stream-level parameters (see `stream_start` for those). This parses bundle2
//! part headers and puts together chunks for inner codecs to parse.

use std::mem;
use std::pin::Pin;

use anyhow::Error;
use anyhow::Result;
use byteorder::BigEndian;
use byteorder::ByteOrder;
use bytes::Buf;
use bytes::Bytes;
use bytes::BytesMut;
use slog::debug;
use slog::o;
use slog::Logger;
use tokio::io::AsyncBufRead;
use tokio_util::codec::Decoder;
use tokio_util::codec::FramedRead;

use crate::errors::ErrorKind;
use crate::part_header;
use crate::part_header::PartHeader;
use crate::part_header::PartHeaderType;
use crate::part_header::PartId;
use crate::part_inner::validate_header;
use crate::types::StreamHeader;
use crate::utils::Decompressor;

pub fn outer_stream<R: AsyncBufRead + Send + 'static>(
    logger: Logger,
    stream_header: &StreamHeader,
    read: R,
) -> Result<OuterStream<R>> {
    let compression = stream_header
        .m_stream_params
        .get("compression")
        .map(String::as_ref);

    Ok(Box::pin(FramedRead::new(
        Decompressor::new(read, compression)?,
        OuterDecoder::new(logger.new(o!("stream" => "outer"))),
    )))
}

pub type OuterStream<R> = Pin<Box<FramedRead<Decompressor<R>, OuterDecoder>>>;

#[derive(Debug)]
enum OuterState {
    Header,
    Payload {
        part_type: PartHeaderType,
        part_id: PartId,
    },
    DiscardPayload,
    StreamEnd,
    Invalid,
}

impl OuterState {
    pub fn take(&mut self) -> Self {
        mem::replace(self, OuterState::Invalid)
    }

    pub fn payload_frame(&self, data: BytesMut) -> OuterFrame {
        match *self {
            OuterState::Payload {
                ref part_type,
                ref part_id,
            } => OuterFrame::Payload {
                part_type: part_type.clone(),
                part_id: *part_id,
                payload: data.freeze(),
            },
            OuterState::DiscardPayload => OuterFrame::Discard,
            _ => panic!("payload_frame called for state without payloads"),
        }
    }

    pub fn part_end_frame(self) -> OuterFrame {
        match self {
            OuterState::Payload { part_type, part_id } => {
                OuterFrame::PartEnd { part_type, part_id }
            }
            OuterState::DiscardPayload => OuterFrame::Discard,
            _ => panic!("part_end_frame called for state without payloads"),
        }
    }
}

#[derive(Debug)]
pub struct OuterDecoder {
    logger: Logger,
    state: OuterState,
}

impl Decoder for OuterDecoder {
    // The decoder may be able to recover from errors, in which case the
    // decoded item will be `Err(_)` with the recoverable error.
    type Item = Result<OuterFrame>;
    // Unrecoverable errors are returned at the outer level.
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>> {
        let (ret, next_state) = Self::decode_next(&self.logger, buf, self.state.take());
        self.state = next_state;
        ret
    }
}

impl OuterDecoder {
    pub fn new(logger: Logger) -> Self {
        OuterDecoder {
            logger,
            state: OuterState::Header,
        }
    }

    /// Decode the next frame.
    ///
    /// Frame decoding may be recoverable in the case of application-level
    /// errors.  In which case, this function returns `Ok(Some(Err(_)))`, and
    /// the stream may continue.
    fn decode_next(
        logger: &Logger,
        buf: &mut BytesMut,
        mut state: OuterState,
    ) -> (Result<Option<Result<OuterFrame>>>, OuterState) {
        // TODO: the only state valid when the stream terminates is
        // StreamEnd. Communicate that to callers.
        match state.take() {
            OuterState::Header => {
                // The header is structured as:
                // ---
                // header_len: u32
                // header: header_len bytes
                // ---
                // See part_header::decode for information about the internal structure.
                if buf.len() < 4 {
                    return (Ok(None), OuterState::Header);
                }

                let header_len = BigEndian::read_u32(&buf[..4]) as usize;
                if buf.len() < 4 + header_len {
                    return (Ok(None), OuterState::Header);
                }

                let _ = buf.split_to(4);
                if header_len == 0 {
                    // A zero-length header indicates that the stream has ended.
                    return (Ok(Some(Ok(OuterFrame::StreamEnd))), OuterState::StreamEnd);
                }

                let part_header = Self::decode_header(logger, buf.split_to(header_len).freeze());
                if let Err(e) = part_header {
                    let next = match e.downcast::<ErrorKind>() {
                        Ok(ek) => {
                            if ek.is_app_error() {
                                (Ok(Some(Err(ek.into()))), OuterState::DiscardPayload)
                            } else {
                                (Err(ek.into()), OuterState::Invalid)
                            }
                        }
                        Err(e) => (Err(e), OuterState::Invalid),
                    };
                    return next;
                };
                let part_header = part_header.unwrap();
                // If no part header was returned, this part wasn't
                // recognized. Throw it away.
                match part_header {
                    None => (
                        Ok(Some(Ok(OuterFrame::Discard))),
                        OuterState::DiscardPayload,
                    ),
                    Some(header) => {
                        let part_type = *header.part_type();
                        let part_id = header.part_id();
                        (
                            Ok(Some(Ok(OuterFrame::Header(header)))),
                            OuterState::Payload { part_type, part_id },
                        )
                    }
                }
            }

            cur_state @ OuterState::Payload { .. } | cur_state @ OuterState::DiscardPayload => {
                let (payload, next_state) = Self::decode_payload(buf, cur_state);
                (Ok(payload.transpose()), next_state)
            }

            OuterState::StreamEnd => (Ok(Some(Ok(OuterFrame::StreamEnd))), OuterState::StreamEnd),

            OuterState::Invalid => (
                Err(ErrorKind::Bundle2Decode("byte stream corrupt".into()).into()),
                OuterState::Invalid,
            ),
        }
    }

    fn decode_header(logger: &Logger, header_bytes: Bytes) -> Result<Option<PartHeader>> {
        let header = part_header::decode(header_bytes)?;
        debug!(logger, "Decoded header: {:?}", header);
        match validate_header(header)? {
            Some(header) => Ok(Some(header)),
            None => {
                // The part couldn't be recognized but wasn't important anyway.
                // Throw it away (the state machine will throw away any associated
                // chunks it finds).
                Ok(None)
            }
        }
    }

    fn decode_payload(
        buf: &mut BytesMut,
        state: OuterState,
    ) -> (Result<Option<OuterFrame>>, OuterState) {
        if buf.len() < 4 {
            return (Ok(None), state);
        }

        // Payloads are in the format:
        // ---
        // total_len: i32
        // payload: Vec<u8>, total_len bytes
        // ---
        // A payload is guaranteed to be < 2**31 bytes, so buffer up
        // until the whole payload is available.
        //
        // TODO: -1 means this part has been interrupted. Handle that
        // case.

        let total_len = BigEndian::read_u32(&buf[..4]);
        if total_len == 0 {
            let _ = buf.get_i32();
            // A zero-size chunk indicates that this part has
            // ended. More parts might be coming up, so go back to the
            // header state.
            (Ok(Some(state.part_end_frame())), OuterState::Header)
        } else {
            let payload = Self::decode_payload_chunk(buf, &state, total_len as usize);
            (Ok(payload), state)
        }
    }

    fn decode_payload_chunk(
        buf: &mut BytesMut,
        state: &OuterState,
        total_len: usize,
    ) -> Option<OuterFrame> {
        // + 4 bytes for the header
        if buf.len() < total_len + 4 {
            return None;
        }

        let _ = buf.get_i32();
        let chunk = buf.split_to(total_len);

        Some(state.payload_frame(chunk))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum OuterFrame {
    Header(PartHeader),
    Payload {
        part_type: PartHeaderType,
        part_id: PartId,
        payload: Bytes,
    },
    PartEnd {
        part_type: PartHeaderType,
        part_id: PartId,
    },
    Discard,
    StreamEnd,
}

impl OuterFrame {
    pub fn is_payload(&self) -> bool {
        match self {
            &OuterFrame::Payload { .. } => true,
            _ => false,
        }
    }

    pub fn get_payload(self) -> Bytes {
        match self {
            OuterFrame::Payload { payload, .. } => payload,
            _ => panic!("get_payload called on an OuterFrame without a payload!"),
        }
    }
}
