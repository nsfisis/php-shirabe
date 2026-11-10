//! Wire framing: `[u64 length LE][u8 tag][u64 corr_id LE][payload]`, where `length` counts
//! everything after itself (tag + corr_id + payload) and the payload is the frame's remaining
//! fields as a PHP-serialized list. See `docs/dev/php-rpc.md`.

use crate::PhpThrow;
use crate::codec::{FromPluginValue, protocol_value};
use crate::value::{self, PluginValue};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{ToPhpString, utf8_text};
use std::io::{Read as _, Write as _};

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

#[derive(Debug)]
pub enum Frame {
    CallFunction {
        corr_id: u64,
        function_name: BString,
        args: Vec<PluginValue>,
    },
    CallStaticMethod {
        corr_id: u64,
        pclass: BString,
        method_name: BString,
        args: Vec<PluginValue>,
    },
    NewObject {
        corr_id: u64,
        pclass: BString,
        ctor_args: Vec<PluginValue>,
    },
    CallPhpMethod {
        corr_id: u64,
        phandle: u64,
        method_name: BString,
        args: Vec<PluginValue>,
    },
    CallRustMethod {
        corr_id: u64,
        rhandle: u64,
        method_name: BString,
        args: Vec<PluginValue>,
    },
    Return {
        corr_id: u64,
        value: PluginValue,
        out_params: IndexMap<u32, PluginValue>,
    },
    /// The payload is the exception's description (`PhpThrow::to_plugin_value`) itself.
    Throw {
        corr_id: u64,
        throw: PhpThrow,
    },
    ReleaseRustHandle {
        rhandle: u64,
    },
    ReleasePhpHandle {
        phandle: u64,
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
            Frame::ReleaseRustHandle { .. } | Frame::ReleasePhpHandle { .. } => 0,
        }
    }

    fn fields(&self) -> Vec<PluginValue> {
        match self {
            Frame::CallFunction {
                function_name,
                args,
                ..
            } => vec![
                PluginValue::string(function_name.clone()),
                PluginValue::List(args.clone()),
            ],
            Frame::CallStaticMethod {
                pclass,
                method_name,
                args,
                ..
            } => vec![
                PluginValue::string(pclass.clone()),
                PluginValue::string(method_name.clone()),
                PluginValue::List(args.clone()),
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
                ..
            } => vec![
                int_value(*phandle),
                PluginValue::String(method_name.clone()),
                PluginValue::List(args.clone()),
            ],
            Frame::CallRustMethod {
                rhandle,
                method_name,
                args,
                ..
            } => vec![
                int_value(*rhandle),
                PluginValue::string(method_name.clone()),
                PluginValue::List(args.clone()),
            ],
            Frame::Return {
                value, out_params, ..
            } => vec![
                value.clone(),
                PluginValue::Array(
                    out_params
                        .iter()
                        .map(|(pos, v)| (pos.to_php_string(), v.clone()))
                        .collect(),
                ),
            ],
            Frame::Throw { throw, .. } => throw.payload_fields(),
            Frame::ReleaseRustHandle { rhandle } => vec![int_value(*rhandle)],
            Frame::ReleasePhpHandle { phandle } => vec![int_value(*phandle)],
        }
    }
}

fn int_value(id: u64) -> PluginValue {
    PluginValue::Int(i64::try_from(id).expect("handle id exceeds i64"))
}

pub fn write_frame(
    stream: &mut std::os::unix::net::UnixStream,
    frame: &Frame,
) -> std::io::Result<()> {
    let payload = value::serialize(&PluginValue::List(frame.fields()));
    let len = 1 + 8 + payload.len() as u64;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&[frame.tag()])?;
    stream.write_all(&frame.corr_id().to_le_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[expect(
    clippy::disallowed_macros,
    reason = "std::io::Error takes text, and the message holds no PHP string"
)]
pub fn read_frame(stream: &mut std::os::unix::net::UnixStream) -> std::io::Result<Frame> {
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
            function_name: field(next()),
            args: field(next()),
        },
        TAG_CALL_STATIC_METHOD => Frame::CallStaticMethod {
            corr_id,
            pclass: field(next()),
            method_name: field(next()),
            args: field(next()),
        },
        TAG_NEW_OBJECT => Frame::NewObject {
            corr_id,
            pclass: field(next()),
            ctor_args: field(next()),
        },
        TAG_CALL_PHP_METHOD => Frame::CallPhpMethod {
            corr_id,
            phandle: expect_id(next()),
            method_name: field(next()),
            args: field(next()),
        },
        TAG_CALL_RUST_METHOD => Frame::CallRustMethod {
            corr_id,
            rhandle: expect_id(next()),
            method_name: field(next()),
            args: field(next()),
        },
        TAG_RETURN => Frame::Return {
            corr_id,
            value: next(),
            out_params: expect_out_params(next()),
        },
        TAG_THROW => Frame::Throw {
            corr_id,
            throw: PhpThrow::from_payload_fields(fields.collect()),
        },
        TAG_RELEASE_RUST_HANDLE => Frame::ReleaseRustHandle {
            rhandle: expect_id(next()),
        },
        TAG_RELEASE_PHP_HANDLE => Frame::ReleasePhpHandle {
            phandle: expect_id(next()),
        },
        _ => panic!("PHP RPC: protocol violation — unknown frame tag {tag:#04x}"),
    }
}

