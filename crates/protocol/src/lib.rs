pub const CRATE_NAME: &str = "stream-sync-protocol";

/// Client identifier allowed by the server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientId(pub String);

/// Identifier for one application/session run.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId(pub String);

/// Version of the StreamSync wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolVersion(pub u32);

/// Version of an application binary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppVersion(pub String);

/// Timestamp value expressed in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMicros(pub u64);

/// Byte length of the fixed packet header used by the initial wire format.
pub const FIXED_HEADER_LEN: u16 = 16;

/// Byte offset of `message_type` in the fixed packet header.
pub const HEADER_MESSAGE_TYPE_OFFSET: usize = 0;

/// Byte offset of `header_length` in the fixed packet header.
pub const HEADER_LENGTH_OFFSET: usize = 2;

/// Byte offset of `protocol_version` in the fixed packet header.
pub const HEADER_PROTOCOL_VERSION_OFFSET: usize = 4;

/// Byte offset of `payload_length` in the fixed packet header.
pub const HEADER_PAYLOAD_LENGTH_OFFSET: usize = 8;

/// Byte offset of `flags` in the fixed packet header.
pub const HEADER_FLAGS_OFFSET: usize = 12;

/// Byte offset of the reserved field in the fixed packet header.
pub const HEADER_RESERVED_OFFSET: usize = 14;

/// Byte length of a variable string length prefix in message payloads.
pub const PAYLOAD_STRING_LEN_PREFIX_LEN: u16 = 2;

/// Byte length of an optional field presence tag in message payloads.
pub const PAYLOAD_OPTION_TAG_LEN: u16 = 1;

/// Byte length of a variable byte array length prefix in message payloads.
pub const PAYLOAD_BYTES_LEN_PREFIX_LEN: u16 = 4;

/// Byte length of a bool field in message payloads.
pub const PAYLOAD_BOOL_LEN: u16 = 1;

/// Byte length of an AuthResponse reason code in message payloads.
pub const AUTH_RESPONSE_REASON_CODE_LEN: u16 = 2;

/// Wire value for H.264 encoded video payloads.
pub const CODEC_H264_WIRE_VALUE: u16 = 1;

/// Byte length of the fixed numeric part of the initial VideoFrame payload.
///
/// This excludes length-prefixed `client_id`, length-prefixed `run_id`, and
/// the variable H.264 payload bytes.
pub const VIDEO_FRAME_NUMERIC_METADATA_LEN: u16 = 46;

/// Decoded fixed packet header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedHeader {
    pub message_type: MessageType,
    pub header_length: u16,
    pub protocol_version: ProtocolVersion,
    pub payload_length: u32,
    pub flags: u16,
    pub reserved: u16,
}

/// Borrowed view produced after fixed header decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketView<'a> {
    pub header: FixedHeader,
    pub payload: &'a [u8],
}

/// Minimal fixed header decoder for the initial wire format.
///
/// This decoder parses only the 16 byte fixed packet header and returns a raw
/// payload slice. It does not check protocol compatibility or interpret the
/// payload body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FixedHeaderCodec;

impl FixedHeaderCodec {
    pub fn decode_fixed_header<'a>(
        &self,
        packet: &'a [u8],
    ) -> Result<PacketView<'a>, ProtocolError> {
        decode_fixed_header(packet)
    }
}

/// Decode the 16 byte fixed packet header and split out the raw payload slice.
pub fn decode_fixed_header(packet: &[u8]) -> Result<PacketView<'_>, ProtocolError> {
    let fixed_header_len = usize::from(FIXED_HEADER_LEN);
    if packet.len() < fixed_header_len {
        return Err(ProtocolError::BufferTooShort);
    }

    let message_type = MessageType::try_from(read_u16_le(packet, HEADER_MESSAGE_TYPE_OFFSET))?;
    let header_length = read_u16_le(packet, HEADER_LENGTH_OFFSET);
    if header_length != FIXED_HEADER_LEN {
        return Err(ProtocolError::InvalidHeaderLength {
            actual: header_length,
        });
    }

    let protocol_version = ProtocolVersion(read_u32_le(packet, HEADER_PROTOCOL_VERSION_OFFSET));
    let payload_length = read_u32_le(packet, HEADER_PAYLOAD_LENGTH_OFFSET);
    let flags = read_u16_le(packet, HEADER_FLAGS_OFFSET);
    let reserved = read_u16_le(packet, HEADER_RESERVED_OFFSET);

    let payload_len = payload_length as usize;
    let expected_packet_len =
        fixed_header_len
            .checked_add(payload_len)
            .ok_or(ProtocolError::InvalidPayloadLength {
                expected: payload_length,
                actual: packet.len().saturating_sub(fixed_header_len),
            })?;

    if packet.len() != expected_packet_len {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: payload_length,
            actual: packet.len().saturating_sub(fixed_header_len),
        });
    }

    Ok(PacketView {
        header: FixedHeader {
            message_type,
            header_length,
            protocol_version,
            payload_length,
            flags,
            reserved,
        },
        payload: &packet[fixed_header_len..],
    })
}

fn read_u16_le(packet: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([packet[offset], packet[offset + 1]])
}

fn read_u32_le(packet: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        packet[offset],
        packet[offset + 1],
        packet[offset + 2],
        packet[offset + 3],
    ])
}

fn read_u64_le(packet: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        packet[offset],
        packet[offset + 1],
        packet[offset + 2],
        packet[offset + 3],
        packet[offset + 4],
        packet[offset + 5],
        packet[offset + 6],
        packet[offset + 7],
    ])
}

fn read_u8(packet: &[u8], offset: usize) -> u8 {
    packet[offset]
}

/// Encode the initial 16 byte fixed header into `output`.
///
/// This writes only the protocol envelope. It does not encode payload fields,
/// choose a destination, queue packets, or send through UDP sockets.
pub fn encode_fixed_header(
    message_type: MessageType,
    protocol_version: ProtocolVersion,
    payload_length: u32,
    output: &mut Vec<u8>,
) {
    output.extend_from_slice(&(message_type as u16).to_le_bytes());
    output.extend_from_slice(&FIXED_HEADER_LEN.to_le_bytes());
    output.extend_from_slice(&protocol_version.0.to_le_bytes());
    output.extend_from_slice(&payload_length.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
}

/// Context passed to future decode entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecodeContext {
    pub expected_protocol_version: ProtocolVersion,
}

impl DecodeContext {
    /// Validate fixed header compatibility before payload decoding.
    pub fn validate_protocol_version(&self, header: FixedHeader) -> Result<(), ProtocolError> {
        validate_protocol_version(*self, header)
    }
}

/// Check that a decoded fixed header matches the protocol version expected by the app.
///
/// The caller supplies the expected version through `DecodeContext`. This helper
/// only compares the fixed header value and does not inspect or decode payload
/// bytes.
pub fn validate_protocol_version(
    context: DecodeContext,
    header: FixedHeader,
) -> Result<(), ProtocolError> {
    if header.protocol_version == context.expected_protocol_version {
        Ok(())
    } else {
        Err(ProtocolError::UnsupportedProtocolVersion {
            expected: context.expected_protocol_version,
            actual: header.protocol_version,
        })
    }
}

