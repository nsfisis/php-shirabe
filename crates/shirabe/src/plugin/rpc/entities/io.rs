//! The methods a `Composer\IO\IOInterface` entity answers over RPC.

use crate::io::IOInterface;
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::dispatch::unavailable_method;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};

pub(crate) fn dispatch_io_method(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"write" | b"writeError" => {
            let (messages, newline, verbosity) = decode_write_args(method_name, args)?;
            let borrowed = io.borrow();
            for message in &messages {
                match method_name.as_bytes() {
                    b"write" => borrowed.write_bytes(message.as_bstr(), newline, verbosity),
                    b"writeError" => {
                        borrowed.write_error_bytes(message.as_bstr(), newline, verbosity)
                    }
                    _ => unreachable!(),
                }
            }
            Ok(PluginValue::Null)
        }
        b"isInteractive" => Ok(io.borrow().is_interactive().to_wire()),
        b"isVerbose" => Ok(io.borrow().is_verbose().to_wire()),
        b"isVeryVerbose" => Ok(io.borrow().is_very_verbose().to_wire()),
        b"isDebug" => Ok(io.borrow().is_debug().to_wire()),
        b"isDecorated" => Ok(io.borrow().is_decorated().to_wire()),
        // TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is
        // widened on demand, driven by explicit errors from real plugins.
        other => Err(unavailable_method("IO", other)),
    }
}

/// Decodes `($messages, bool $newline, int $verbosity)`: `$messages` is a string or a list of
/// strings in PHP.
fn decode_write_args(
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<(Vec<BString>, bool, i64), PhpThrow> {
    let messages = match args.first() {
        Some(PluginValue::String(_)) => vec![arg::<BString>(method_name, args, 0)?],
        _ => arg::<Vec<BString>>(method_name, args, 0)?,
    };
    let newline = arg_or::<bool>(method_name, args, 1, true)?;
    let verbosity = arg_or::<i64>(method_name, args, 2, crate::io::NORMAL)?;
    Ok((messages, newline, verbosity))
}
