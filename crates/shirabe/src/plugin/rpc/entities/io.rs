//! The methods a `Composer\IO\IOInterface` entity answers over RPC.

use crate::io::IOInterface;
use crate::io::buffer_io::BufferIO;
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
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
        // Only a `Composer\IO\BufferIO` stub forwards these, and that stub is handed out for a
        // BufferIO entity alone.
        b"getOutput" => Ok(PluginValue::string(buffer_io(&*io.borrow()).get_output())),
        b"setUserInputs" => {
            let inputs = arg::<Vec<BString>>(method_name, args, 0)?;
            buffer_io(&*io.borrow())
                .set_user_inputs(inputs)
                .map_err(|error| error_throw(b"setUserInputs failed", &error))?;
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is
        // widened on demand, driven by explicit errors from real plugins. The ask* methods take
        // `InputValue` defaults and validators, which cannot hold an int or float default, a
        // non-string choice, or a validator returning any other PHP value.
        other => Err(unavailable_method("IO", other)),
    }
}

fn buffer_io(io: &dyn IOInterface) -> &BufferIO {
    io.as_any()
        .downcast_ref::<BufferIO>()
        .expect("only a BufferIO stub forwards the BufferIO methods")
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