/// Decode an `AuthRequest` payload after fixed header and protocol version checks.
///
/// This parses only the payload layout documented for the initial MVP wire
/// format. It does not authenticate the client or decide whether the request is
/// accepted.
pub fn decode_auth_request_payload(
    header: FixedHeader,
    payload: &[u8],
) -> Result<AuthRequest, ProtocolError> {
    if header.message_type != MessageType::AuthRequest {
        return Err(ProtocolError::UnexpectedMessageType {
            expected: MessageType::AuthRequest,
            actual: header.message_type,
        });
    }

    if payload.len() != header.payload_length as usize {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: header.payload_length,
            actual: payload.len(),
        });
    }

    let mut reader = PayloadReader::new(payload);
    let client_id = ClientId(reader.read_string()?);
    let run_id = RunId(reader.read_string()?);
    let app_version = AppVersion(reader.read_string()?);
    let shared_token = reader.read_string()?;
    let display_name = reader.read_optional_string()?;
    reader.finish()?;

    Ok(AuthRequest {
        message_type: MessageType::AuthRequest,
        protocol_version: header.protocol_version,
        client_id,
        run_id,
        app_version,
        shared_token,
        display_name,
        capabilities: Vec::new(),
        requested_video_profile: None,
    })
}

/// Decode a `Heartbeat` payload after fixed header and protocol version checks.
///
/// This parses only the payload layout documented for the initial MVP wire
/// format. It does not update liveness state, compute RTT, or decide whether a
/// client has timed out.
pub fn decode_heartbeat_payload(
    header: FixedHeader,
    payload: &[u8],
) -> Result<Heartbeat, ProtocolError> {
    if header.message_type != MessageType::Heartbeat {
        return Err(ProtocolError::UnexpectedMessageType {
            expected: MessageType::Heartbeat,
            actual: header.message_type,
        });
    }

    if payload.len() != header.payload_length as usize {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: header.payload_length,
            actual: payload.len(),
        });
    }

    let mut reader = PayloadReader::new(payload);
    let client_id = ClientId(reader.read_string()?);
    let run_id = RunId(reader.read_string()?);
    let sent_at = TimestampMicros(reader.read_u64()?);
    let local_time = reader.read_optional_u64()?.map(TimestampMicros);
    let short_status = reader.read_optional_string()?;
    reader.finish()?;

    Ok(Heartbeat {
        message_type: MessageType::Heartbeat,
        protocol_version: header.protocol_version,
        client_id,
        run_id,
        sent_at,
        local_time,
        short_status,
    })
}

/// Decode a `VideoFrame` payload after fixed header and protocol version checks.
///
/// This parses the frame metadata and copies the H.264 payload bytes as-is. It
/// does not inspect, decode, fragment, or repair the encoded video data.
pub fn decode_video_frame_payload(
    header: FixedHeader,
    payload: &[u8],
) -> Result<VideoFrame, ProtocolError> {
    if header.message_type != MessageType::VideoFrame {
        return Err(ProtocolError::UnexpectedMessageType {
            expected: MessageType::VideoFrame,
            actual: header.message_type,
        });
    }

    if payload.len() != header.payload_length as usize {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: header.payload_length,
            actual: payload.len(),
        });
    }

    let mut reader = PayloadReader::new(payload);
    let client_id = ClientId(reader.read_string()?);
    let run_id = RunId(reader.read_string()?);
    let frame_id = reader.read_u64()?;
    let capture_timestamp = TimestampMicros(reader.read_u64()?);
    let send_timestamp = TimestampMicros(reader.read_u64()?);
    let is_keyframe = reader.read_bool()?;
    let metadata_reserved = reader.read_array_3()?;
    if metadata_reserved != [0; 3] {
        return Err(ProtocolError::InvalidMetadataReserved {
            actual: metadata_reserved,
        });
    }
    let width = reader.read_u32()?;
    let height = reader.read_u32()?;
    let fps_nominal = reader.read_u32()?;
    let codec = Codec::try_from(reader.read_u16()?)?;
    let payload_size = reader.read_u32()? as usize;
    let h264_payload = reader.read_exact_bytes(payload_size)?;
    reader.finish()?;

    Ok(VideoFrame {
        message_type: MessageType::VideoFrame,
        protocol_version: header.protocol_version,
        client_id,
        run_id,
        frame_id,
        capture_timestamp,
        send_timestamp,
        is_keyframe,
        metadata_reserved,
        width,
        height,
        fps_nominal,
        codec,
        payload_size,
        payload: h264_payload,
    })
}

/// Decode a payload by dispatching on the fixed header `message_type`.
///
/// The caller is expected to run fixed header decode and protocol version
/// validation first. This helper only chooses the message-specific payload
/// decoder and does not perform socket I/O or app handler work.
pub fn decode_payload_by_message_type(
    context: DecodeContext,
    header: FixedHeader,
    payload: &[u8],
) -> Result<ProtocolMessage, ProtocolError> {
    match header.message_type {
        MessageType::AuthRequest => {
            AuthRequestPayloadDecoder.decode_payload(context, header, payload)
        }
        MessageType::Heartbeat => HeartbeatPayloadDecoder.decode_payload(context, header, payload),
        MessageType::VideoFrame => {
            VideoFramePayloadDecoder.decode_payload(context, header, payload)
        }
        message_type => Err(ProtocolError::PayloadDecodeNotImplemented(message_type)),
    }
}

struct PayloadReader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> PayloadReader<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn read_string(&mut self) -> Result<String, ProtocolError> {
        let len_prefix_len = usize::from(PAYLOAD_STRING_LEN_PREFIX_LEN);
        self.ensure_available(len_prefix_len)?;
        let byte_len = read_u16_le(self.payload, self.offset) as usize;
        self.offset += len_prefix_len;

        self.ensure_available(byte_len)?;
        let bytes = &self.payload[self.offset..self.offset + byte_len];
        self.offset += byte_len;

        String::from_utf8(bytes.to_vec()).map_err(|_| ProtocolError::InvalidUtf8String)
    }

    fn read_u16(&mut self) -> Result<u16, ProtocolError> {
        let byte_len = 2;
        self.ensure_available(byte_len)?;
        let value = read_u16_le(self.payload, self.offset);
        self.offset += byte_len;
        Ok(value)
    }

    fn read_u32(&mut self) -> Result<u32, ProtocolError> {
        let byte_len = 4;
        self.ensure_available(byte_len)?;
        let value = read_u32_le(self.payload, self.offset);
        self.offset += byte_len;
        Ok(value)
    }

    fn read_u64(&mut self) -> Result<u64, ProtocolError> {
        let byte_len = 8;
        self.ensure_available(byte_len)?;
        let value = read_u64_le(self.payload, self.offset);
        self.offset += byte_len;
        Ok(value)
    }

    fn read_bool(&mut self) -> Result<bool, ProtocolError> {
        self.ensure_available(1)?;
        let value = read_u8(self.payload, self.offset);
        self.offset += 1;

        match value {
            0 => Ok(false),
            1 => Ok(true),
            actual => Err(ProtocolError::InvalidBoolValue { actual }),
        }
    }

    fn read_array_3(&mut self) -> Result<[u8; 3], ProtocolError> {
        let byte_len = 3;
        self.ensure_available(byte_len)?;
        let value = [
            self.payload[self.offset],
            self.payload[self.offset + 1],
            self.payload[self.offset + 2],
        ];
        self.offset += byte_len;
        Ok(value)
    }

    fn read_exact_bytes(&mut self, len: usize) -> Result<Vec<u8>, ProtocolError> {
        self.ensure_available(len)?;
        let bytes = self.payload[self.offset..self.offset + len].to_vec();
        self.offset += len;
        Ok(bytes)
    }

    fn read_optional_string(&mut self) -> Result<Option<String>, ProtocolError> {
        self.ensure_available(usize::from(PAYLOAD_OPTION_TAG_LEN))?;
        let present = read_u8(self.payload, self.offset);
        self.offset += usize::from(PAYLOAD_OPTION_TAG_LEN);

        match present {
            0 => Ok(None),
            1 => self.read_string().map(Some),
            actual => Err(ProtocolError::InvalidOptionalTag { actual }),
        }
    }

    fn read_optional_u64(&mut self) -> Result<Option<u64>, ProtocolError> {
        self.ensure_available(usize::from(PAYLOAD_OPTION_TAG_LEN))?;
        let present = read_u8(self.payload, self.offset);
        self.offset += usize::from(PAYLOAD_OPTION_TAG_LEN);

        match present {
            0 => Ok(None),
            1 => self.read_u64().map(Some),
            actual => Err(ProtocolError::InvalidOptionalTag { actual }),
        }
    }

    fn finish(&self) -> Result<(), ProtocolError> {
        if self.offset == self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayloadLength {
                expected: self.offset as u32,
                actual: self.payload.len(),
            })
        }
    }

    fn ensure_available(&self, len: usize) -> Result<(), ProtocolError> {
        let expected = self
            .offset
            .checked_add(len)
            .ok_or(ProtocolError::InvalidPayloadLength {
                expected: u32::MAX,
                actual: self.payload.len(),
            })?;

        if expected <= self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayloadLength {
                expected: expected as u32,
                actual: self.payload.len(),
            })
        }
    }
}