fn field<T: FromPluginValue>(value: PluginValue) -> T {
    protocol_value(&value, "a frame field")
}

fn expect_id(value: PluginValue) -> u64 {
    let id: i64 = field(value);
    u64::try_from(id)
        .unwrap_or_else(|_| panic!("PHP RPC: protocol violation — negative handle id {id}"))
}

fn expect_out_params(value: PluginValue) -> IndexMap<u32, PluginValue> {
    field::<IndexMap<BString, PluginValue>>(value)
        .into_iter()
        .map(|(key, item)| {
            let position = utf8_text(&key)
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| {
                    panic!("PHP RPC: protocol violation — out param key is not a position: {key:?}")
                });
            (position, item)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(frame: Frame) -> Frame {
        let (mut a, mut b) = std::os::unix::net::UnixStream::pair().unwrap();
        write_frame(&mut a, &frame).unwrap();
        read_frame(&mut b).unwrap()
    }

    #[test]
    fn frame_roundtrip_call_function() {
        let frame = roundtrip(Frame::CallFunction {
            corr_id: 7,
            function_name: "defined".into(),
            args: vec![PluginValue::string("PHP_VERSION")],
        });
        match frame {
            Frame::CallFunction {
                corr_id,
                function_name,
                args,
            } => {
                assert_eq!(corr_id, 7);
                assert_eq!(function_name, "defined");
                assert_eq!(args, vec![PluginValue::string("PHP_VERSION")]);
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_non_utf8_names() {
        let frame = roundtrip(Frame::CallStaticMethod {
            corr_id: 3,
            pclass: b"Caf\xe9".into(),
            method_name: b"m\xff".into(),
            args: vec![],
        });
        match frame {
            Frame::CallStaticMethod {
                pclass,
                method_name,
                ..
            } => {
                assert_eq!(pclass, b"Caf\xe9".as_slice());
                assert_eq!(method_name, b"m\xff".as_slice());
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_throw_with_properties() {
        let mut throw = PhpThrow::new(
            "Composer\\Downloader\\TransportException",
            "The \"https://example.org\" file could not be downloaded",
            401,
        );
        *throw.properties = [
            ("statusCode".into(), PluginValue::Int(401)),
            ("response".into(), PluginValue::Null),
        ]
        .into_iter()
        .collect();
        throw.file = "/vendor/composer/composer/src/Composer/Util/Http/CurlDownloader.php".into();
        throw.line = 42;
        throw.parents = vec!["RuntimeException".into(), "Exception".into()];
        let frame = roundtrip(Frame::Throw { corr_id: 11, throw });
        match frame {
            Frame::Throw { corr_id, throw } => {
                assert_eq!(
                    throw.file,
                    "/vendor/composer/composer/src/Composer/Util/Http/CurlDownloader.php"
                );
                assert_eq!(throw.line, 42);
                assert_eq!(throw.parents, vec!["RuntimeException", "Exception"]);
                assert_eq!(corr_id, 11);
                assert_eq!(
                    throw.exception_class,
                    "Composer\\Downloader\\TransportException"
                );
                assert_eq!(
                    throw.message,
                    "The \"https://example.org\" file could not be downloaded"
                );
                assert_eq!(throw.code, 401);
                assert_eq!(
                    throw.properties.get(bstr::B("statusCode")),
                    Some(&PluginValue::Int(401))
                );
                assert_eq!(
                    throw.properties.get(bstr::B("response")),
                    Some(&PluginValue::Null)
                );
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
        let frame = roundtrip(Frame::ReleaseRustHandle { rhandle: 4 });
        match frame {
            Frame::ReleaseRustHandle { rhandle } => {
                assert_eq!(rhandle, 4);
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "an anyhow error raised by the port, read through core::fmt"
    )]
    #[test]
    fn read_frame_rejects_oversized_length() {
        let (mut a, mut b) = std::os::unix::net::UnixStream::pair().unwrap();
        a.write_all(&(MAX_FRAME_LEN + 1).to_le_bytes()).unwrap();
        let err = read_frame(&mut b).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("MAX_FRAME_LEN"), "{err}");
    }
}
