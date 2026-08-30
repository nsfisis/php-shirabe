//! Wire framing: `[u64 length LE][u8 tag][u64 corr_id LE][payload]`, where `length` counts
//! everything after itself (tag + corr_id + payload) and the payload is the frame's remaining
//! fields as a PHP-serialized list. See `docs/dev/php-rpc.md`.

use crate::value::{self, PluginValue};
use indexmap::IndexMap;
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;

/// Upper bound for the declared frame length, so a corrupted length header cannot make the
/// process allocate absurd amounts of memory.
pub const MAX_FRAME_LEN: u64 = 256 * 1024 * 1024;

pub const TAG_CALL_FUNCTION: u8 = 0x00;
pub const TAG_CALL_STATIC_METHOD: u8 = 0x01;
pub const TAG_NEW_OBJECT: u8 = 0x02;
pub const TAG_CALL_PHP_METHOD: u8 = 0x03;
pub const TAG_CALL_RUST_METHOD: u8 = 0x04;
pub const TAG_RETURN: u8 = 0x05;
pub const TAG_THROW: u8 = 0x06;
pub const TAG_RELEASE_RUST_HANDLE: u8 = 0x07;
pub const TAG_RELEASE_PHP_HANDLE: u8 = 0x08;
pub const TAG_EPOCH_BUMP: u8 = 0x09;

#[derive(Debug)]
pub enum Frame {
    CallFunction {
        corr_id: u64,
        function_name: String,
        args: Vec<PluginValue>,
        out_param_positions: Vec<u32>,
    },
    CallStaticMethod {
        corr_id: u64,
        pclass: String,
        method_name: String,
        args: Vec<PluginValue>,
        out_param_positions: Vec<u32>,
    },
    NewObject {
        corr_id: u64,
        pclass: String,
        ctor_args: Vec<PluginValue>,
    },
    CallPhpMethod {
        corr_id: u64,
        phandle: u64,
        method_name: String,
        args: Vec<PluginValue>,
        out_param_positions: Vec<u32>,
    },
    CallRustMethod {
        corr_id: u64,
        rhandle: u64,
        method_name: String,
        args: Vec<PluginValue>,
        out_param_positions: Vec<u32>,
    },
    Return {
        corr_id: u64,
        value: PluginValue,
        out_params: IndexMap<u32, PluginValue>,
    },
    Throw {
        corr_id: u64,
        exception_class: String,
        message: String,
        code: i64,
        /// The state the exception carries beyond `message` and `code`, keyed by the property
        /// names its class declares. Empty for an exception that carries none.
        properties: IndexMap<String, PluginValue>,
    },
    ReleaseRustHandle {
        rhandle: u64,
    },
    ReleasePhpHandle {
        phandle: u64,
    },
    EpochBump {
        rhandle: u64,
        epoch: u64,
    },
}

impl Frame {
    fn tag(&self) -> u8 {
        match self {
            Frame::CallFunction { .. } => TAG_CALL_FUNCTION,
            Frame::CallStaticMethod { .. } => TAG_CALL_STATIC_METHOD,
            Frame::NewObject { .. } => TAG_NEW_OBJECT,
            Frame::CallPhpMethod { .. } => TAG_CALL_PHP_METHOD,
            Frame::CallRustMethod { .. } => TAG_CALL_RUST_METHOD,
            Frame::Return { .. } => TAG_RETURN,
            Frame::Throw { .. } => TAG_THROW,
            Frame::ReleaseRustHandle { .. } => TAG_RELEASE_RUST_HANDLE,
            Frame::ReleasePhpHandle { .. } => TAG_RELEASE_PHP_HANDLE,
            Frame::EpochBump { .. } => TAG_EPOCH_BUMP,
        }
    }

    /// One-way notifications carry no correlation id; the field is 0 on the wire.
    fn corr_id(&self) -> u64 {
        match self {
            Frame::CallFunction { corr_id, .. }
            | Frame::CallStaticMethod { corr_id, .. }
            | Frame::NewObject { corr_id, .. }
            | Frame::CallPhpMethod { corr_id, .. }
            | Frame::CallRustMethod { corr_id, .. }
            | Frame::Return { corr_id, .. }
            | Frame::Throw { corr_id, .. } => *corr_id,
            Frame::ReleaseRustHandle { .. }
            | Frame::ReleasePhpHandle { .. }
            | Frame::EpochBump { .. } => 0,
        }
    }