fn write_string(output: &mut Vec<u8>, value: &str) -> Result<(), ProtocolError> {
    let byte_len = value.len();
    let byte_len = u16::try_from(byte_len)
        .map_err(|_| ProtocolError::PayloadStringTooLong { actual: byte_len })?;
    output.extend_from_slice(&byte_len.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

fn write_optional_string(output: &mut Vec<u8>, value: Option<&str>) -> Result<(), ProtocolError> {
    match value {
        Some(value) => {
            output.push(1);
            write_string(output, value)
        }
        None => {
            output.push(0);
            Ok(())
        }
    }
}

fn write_optional_u64(output: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            output.push(1);
            output.extend_from_slice(&value.to_le_bytes());
        }
        None => output.push(0),
    }
}

fn write_optional_protocol_version(output: &mut Vec<u8>, value: Option<ProtocolVersion>) {
    match value {
        Some(value) => {
            output.push(1);
            output.extend_from_slice(&value.0.to_le_bytes());
        }
        None => output.push(0),
    }
}

/// Context passed to future encode entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncodeContext {
    pub protocol_version: ProtocolVersion,
}

/// Encode one `AuthRequest` packet as fixed header plus payload bytes.
///
/// The payload follows the documented order: `client_id`, `run_id`,
/// `app_version`, `shared_token`, and `display_name`. This function does not
/// resolve destinations, open sockets, retry, or manage authentication state.
pub fn encode_auth_request(
    context: EncodeContext,
    request: &AuthRequest,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let mut payload = Vec::new();
    encode_auth_request_payload(request, &mut payload)?;
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| ProtocolError::InvalidPayloadLength {
            expected: u32::MAX,
            actual: payload.len(),
        })?;

    encode_fixed_header(
        MessageType::AuthRequest,
        context.protocol_version,
        payload_length,
        output,
    );
    output.extend_from_slice(&payload);
    Ok(())
}

/// Encode one `AuthResponse` packet as fixed header plus payload bytes.
///
/// The payload follows the documented order: `client_id`, `run_id`, `accepted`,
/// `reason_code`, `message`, `server_time`, and
/// `expected_protocol_version`. This function does not send the bytes or manage
/// destinations.
pub fn encode_auth_response(
    context: EncodeContext,
    response: &AuthResponse,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let mut payload = Vec::new();
    encode_auth_response_payload(response, &mut payload)?;
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| ProtocolError::InvalidPayloadLength {
            expected: u32::MAX,
            actual: payload.len(),
        })?;

    encode_fixed_header(
        MessageType::AuthResponse,
        context.protocol_version,
        payload_length,
        output,
    );
    output.extend_from_slice(&payload);
    Ok(())
}

/// Encode one `HeartbeatAck` packet as fixed header plus payload bytes.
///
/// The payload follows the documented order: `client_id`, `run_id`,
/// `echoed_sent_at`, `server_received_at`, and `server_sent_at`. This function
/// does not send the bytes, manage heartbeat state, or calculate RTT / offset.
pub fn encode_heartbeat_ack(
    context: EncodeContext,
    ack: &HeartbeatAck,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let mut payload = Vec::new();
    encode_heartbeat_ack_payload(ack, &mut payload)?;
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| ProtocolError::InvalidPayloadLength {
            expected: u32::MAX,
            actual: payload.len(),
        })?;

    encode_fixed_header(
        MessageType::HeartbeatAck,
        context.protocol_version,
        payload_length,
        output,
    );
    output.extend_from_slice(&payload);
    Ok(())
}

/// Encode one `VideoFrame` packet as fixed header plus payload bytes.
///
/// The payload follows the documented order: frame metadata first, then the
/// raw H.264 bytes. This function does not compress video, split frames, retry,
/// or send the bytes.
pub fn encode_video_frame(
    context: EncodeContext,
    frame: &VideoFrame,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    let mut payload = Vec::new();
    encode_video_frame_payload(frame, &mut payload)?;
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| ProtocolError::InvalidPayloadLength {
            expected: u32::MAX,
            actual: payload.len(),
        })?;

    encode_fixed_header(
        MessageType::VideoFrame,
        context.protocol_version,
        payload_length,
        output,
    );
    output.extend_from_slice(&payload);
    Ok(())
}

/// Encode only the `AuthRequest` payload body.
pub fn encode_auth_request_payload(
    request: &AuthRequest,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    write_string(output, &request.client_id.0)?;
    write_string(output, &request.run_id.0)?;
    write_string(output, &request.app_version.0)?;
    write_string(output, &request.shared_token)?;
    write_optional_string(output, request.display_name.as_deref())?;
    Ok(())
}

/// Encode only the `AuthResponse` payload body.
pub fn encode_auth_response_payload(
    response: &AuthResponse,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    write_string(output, &response.client_id.0)?;
    write_string(output, &response.run_id.0)?;
    write_bool(output, response.accepted);
    output.extend_from_slice(&response.reason_code.wire_code().to_le_bytes());
    write_optional_string(output, response.message.as_deref())?;
    write_optional_u64(output, response.server_time.map(|timestamp| timestamp.0));
    write_optional_protocol_version(output, response.expected_protocol_version);
    Ok(())
}

/// Encode only the `HeartbeatAck` payload body.
pub fn encode_heartbeat_ack_payload(
    ack: &HeartbeatAck,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    write_string(output, &ack.client_id.0)?;
    write_string(output, &ack.run_id.0)?;
    output.extend_from_slice(&ack.echoed_sent_at.0.to_le_bytes());
    output.extend_from_slice(&ack.server_received_at.0.to_le_bytes());
    output.extend_from_slice(&ack.server_sent_at.0.to_le_bytes());
    Ok(())
}

/// Encode only the `VideoFrame` payload body.
pub fn encode_video_frame_payload(
    frame: &VideoFrame,
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    if frame.metadata_reserved != [0; 3] {
        return Err(ProtocolError::InvalidMetadataReserved {
            actual: frame.metadata_reserved,
        });
    }

    let payload_size =
        u32::try_from(frame.payload.len()).map_err(|_| ProtocolError::InvalidPayloadLength {
            expected: u32::MAX,
            actual: frame.payload.len(),
        })?;
    if frame.payload_size != frame.payload.len() {
        return Err(ProtocolError::InvalidPayloadLength {
            expected: payload_size,
            actual: frame.payload_size,
        });
    }

    write_string(output, &frame.client_id.0)?;
    write_string(output, &frame.run_id.0)?;
    output.extend_from_slice(&frame.frame_id.to_le_bytes());
    output.extend_from_slice(&frame.capture_timestamp.0.to_le_bytes());
    output.extend_from_slice(&frame.send_timestamp.0.to_le_bytes());
    write_bool(output, frame.is_keyframe);
    output.extend_from_slice(&frame.metadata_reserved);
    output.extend_from_slice(&frame.width.to_le_bytes());
    output.extend_from_slice(&frame.height.to_le_bytes());
    output.extend_from_slice(&frame.fps_nominal.to_le_bytes());
    output.extend_from_slice(&frame.codec.wire_code().to_le_bytes());
    output.extend_from_slice(&payload_size.to_le_bytes());
    output.extend_from_slice(&frame.payload);
    Ok(())
}

/// Decoded protocol message variants.
///
/// This enum defines the message dispatch boundary. Constructing these variants
/// from bytes is intentionally left for a later implementation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolMessage {
    AuthRequest(AuthRequest),
    AuthResponse(AuthResponse),
    Heartbeat(Heartbeat),
    HeartbeatAck(HeartbeatAck),
    VideoFrame(VideoFrame),
    ClientStats(ClientStats),
    ServerNotice(ServerNotice),
}

impl ProtocolMessage {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::AuthRequest(_) => MessageType::AuthRequest,
            Self::AuthResponse(_) => MessageType::AuthResponse,
            Self::Heartbeat(_) => MessageType::Heartbeat,
            Self::HeartbeatAck(_) => MessageType::HeartbeatAck,
            Self::VideoFrame(_) => MessageType::VideoFrame,
            Self::ClientStats(_) => MessageType::ClientStats,
            Self::ServerNotice(_) => MessageType::ServerNotice,
        }
    }
}

/// Errors that future wire encode/decode entry points may return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    BufferTooShort,
    InvalidHeaderLength {
        actual: u16,
    },
    InvalidPayloadLength {
        expected: u32,
        actual: usize,
    },
    InvalidOptionalTag {
        actual: u8,
    },
    InvalidBoolValue {
        actual: u8,
    },
    InvalidMetadataReserved {
        actual: [u8; 3],
    },
    InvalidUtf8String,
    UnsupportedCodec {
        actual: u16,
    },
    UnknownMessageType(u16),
    UnexpectedMessageType {
        expected: MessageType,
        actual: MessageType,
    },
    UnsupportedProtocolVersion {
        expected: ProtocolVersion,
        actual: ProtocolVersion,
    },
    PayloadStringTooLong {
        actual: usize,
    },
    PayloadDecodeNotImplemented(MessageType),
    EncodeNotImplemented(MessageType),
}

/// Future entry point for fixed header decoding.
///
/// Implementors should only parse the fixed packet header and split out the
/// payload slice. They should not perform socket I/O or app-level handling.
pub trait FixedHeaderDecoder {
    fn decode_fixed_header<'a>(&self, packet: &'a [u8]) -> Result<PacketView<'a>, ProtocolError>;
}

impl FixedHeaderDecoder for FixedHeaderCodec {
    fn decode_fixed_header<'a>(&self, packet: &'a [u8]) -> Result<PacketView<'a>, ProtocolError> {
        decode_fixed_header(packet)
    }
}

/// Future entry point for payload decoding after message type dispatch.
pub trait PayloadDecoder {
    fn decode_payload(
        &self,
        context: DecodeContext,
        header: FixedHeader,
        payload: &[u8],
    ) -> Result<ProtocolMessage, ProtocolError>;
}

/// Minimal payload decoder for `AuthRequest`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AuthRequestPayloadDecoder;

impl PayloadDecoder for AuthRequestPayloadDecoder {
    fn decode_payload(
        &self,
        _context: DecodeContext,
        header: FixedHeader,
        payload: &[u8],
    ) -> Result<ProtocolMessage, ProtocolError> {
        decode_auth_request_payload(header, payload).map(ProtocolMessage::AuthRequest)
    }
}

/// Minimal payload decoder for `Heartbeat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatPayloadDecoder;

impl PayloadDecoder for HeartbeatPayloadDecoder {
    fn decode_payload(
        &self,
        _context: DecodeContext,
        header: FixedHeader,
        payload: &[u8],
    ) -> Result<ProtocolMessage, ProtocolError> {
        decode_heartbeat_payload(header, payload).map(ProtocolMessage::Heartbeat)
    }
}

/// Minimal payload decoder for `VideoFrame`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct VideoFramePayloadDecoder;

impl PayloadDecoder for VideoFramePayloadDecoder {
    fn decode_payload(
        &self,
        _context: DecodeContext,
        header: FixedHeader,
        payload: &[u8],
    ) -> Result<ProtocolMessage, ProtocolError> {
        decode_video_frame_payload(header, payload).map(ProtocolMessage::VideoFrame)
    }
}

/// Future entry point for message encoding into one packet buffer.
pub trait MessageEncoder {
    fn encode_message(
        &self,
        context: EncodeContext,
        message: &ProtocolMessage,
        output: &mut Vec<u8>,
    ) -> Result<(), ProtocolError>;
}

/// Protocol encoder boundary for currently supported outbound messages.
///
/// This writes one complete packet buffer for supported outbound messages.
/// Other messages remain unsupported until their payload layouts are
/// implemented.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ProtocolMessageEncoderBoundary;

impl MessageEncoder for ProtocolMessageEncoderBoundary {
    fn encode_message(
        &self,
        context: EncodeContext,
        message: &ProtocolMessage,
        output: &mut Vec<u8>,
    ) -> Result<(), ProtocolError> {
        match message {
            ProtocolMessage::AuthRequest(request) => encode_auth_request(context, request, output),
            ProtocolMessage::AuthResponse(response) => {
                encode_auth_response(context, response, output)
            }
            ProtocolMessage::HeartbeatAck(ack) => encode_heartbeat_ack(context, ack, output),
            ProtocolMessage::VideoFrame(frame) => encode_video_frame(context, frame, output),
            message => Err(ProtocolError::EncodeNotImplemented(message.message_type())),
        }
    }
}

/// Message kinds used by the MVP protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageType {
    AuthRequest = 1,
    AuthResponse = 2,
    Heartbeat = 3,
    HeartbeatAck = 4,
    VideoFrame = 5,
    ClientStats = 6,
    ServerNotice = 7,
}

impl TryFrom<u16> for MessageType {
    type Error = ProtocolError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::AuthRequest),
            2 => Ok(Self::AuthResponse),
            3 => Ok(Self::Heartbeat),
            4 => Ok(Self::HeartbeatAck),
            5 => Ok(Self::VideoFrame),
            6 => Ok(Self::ClientStats),
            7 => Ok(Self::ServerNotice),
            value => Err(ProtocolError::UnknownMessageType(value)),
        }
    }
}

/// Initial authentication request sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub app_version: AppVersion,
    pub shared_token: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<String>,
    pub requested_video_profile: Option<String>,
}

/// Authentication result returned by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResponse {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub accepted: bool,
    pub reason_code: AuthResponseReasonCode,
    pub message: Option<String>,
    pub server_time: Option<TimestampMicros>,
    pub expected_protocol_version: Option<ProtocolVersion>,
}

/// Reason code for an authentication response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum AuthResponseReasonCode {
    Ok = 0,
    InvalidToken = 1,
    UnknownClient = 2,
    ProtocolMismatch = 3,
    AlreadyConnected = 4,
    InternalError = 5,
}

impl AuthResponseReasonCode {
    pub const fn wire_code(self) -> u16 {
        self as u16
    }
}

/// Periodic liveness message sent by an authenticated client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub sent_at: TimestampMicros,
    pub local_time: Option<TimestampMicros>,
    pub short_status: Option<String>,
}

/// Server response to a heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
}

/// Client-side observation created after receiving one `HeartbeatAck`.
///
/// This is a typed flow object for future RTT / offset reporting. It is not
/// encoded as a wire payload by the current protocol encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatAckObservation {
    pub client_id: ClientId,
    pub run_id: RunId,
    pub echoed_sent_at: TimestampMicros,
    pub server_received_at: TimestampMicros,
    pub server_sent_at: TimestampMicros,
    pub client_received_at: TimestampMicros,
}