    fn fields(&self) -> Vec<PluginValue> {
        match self {
            Frame::CallFunction {
                function_name,
                args,
                out_param_positions,
                ..
            } => vec![
                PluginValue::string(function_name.clone()),
                PluginValue::List(args.clone()),
                positions_value(out_param_positions),
            ],
            Frame::CallStaticMethod {
                pclass,
                method_name,
                args,
                out_param_positions,
                ..
            } => vec![
                PluginValue::string(pclass.clone()),
                PluginValue::string(method_name.clone()),
                PluginValue::List(args.clone()),
                positions_value(out_param_positions),
            ],
            Frame::NewObject {
                pclass, ctor_args, ..
            } => vec![
                PluginValue::string(pclass.clone()),
                PluginValue::List(ctor_args.clone()),
            ],
            Frame::CallPhpMethod {
                phandle,
                method_name,
                args,
                out_param_positions,
                ..
            } => vec![
                int_value(*phandle),
                PluginValue::string(method_name.clone()),
                PluginValue::List(args.clone()),
                positions_value(out_param_positions),
            ],
            Frame::CallRustMethod {
                rhandle,
                method_name,
                args,
                out_param_positions,
                ..
            } => vec![
                int_value(*rhandle),
                PluginValue::string(method_name.clone()),
                PluginValue::List(args.clone()),
                positions_value(out_param_positions),
            ],
            Frame::Return {
                value, out_params, ..
            } => vec![
                value.clone(),
                PluginValue::Array(
                    out_params
                        .iter()
                        .map(|(pos, v)| (pos.to_string().into_bytes(), v.clone()))
                        .collect(),
                ),
            ],
            Frame::Throw {
                exception_class,
                message,
                code,
                properties,
                ..
            } => vec![
                PluginValue::string(exception_class.clone()),
                PluginValue::string(message.clone()),
                PluginValue::Int(*code),
                PluginValue::Array(
                    properties
                        .iter()
                        .map(|(name, v)| (name.clone().into_bytes(), v.clone()))
                        .collect(),
                ),
            ],
            Frame::ReleaseRustHandle { rhandle } => vec![int_value(*rhandle)],
            Frame::ReleasePhpHandle { phandle } => vec![int_value(*phandle)],
            Frame::EpochBump { rhandle, epoch } => vec![int_value(*rhandle), int_value(*epoch)],
        }
    }
}

fn int_value(id: u64) -> PluginValue {
    PluginValue::Int(i64::try_from(id).expect("handle id exceeds i64"))
}

fn positions_value(positions: &[u32]) -> PluginValue {
    PluginValue::List(
        positions
            .iter()
            .map(|p| PluginValue::Int(i64::from(*p)))
            .collect(),
    )
}

pub fn write_frame(stream: &mut UnixStream, frame: &Frame) -> std::io::Result<()> {
    let payload = value::serialize(&PluginValue::List(frame.fields()));
    let len = 1 + 8 + payload.len() as u64;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&[frame.tag()])?;
    stream.write_all(&frame.corr_id().to_le_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

pub fn read_frame(stream: &mut UnixStream) -> std::io::Result<Frame> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let len = u64::from_le_bytes(header);
    if len > MAX_FRAME_LEN {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame length {len} exceeds MAX_FRAME_LEN"),
        ));
    }
    if len < 9 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame length {len} is shorter than the tag and corr_id fields"),
        ));
    }
    let mut tag = [0u8; 1];
    stream.read_exact(&mut tag)?;
    let mut corr_id = [0u8; 8];
    stream.read_exact(&mut corr_id)?;
    let mut payload = vec![0u8; (len - 9) as usize];
    stream.read_exact(&mut payload)?;
    Ok(decode_frame(tag[0], u64::from_le_bytes(corr_id), &payload))
}

/// Both sides of this protocol are written in the same commit of Shirabe, so a frame that does
/// not decode is a programming error, not a runtime condition; every mismatch panics.
fn decode_frame(tag: u8, corr_id: u64, payload: &[u8]) -> Frame {
    let fields = match value::unserialize(payload) {
        Ok(PluginValue::List(fields)) => fields,
        other => panic!("PHP RPC: protocol violation — frame payload is not a list: {other:?}"),
    };
    let mut fields = fields.into_iter();
    let mut next = || {
        fields
            .next()
            .unwrap_or_else(|| panic!("PHP RPC: protocol violation — missing frame field"))
    };
    match tag {
        TAG_CALL_FUNCTION => Frame::CallFunction {
            corr_id,
            function_name: expect_string(next()),
            args: expect_list(next()),
            out_param_positions: expect_positions(next()),
        },
        TAG_CALL_STATIC_METHOD => Frame::CallStaticMethod {
            corr_id,
            pclass: expect_string(next()),
            method_name: expect_string(next()),
            args: expect_list(next()),
            out_param_positions: expect_positions(next()),
        },
        TAG_NEW_OBJECT => Frame::NewObject {
            corr_id,
            pclass: expect_string(next()),
            ctor_args: expect_list(next()),
        },
        TAG_CALL_PHP_METHOD => Frame::CallPhpMethod {
            corr_id,
            phandle: expect_id(next()),
            method_name: expect_string(next()),
            args: expect_list(next()),
            out_param_positions: expect_positions(next()),
        },
        TAG_CALL_RUST_METHOD => Frame::CallRustMethod {
            corr_id,
            rhandle: expect_id(next()),
            method_name: expect_string(next()),
            args: expect_list(next()),
            out_param_positions: expect_positions(next()),
        },
        TAG_RETURN => Frame::Return {
            corr_id,
            value: next(),
            out_params: expect_out_params(next()),
        },
        TAG_THROW => Frame::Throw {
            corr_id,
            exception_class: expect_string(next()),
            message: expect_string(next()),
            code: match next() {
                PluginValue::Int(code) => code,
                other => {
                    panic!("PHP RPC: protocol violation — Throw code is not an int: {other:?}")
                }
            },
            properties: expect_properties(next()),
        },
        TAG_RELEASE_RUST_HANDLE => Frame::ReleaseRustHandle {
            rhandle: expect_id(next()),
        },
        TAG_RELEASE_PHP_HANDLE => Frame::ReleasePhpHandle {
            phandle: expect_id(next()),
        },
        TAG_EPOCH_BUMP => Frame::EpochBump {
            rhandle: expect_id(next()),
            epoch: expect_id(next()),
        },
        _ => panic!("PHP RPC: protocol violation — unknown frame tag {tag:#04x}"),
    }
}

fn expect_string(value: PluginValue) -> String {
    match value {
        PluginValue::String(bytes) => String::from_utf8(bytes)
            .unwrap_or_else(|e| panic!("PHP RPC: protocol violation — non-UTF-8 name field: {e}")),
        other => panic!("PHP RPC: protocol violation — expected a string field: {other:?}"),
    }
}

fn expect_list(value: PluginValue) -> Vec<PluginValue> {
    match value {
        PluginValue::List(items) => items,
        other => panic!("PHP RPC: protocol violation — expected a list field: {other:?}"),
    }
}

fn expect_id(value: PluginValue) -> u64 {
    match value {
        PluginValue::Int(n) => u64::try_from(n)
            .unwrap_or_else(|_| panic!("PHP RPC: protocol violation — negative handle id {n}")),
        other => panic!("PHP RPC: protocol violation — expected an int id field: {other:?}"),
    }
}

fn expect_positions(value: PluginValue) -> Vec<u32> {
    expect_list(value)
        .into_iter()
        .map(|item| match item {
            PluginValue::Int(n) => u32::try_from(n).unwrap_or_else(|_| {
                panic!("PHP RPC: protocol violation — out param position {n} out of range")
            }),
            other => {
                panic!("PHP RPC: protocol violation — out param position is not an int: {other:?}")
            }
        })
        .collect()
}