/// Boundary that turns a received `HeartbeatAck` plus client receive time into
/// an observation for a future client-to-server report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatAckObservationBoundary;

impl HeartbeatAckObservationBoundary {
    pub fn observe(
        &self,
        ack: &HeartbeatAck,
        client_received_at: TimestampMicros,
    ) -> HeartbeatAckObservation {
        HeartbeatAckObservation {
            client_id: ack.client_id.clone(),
            run_id: ack.run_id.clone(),
            echoed_sent_at: ack.echoed_sent_at,
            server_received_at: ack.server_received_at,
            server_sent_at: ack.server_sent_at,
            client_received_at,
        }
    }
}

/// Typed carrier for returning one heartbeat ack observation to the server.
///
/// The selected future wire carrier is `ClientStats`. This type fixes the
/// message flow without implementing `ClientStats` payload encode/decode yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatObservationCarrier {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub observation: HeartbeatAckObservation,
}

/// Boundary that wraps an observation in the future `ClientStats` carrier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct HeartbeatObservationCarrierBoundary;

impl HeartbeatObservationCarrierBoundary {
    pub fn build_client_stats_carrier(
        &self,
        protocol_version: ProtocolVersion,
        observation: HeartbeatAckObservation,
    ) -> HeartbeatObservationCarrier {
        HeartbeatObservationCarrier {
            message_type: MessageType::ClientStats,
            protocol_version,
            observation,
        }
    }
}

/// Encoded video frame sent from a client to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub frame_id: u64,
    pub capture_timestamp: TimestampMicros,
    pub send_timestamp: TimestampMicros,
    pub is_keyframe: bool,
    pub metadata_reserved: [u8; 3],
    pub width: u32,
    pub height: u32,
    pub fps_nominal: u32,
    pub codec: Codec,
    pub payload_size: usize,
    pub payload: Vec<u8>,
}

/// Video codec identifier for encoded frame payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
}

impl Codec {
    pub const fn wire_code(self) -> u16 {
        match self {
            Self::H264 => CODEC_H264_WIRE_VALUE,
        }
    }
}

impl TryFrom<u16> for Codec {
    type Error = ProtocolError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            CODEC_H264_WIRE_VALUE => Ok(Self::H264),
            actual => Err(ProtocolError::UnsupportedCodec { actual }),
        }
    }
}

/// Periodic client-side metrics used for monitoring and troubleshooting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientStats {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub client_id: ClientId,
    pub run_id: RunId,
    pub sent_at: TimestampMicros,
    pub capture_fps: u32,
    pub dropped_frames: u64,
    pub bitrate_kbps: u32,
}

/// Server-side notice sent to report warnings, disconnects, or protocol issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNotice {
    pub message_type: MessageType,
    pub protocol_version: ProtocolVersion,
    pub run_id: RunId,
    pub notice_type: NoticeType,
    pub message: String,
}