fn expect_properties(value: PluginValue) -> IndexMap<String, PluginValue> {
    match value {
        PluginValue::List(items) if items.is_empty() => IndexMap::new(),
        PluginValue::Array(map) => map
            .into_iter()
            .map(|(key, item)| {
                let name = String::from_utf8(key).unwrap_or_else(|error| {
                    panic!(
                        "PHP RPC: protocol violation — exception property name is not UTF-8: {:?}",
                        String::from_utf8_lossy(error.as_bytes())
                    )
                });
                (name, item)
            })
            .collect(),
        other => {
            panic!("PHP RPC: protocol violation — exception properties is not an array: {other:?}")
        }
    }
}

fn expect_out_params(value: PluginValue) -> IndexMap<u32, PluginValue> {
    match value {
        PluginValue::List(items) if items.is_empty() => IndexMap::new(),
        PluginValue::List(items) => items
            .into_iter()
            .enumerate()
            .map(|(index, item)| (index as u32, item))
            .collect(),
        PluginValue::Array(map) => map
            .into_iter()
            .map(|(key, item)| {
                let position = std::str::from_utf8(&key)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| {
                        panic!(
                            "PHP RPC: protocol violation — out param key is not a position: {:?}",
                            String::from_utf8_lossy(&key)
                        )
                    });
                (position, item)
            })
            .collect(),
        other => panic!("PHP RPC: protocol violation — out_params is not an array: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(frame: Frame) -> Frame {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, &frame).unwrap();
        read_frame(&mut b).unwrap()
    }

    #[test]
    fn frame_roundtrip_call_function() {
        let frame = roundtrip(Frame::CallFunction {
            corr_id: 7,
            function_name: "defined".to_string(),
            args: vec![PluginValue::string("PHP_VERSION")],
            out_param_positions: vec![],
        });
        match frame {
            Frame::CallFunction {
                corr_id,
                function_name,
                args,
                out_param_positions,
            } => {
                assert_eq!(corr_id, 7);
                assert_eq!(function_name, "defined");
                assert_eq!(args, vec![PluginValue::string("PHP_VERSION")]);
                assert!(out_param_positions.is_empty());
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_throw_with_properties() {
        let frame = roundtrip(Frame::Throw {
            corr_id: 11,
            exception_class: "Composer\\Downloader\\TransportException".to_string(),
            message: "The \"https://example.org\" file could not be downloaded".to_string(),
            code: 401,
            properties: [
                ("statusCode".to_string(), PluginValue::Int(401)),
                ("response".to_string(), PluginValue::Null),
            ]
            .into_iter()
            .collect(),
        });
        match frame {
            Frame::Throw {
                corr_id,
                exception_class,
                message,
                code,
                properties,
            } => {
                assert_eq!(corr_id, 11);
                assert_eq!(exception_class, "Composer\\Downloader\\TransportException");
                assert_eq!(
                    message,
                    "The \"https://example.org\" file could not be downloaded"
                );
                assert_eq!(code, 401);
                assert_eq!(properties.get("statusCode"), Some(&PluginValue::Int(401)));
                assert_eq!(properties.get("response"), Some(&PluginValue::Null));
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_return_with_out_params() {
        let frame = roundtrip(Frame::Return {
            corr_id: 9,
            value: PluginValue::Bool(true),
            out_params: [(2u32, PluginValue::string("x"))].into_iter().collect(),
        });
        match frame {
            Frame::Return {
                corr_id,
                value,
                out_params,
            } => {
                assert_eq!(corr_id, 9);
                assert_eq!(value, PluginValue::Bool(true));
                assert_eq!(out_params.get(&2), Some(&PluginValue::string("x")));
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_one_way_notification() {
        let frame = roundtrip(Frame::EpochBump {
            rhandle: 4,
            epoch: 2,
        });
        match frame {
            Frame::EpochBump { rhandle, epoch } => {
                assert_eq!(rhandle, 4);
                assert_eq!(epoch, 2);
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn read_frame_rejects_oversized_length() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        a.write_all(&(MAX_FRAME_LEN + 1).to_le_bytes()).unwrap();
        let err = read_frame(&mut b).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("MAX_FRAME_LEN"), "{err}");
    }
}