/// Type of server notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeType {
    Warning,
    Disconnect,
    ProtocolError,
    AuthExpired,
    ServerShutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_fixed_header_and_payload_slice() {
        let payload = [0xaa, 0xbb, 0xcc];
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );

        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        assert_eq!(decoded.header.message_type, MessageType::VideoFrame);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.payload_length, payload.len() as u32);
        assert_eq!(decoded.header.flags, 0x0034);
        assert_eq!(decoded.header.reserved, 0);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn rejects_short_header() {
        let packet = [0; 15];

        let decoded = decode_fixed_header(&packet);

        assert_eq!(decoded, Err(ProtocolError::BufferTooShort));
    }

    #[test]
    fn rejects_unknown_message_type() {
        let packet = test_packet(999, FIXED_HEADER_LEN, 1, &[]);

        let decoded = decode_fixed_header(&packet);

        assert_eq!(decoded, Err(ProtocolError::UnknownMessageType(999)));
    }

    #[test]
    fn rejects_invalid_header_length() {
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN + 1,
            1,
            &[],
        );

        let decoded = decode_fixed_header(&packet);

        assert_eq!(
            decoded,
            Err(ProtocolError::InvalidHeaderLength {
                actual: FIXED_HEADER_LEN + 1
            })
        );
    }

    #[test]
    fn rejects_invalid_payload_length() {
        let mut packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &[0xaa]);
        packet[HEADER_PAYLOAD_LENGTH_OFFSET..HEADER_PAYLOAD_LENGTH_OFFSET + 4]
            .copy_from_slice(&2_u32.to_le_bytes());

        let decoded = decode_fixed_header(&packet);

        assert_eq!(
            decoded,
            Err(ProtocolError::InvalidPayloadLength {
                expected: 2,
                actual: 1
            })
        );
    }

    #[test]
    fn accepts_matching_protocol_version() {
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &[]);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");
        let context = DecodeContext {
            expected_protocol_version: ProtocolVersion(2),
        };

        assert_eq!(validate_protocol_version(context, decoded.header), Ok(()));
        assert_eq!(context.validate_protocol_version(decoded.header), Ok(()));
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 1, &[]);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");
        let context = DecodeContext {
            expected_protocol_version: ProtocolVersion(2),
        };

        assert_eq!(
            validate_protocol_version(context, decoded.header),
            Err(ProtocolError::UnsupportedProtocolVersion {
                expected: ProtocolVersion(2),
                actual: ProtocolVersion(1)
            })
        );
    }

    #[test]
    fn auth_response_reason_code_wire_values_are_stable() {
        assert_eq!(AUTH_RESPONSE_REASON_CODE_LEN, 2);
        assert_eq!(AuthResponseReasonCode::Ok.wire_code(), 0);
        assert_eq!(AuthResponseReasonCode::InvalidToken.wire_code(), 1);
        assert_eq!(AuthResponseReasonCode::UnknownClient.wire_code(), 2);
        assert_eq!(AuthResponseReasonCode::ProtocolMismatch.wire_code(), 3);
        assert_eq!(AuthResponseReasonCode::AlreadyConnected.wire_code(), 4);
        assert_eq!(AuthResponseReasonCode::InternalError.wire_code(), 5);
    }

    #[test]
    fn encodes_auth_request_payload_with_display_name() {
        let request = AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: "shared-secret".to_string(),
            display_name: Some("Alice".to_string()),
            capabilities: Vec::new(),
            requested_video_profile: None,
        };
        let mut payload = Vec::new();

        encode_auth_request_payload(&request, &mut payload)
            .expect("auth request payload should encode");

        let mut expected = Vec::new();
        push_string(&mut expected, "client-1");
        push_string(&mut expected, "run-1");
        push_string(&mut expected, "0.1.0");
        push_string(&mut expected, "shared-secret");
        push_optional_string(&mut expected, Some("Alice"));
        assert_eq!(payload, expected);
    }

    #[test]
    fn protocol_message_encoder_encodes_auth_request_packet() {
        let request = AuthRequest {
            message_type: MessageType::AuthRequest,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            app_version: AppVersion("0.1.0".to_string()),
            shared_token: "shared-secret".to_string(),
            display_name: None,
            capabilities: Vec::new(),
            requested_video_profile: None,
        };
        let message = ProtocolMessage::AuthRequest(request.clone());
        let encoder = ProtocolMessageEncoderBoundary;
        let mut output = Vec::new();

        encoder
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &message,
                &mut output,
            )
            .expect("auth request packet should encode");

        let decoded = decode_fixed_header(&output).expect("encoded fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthRequest);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.flags, 0);
        assert_eq!(decoded.header.reserved, 0);

        let mut expected_payload = Vec::new();
        encode_auth_request_payload(&request, &mut expected_payload)
            .expect("expected payload should encode");
        assert_eq!(decoded.header.payload_length, expected_payload.len() as u32);
        assert_eq!(decoded.payload, expected_payload.as_slice());

        let decoded_request = decode_auth_request_payload(decoded.header, decoded.payload)
            .expect("encoded request should decode");
        assert_eq!(decoded_request, request);
    }

    #[test]
    fn encodes_auth_response_payload_without_optional_fields() {
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: None,
            expected_protocol_version: None,
        };
        let mut payload = Vec::new();

        encode_auth_response_payload(&response, &mut payload)
            .expect("auth response payload should encode");

        let mut expected = Vec::new();
        push_string(&mut expected, "client-1");
        push_string(&mut expected, "run-1");
        expected.push(1);
        push_u16(&mut expected, AuthResponseReasonCode::Ok.wire_code());
        expected.push(0);
        expected.push(0);
        expected.push(0);
        assert_eq!(payload, expected);
    }

    #[test]
    fn encodes_auth_response_payload_with_optional_fields() {
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(1),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            accepted: false,
            reason_code: AuthResponseReasonCode::ProtocolMismatch,
            message: Some("unsupported protocol_version".to_string()),
            server_time: Some(TimestampMicros(2_000_000)),
            expected_protocol_version: Some(ProtocolVersion(2)),
        };
        let mut payload = Vec::new();

        encode_auth_response_payload(&response, &mut payload)
            .expect("auth response payload should encode");

        let mut expected = Vec::new();
        push_string(&mut expected, "client-1");
        push_string(&mut expected, "run-1");
        expected.push(0);
        push_u16(
            &mut expected,
            AuthResponseReasonCode::ProtocolMismatch.wire_code(),
        );
        push_optional_string(&mut expected, Some("unsupported protocol_version"));
        push_optional_u64(&mut expected, Some(2_000_000));
        expected.push(1);
        push_u32(&mut expected, 2);
        assert_eq!(payload, expected);
    }

    #[test]
    fn protocol_message_encoder_encodes_auth_response_packet() {
        let response = AuthResponse {
            message_type: MessageType::AuthResponse,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            accepted: true,
            reason_code: AuthResponseReasonCode::Ok,
            message: None,
            server_time: Some(TimestampMicros(2_000_000)),
            expected_protocol_version: None,
        };
        let message = ProtocolMessage::AuthResponse(response.clone());
        let encoder = ProtocolMessageEncoderBoundary;
        let mut output = Vec::new();

        encoder
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &message,
                &mut output,
            )
            .expect("auth response packet should encode");

        let decoded = decode_fixed_header(&output).expect("encoded fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::AuthResponse);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.flags, 0);
        assert_eq!(decoded.header.reserved, 0);

        let mut expected_payload = Vec::new();
        encode_auth_response_payload(&response, &mut expected_payload)
            .expect("expected payload should encode");
        assert_eq!(decoded.header.payload_length, expected_payload.len() as u32);
        assert_eq!(decoded.payload, expected_payload.as_slice());
    }

    #[test]
    fn encodes_heartbeat_ack_payload() {
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        };
        let mut payload = Vec::new();

        encode_heartbeat_ack_payload(&ack, &mut payload)
            .expect("heartbeat ack payload should encode");

        let mut expected = Vec::new();
        push_string(&mut expected, "client-1");
        push_string(&mut expected, "run-1");
        push_u64(&mut expected, 1_000_000);
        push_u64(&mut expected, 1_000_100);
        push_u64(&mut expected, 1_000_200);
        assert_eq!(payload, expected);
    }

    #[test]
    fn protocol_message_encoder_encodes_heartbeat_ack_packet() {
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000_000),
            server_received_at: TimestampMicros(1_000_100),
            server_sent_at: TimestampMicros(1_000_200),
        };
        let message = ProtocolMessage::HeartbeatAck(ack.clone());
        let encoder = ProtocolMessageEncoderBoundary;
        let mut output = Vec::new();

        encoder
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &message,
                &mut output,
            )
            .expect("heartbeat ack packet should encode");

        let decoded = decode_fixed_header(&output).expect("encoded fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::HeartbeatAck);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.flags, 0);
        assert_eq!(decoded.header.reserved, 0);

        let mut expected_payload = Vec::new();
        encode_heartbeat_ack_payload(&ack, &mut expected_payload)
            .expect("expected payload should encode");
        assert_eq!(decoded.header.payload_length, expected_payload.len() as u32);
        assert_eq!(decoded.payload, expected_payload.as_slice());
    }

    #[test]
    fn heartbeat_ack_observation_boundary_preserves_ack_fields() {
        let ack = HeartbeatAck {
            message_type: MessageType::HeartbeatAck,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
        };
        let boundary = HeartbeatAckObservationBoundary;

        let observation = boundary.observe(&ack, TimestampMicros(1_150));

        assert_eq!(observation.client_id, ClientId("client-1".to_string()));
        assert_eq!(observation.run_id, RunId("run-1".to_string()));
        assert_eq!(observation.echoed_sent_at, TimestampMicros(1_000));
        assert_eq!(observation.server_received_at, TimestampMicros(2_100));
        assert_eq!(observation.server_sent_at, TimestampMicros(2_150));
        assert_eq!(observation.client_received_at, TimestampMicros(1_150));
    }

    #[test]
    fn heartbeat_observation_carrier_boundary_uses_client_stats_carrier() {
        let observation = HeartbeatAckObservation {
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            echoed_sent_at: TimestampMicros(1_000),
            server_received_at: TimestampMicros(2_100),
            server_sent_at: TimestampMicros(2_150),
            client_received_at: TimestampMicros(1_150),
        };
        let boundary = HeartbeatObservationCarrierBoundary;

        let carrier = boundary.build_client_stats_carrier(ProtocolVersion(2), observation.clone());

        assert_eq!(carrier.message_type, MessageType::ClientStats);
        assert_eq!(carrier.protocol_version, ProtocolVersion(2));
        assert_eq!(carrier.observation, observation);
    }

    #[test]
    fn encodes_video_frame_payload_with_metadata_and_h264_bytes() {
        let frame = test_video_frame(vec![0xaa, 0xbb, 0xcc]);
        let mut payload = Vec::new();

        encode_video_frame_payload(&frame, &mut payload)
            .expect("video frame payload should encode");

        let expected =
            video_frame_payload(1, [0; 3], CODEC_H264_WIRE_VALUE, &[0xaa, 0xbb, 0xcc], 3);
        assert_eq!(payload, expected);
    }

    #[test]
    fn protocol_message_encoder_encodes_video_frame_packet() {
        let frame = test_video_frame(vec![0xaa, 0xbb, 0xcc]);
        let message = ProtocolMessage::VideoFrame(frame.clone());
        let encoder = ProtocolMessageEncoderBoundary;
        let mut output = Vec::new();

        encoder
            .encode_message(
                EncodeContext {
                    protocol_version: ProtocolVersion(2),
                },
                &message,
                &mut output,
            )
            .expect("video frame packet should encode");

        let decoded = decode_fixed_header(&output).expect("encoded fixed header should decode");
        assert_eq!(decoded.header.message_type, MessageType::VideoFrame);
        assert_eq!(decoded.header.header_length, FIXED_HEADER_LEN);
        assert_eq!(decoded.header.protocol_version, ProtocolVersion(2));
        assert_eq!(decoded.header.flags, 0);
        assert_eq!(decoded.header.reserved, 0);

        let mut expected_payload = Vec::new();
        encode_video_frame_payload(&frame, &mut expected_payload)
            .expect("expected payload should encode");
        assert_eq!(decoded.header.payload_length, expected_payload.len() as u32);
        assert_eq!(decoded.payload, expected_payload.as_slice());

        let decoded_frame = decode_video_frame_payload(decoded.header, decoded.payload)
            .expect("encoded frame should decode");
        assert_eq!(decoded_frame, frame);
    }

    #[test]
    fn rejects_video_frame_encode_payload_size_mismatch() {
        let mut frame = test_video_frame(vec![0xaa, 0xbb, 0xcc]);
        frame.payload_size = 999;
        let mut payload = Vec::new();

        let error = encode_video_frame_payload(&frame, &mut payload);

        assert_eq!(
            error,
            Err(ProtocolError::InvalidPayloadLength {
                expected: 3,
                actual: 999
            })
        );
    }

    #[test]
    fn rejects_video_frame_encode_reserved_metadata() {
        let mut frame = test_video_frame(vec![0xaa]);
        frame.metadata_reserved = [1, 0, 0];
        let mut payload = Vec::new();

        let error = encode_video_frame_payload(&frame, &mut payload);

        assert_eq!(
            error,
            Err(ProtocolError::InvalidMetadataReserved { actual: [1, 0, 0] })
        );
    }

    #[test]
    fn decodes_auth_request_payload_with_display_name() {
        let payload = auth_request_payload(Some("Alice"));
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let request = decode_auth_request_payload(decoded.header, decoded.payload)
            .expect("auth request should decode");

        assert_eq!(request.message_type, MessageType::AuthRequest);
        assert_eq!(request.protocol_version, ProtocolVersion(2));
        assert_eq!(request.client_id, ClientId("client-1".to_string()));
        assert_eq!(request.run_id, RunId("run-1".to_string()));
        assert_eq!(request.app_version, AppVersion("0.1.0".to_string()));
        assert_eq!(request.shared_token, "shared-secret");
        assert_eq!(request.display_name, Some("Alice".to_string()));
        assert!(request.capabilities.is_empty());
        assert_eq!(request.requested_video_profile, None);
    }

    #[test]
    fn decodes_auth_request_payload_without_display_name() {
        let payload = auth_request_payload(None);
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");
        let decoder = AuthRequestPayloadDecoder;

        let message = decoder
            .decode_payload(
                DecodeContext {
                    expected_protocol_version: ProtocolVersion(2),
                },
                decoded.header,
                decoded.payload,
            )
            .expect("auth request should decode");

        let ProtocolMessage::AuthRequest(request) = message else {
            panic!("expected auth request message");
        };
        assert_eq!(request.display_name, None);
    }

    #[test]
    fn rejects_auth_request_payload_for_unexpected_message_type() {
        let payload = auth_request_payload(None);
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let request = decode_auth_request_payload(decoded.header, decoded.payload);

        assert_eq!(
            request,
            Err(ProtocolError::UnexpectedMessageType {
                expected: MessageType::AuthRequest,
                actual: MessageType::Heartbeat
            })
        );
    }

    #[test]
    fn rejects_truncated_auth_request_string() {
        let mut payload = auth_request_payload(None);
        payload.truncate(payload.len() - 1);
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let request = decode_auth_request_payload(decoded.header, decoded.payload);

        assert_eq!(
            request,
            Err(ProtocolError::InvalidPayloadLength {
                expected: payload.len() as u32 + 1,
                actual: payload.len()
            })
        );
    }

    #[test]
    fn rejects_invalid_auth_request_optional_tag() {
        let mut payload = auth_request_payload(None);
        let tag_offset = payload.len() - usize::from(PAYLOAD_OPTION_TAG_LEN);
        payload[tag_offset] = 2;
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let request = decode_auth_request_payload(decoded.header, decoded.payload);

        assert_eq!(
            request,
            Err(ProtocolError::InvalidOptionalTag { actual: 2 })
        );
    }

    #[test]
    fn rejects_invalid_auth_request_utf8() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.push(0xff);
        push_string(&mut payload, "run-1");
        push_string(&mut payload, "0.1.0");
        push_string(&mut payload, "shared-secret");
        payload.push(0);
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let request = decode_auth_request_payload(decoded.header, decoded.payload);

        assert_eq!(request, Err(ProtocolError::InvalidUtf8String));
    }

    #[test]
    fn decodes_heartbeat_payload_with_optional_fields() {
        let payload = heartbeat_payload(Some(1_234_999), Some("ok"));
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let heartbeat = decode_heartbeat_payload(decoded.header, decoded.payload)
            .expect("heartbeat should decode");

        assert_eq!(heartbeat.message_type, MessageType::Heartbeat);
        assert_eq!(heartbeat.protocol_version, ProtocolVersion(2));
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
        assert_eq!(heartbeat.run_id, RunId("run-1".to_string()));
        assert_eq!(heartbeat.sent_at, TimestampMicros(1_234_567));
        assert_eq!(heartbeat.local_time, Some(TimestampMicros(1_234_999)));
        assert_eq!(heartbeat.short_status, Some("ok".to_string()));
    }

    #[test]
    fn decodes_heartbeat_payload_without_optional_fields() {
        let payload = heartbeat_payload(None, None);
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");
        let decoder = HeartbeatPayloadDecoder;

        let message = decoder
            .decode_payload(
                DecodeContext {
                    expected_protocol_version: ProtocolVersion(2),
                },
                decoded.header,
                decoded.payload,
            )
            .expect("heartbeat should decode");

        let ProtocolMessage::Heartbeat(heartbeat) = message else {
            panic!("expected heartbeat message");
        };
        assert_eq!(heartbeat.local_time, None);
        assert_eq!(heartbeat.short_status, None);
    }

    #[test]
    fn rejects_heartbeat_payload_for_unexpected_message_type() {
        let payload = heartbeat_payload(None, None);
        let packet = test_packet(
            MessageType::AuthRequest as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let heartbeat = decode_heartbeat_payload(decoded.header, decoded.payload);

        assert_eq!(
            heartbeat,
            Err(ProtocolError::UnexpectedMessageType {
                expected: MessageType::Heartbeat,
                actual: MessageType::AuthRequest
            })
        );
    }

    #[test]
    fn rejects_truncated_heartbeat_optional_timestamp() {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_u64(&mut payload, 1_234_567);
        payload.push(1);
        payload.extend_from_slice(&1_u32.to_le_bytes());
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let heartbeat = decode_heartbeat_payload(decoded.header, decoded.payload);

        assert_eq!(
            heartbeat,
            Err(ProtocolError::InvalidPayloadLength {
                expected: payload.len() as u32 + 4,
                actual: payload.len()
            })
        );
    }

    #[test]
    fn rejects_invalid_heartbeat_optional_tag() {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_u64(&mut payload, 1_234_567);
        payload.push(2);
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let heartbeat = decode_heartbeat_payload(decoded.header, decoded.payload);

        assert_eq!(
            heartbeat,
            Err(ProtocolError::InvalidOptionalTag { actual: 2 })
        );
    }

    #[test]
    fn decodes_video_frame_payload() {
        let h264_payload = [0xaa, 0xbb, 0xcc];
        let payload = video_frame_payload(1, [0; 3], CODEC_H264_WIRE_VALUE, &h264_payload, 3);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload)
            .expect("frame should decode");

        assert_eq!(frame.message_type, MessageType::VideoFrame);
        assert_eq!(frame.protocol_version, ProtocolVersion(2));
        assert_eq!(frame.client_id, ClientId("client-1".to_string()));
        assert_eq!(frame.run_id, RunId("run-1".to_string()));
        assert_eq!(frame.frame_id, 42);
        assert_eq!(frame.capture_timestamp, TimestampMicros(1_000_000));
        assert_eq!(frame.send_timestamp, TimestampMicros(1_000_100));
        assert!(frame.is_keyframe);
        assert_eq!(frame.metadata_reserved, [0; 3]);
        assert_eq!(frame.width, 1280);
        assert_eq!(frame.height, 720);
        assert_eq!(frame.fps_nominal, 30);
        assert_eq!(frame.codec, Codec::H264);
        assert_eq!(frame.payload_size, h264_payload.len());
        assert_eq!(frame.payload, h264_payload);
    }

    #[test]
    fn decodes_video_frame_payload_through_decoder() {
        let h264_payload = [0x01, 0x02];
        let payload = video_frame_payload(0, [0; 3], CODEC_H264_WIRE_VALUE, &h264_payload, 2);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");
        let decoder = VideoFramePayloadDecoder;

        let message = decoder
            .decode_payload(
                DecodeContext {
                    expected_protocol_version: ProtocolVersion(2),
                },
                decoded.header,
                decoded.payload,
            )
            .expect("frame should decode");

        let ProtocolMessage::VideoFrame(frame) = message else {
            panic!("expected video frame message");
        };
        assert!(!frame.is_keyframe);
        assert_eq!(frame.payload, h264_payload);
    }

    #[test]
    fn dispatches_payload_decode_by_message_type() {
        let payload = heartbeat_payload(None, None);
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let message = decode_payload_by_message_type(
            DecodeContext {
                expected_protocol_version: ProtocolVersion(2),
            },
            decoded.header,
            decoded.payload,
        )
        .expect("heartbeat should decode");

        let ProtocolMessage::Heartbeat(heartbeat) = message else {
            panic!("expected heartbeat message");
        };
        assert_eq!(heartbeat.client_id, ClientId("client-1".to_string()));
    }

    #[test]
    fn returns_not_implemented_for_undecoded_message_type() {
        let packet = test_packet(MessageType::AuthResponse as u16, FIXED_HEADER_LEN, 2, &[]);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let message = decode_payload_by_message_type(
            DecodeContext {
                expected_protocol_version: ProtocolVersion(2),
            },
            decoded.header,
            decoded.payload,
        );

        assert_eq!(
            message,
            Err(ProtocolError::PayloadDecodeNotImplemented(
                MessageType::AuthResponse
            ))
        );
    }

    #[test]
    fn rejects_video_frame_payload_for_unexpected_message_type() {
        let payload = video_frame_payload(1, [0; 3], CODEC_H264_WIRE_VALUE, &[0xaa], 1);
        let packet = test_packet(MessageType::Heartbeat as u16, FIXED_HEADER_LEN, 2, &payload);
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload);

        assert_eq!(
            frame,
            Err(ProtocolError::UnexpectedMessageType {
                expected: MessageType::VideoFrame,
                actual: MessageType::Heartbeat
            })
        );
    }

    #[test]
    fn rejects_video_frame_payload_size_mismatch() {
        let payload = video_frame_payload(1, [0; 3], CODEC_H264_WIRE_VALUE, &[0xaa, 0xbb], 3);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload);

        assert!(matches!(
            frame,
            Err(ProtocolError::InvalidPayloadLength { .. })
        ));
    }

    #[test]
    fn rejects_invalid_video_frame_bool() {
        let payload = video_frame_payload(2, [0; 3], CODEC_H264_WIRE_VALUE, &[0xaa], 1);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload);

        assert_eq!(frame, Err(ProtocolError::InvalidBoolValue { actual: 2 }));
    }

    #[test]
    fn rejects_invalid_video_frame_metadata_reserved() {
        let payload = video_frame_payload(1, [1, 0, 0], CODEC_H264_WIRE_VALUE, &[0xaa], 1);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload);

        assert_eq!(
            frame,
            Err(ProtocolError::InvalidMetadataReserved { actual: [1, 0, 0] })
        );
    }

    #[test]
    fn rejects_unsupported_video_frame_codec() {
        let payload = video_frame_payload(1, [0; 3], 999, &[0xaa], 1);
        let packet = test_packet(
            MessageType::VideoFrame as u16,
            FIXED_HEADER_LEN,
            2,
            &payload,
        );
        let decoded = decode_fixed_header(&packet).expect("fixed header should decode");

        let frame = decode_video_frame_payload(decoded.header, decoded.payload);

        assert_eq!(frame, Err(ProtocolError::UnsupportedCodec { actual: 999 }));
    }

    fn auth_request_payload(display_name: Option<&str>) -> Vec<u8> {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_string(&mut payload, "0.1.0");
        push_string(&mut payload, "shared-secret");
        push_optional_string(&mut payload, display_name);
        payload
    }

    fn heartbeat_payload(local_time: Option<u64>, short_status: Option<&str>) -> Vec<u8> {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_u64(&mut payload, 1_234_567);
        push_optional_u64(&mut payload, local_time);
        push_optional_string(&mut payload, short_status);
        payload
    }

    fn video_frame_payload(
        is_keyframe: u8,
        metadata_reserved: [u8; 3],
        codec: u16,
        h264_payload: &[u8],
        declared_payload_size: u32,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_string(&mut payload, "client-1");
        push_string(&mut payload, "run-1");
        push_u64(&mut payload, 42);
        push_u64(&mut payload, 1_000_000);
        push_u64(&mut payload, 1_000_100);
        payload.push(is_keyframe);
        payload.extend_from_slice(&metadata_reserved);
        push_u32(&mut payload, 1280);
        push_u32(&mut payload, 720);
        push_u32(&mut payload, 30);
        push_u16(&mut payload, codec);
        push_u32(&mut payload, declared_payload_size);
        payload.extend_from_slice(h264_payload);
        payload
    }

    fn test_video_frame(payload: Vec<u8>) -> VideoFrame {
        VideoFrame {
            message_type: MessageType::VideoFrame,
            protocol_version: ProtocolVersion(2),
            client_id: ClientId("client-1".to_string()),
            run_id: RunId("run-1".to_string()),
            frame_id: 42,
            capture_timestamp: TimestampMicros(1_000_000),
            send_timestamp: TimestampMicros(1_000_100),
            is_keyframe: true,
            metadata_reserved: [0; 3],
            width: 1280,
            height: 720,
            fps_nominal: 30,
            codec: Codec::H264,
            payload_size: payload.len(),
            payload,
        }
    }

    fn push_string(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(&(value.len() as u16).to_le_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    fn push_optional_string(output: &mut Vec<u8>, value: Option<&str>) {
        match value {
            Some(value) => {
                output.push(1);
                push_string(output, value);
            }
            None => output.push(0),
        }
    }

    fn push_u16(output: &mut Vec<u8>, value: u16) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u64(output: &mut Vec<u8>, value: u64) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_optional_u64(output: &mut Vec<u8>, value: Option<u64>) {
        match value {
            Some(value) => {
                output.push(1);
                push_u64(output, value);
            }
            None => output.push(0),
        }
    }

    fn test_packet(
        message_type: u16,
        header_length: u16,
        protocol_version: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = vec![0; usize::from(FIXED_HEADER_LEN)];
        packet[HEADER_MESSAGE_TYPE_OFFSET..HEADER_MESSAGE_TYPE_OFFSET + 2]
            .copy_from_slice(&message_type.to_le_bytes());
        packet[HEADER_LENGTH_OFFSET..HEADER_LENGTH_OFFSET + 2]
            .copy_from_slice(&header_length.to_le_bytes());
        packet[HEADER_PROTOCOL_VERSION_OFFSET..HEADER_PROTOCOL_VERSION_OFFSET + 4]
            .copy_from_slice(&protocol_version.to_le_bytes());
        packet[HEADER_PAYLOAD_LENGTH_OFFSET..HEADER_PAYLOAD_LENGTH_OFFSET + 4]
            .copy_from_slice(&(payload.len() as u32).to_le_bytes());
        packet[HEADER_FLAGS_OFFSET..HEADER_FLAGS_OFFSET + 2]
            .copy_from_slice(&0x0034_u16.to_le_bytes());
        packet[HEADER_RESERVED_OFFSET..HEADER_RESERVED_OFFSET + 2]
            .copy_from_slice(&0_u16.to_le_bytes());
        packet.extend_from_slice(payload);
        packet
    }
}
