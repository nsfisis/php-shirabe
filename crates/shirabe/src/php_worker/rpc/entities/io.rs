//! The methods a `Composer\IO\IOInterface` entity answers over RPC.

use crate::io::{__php_binding_buffer_io, __php_binding_io_interface, BufferIO, IOInterface};
use crate::php_worker::rpc::entity::Call;
use bstr::{BString, ByteSlice as _};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue};

// TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is widened on
// demand. The ask* methods take `InputValue` defaults and validators, which cannot hold an int or
// float default, a non-string choice, or a validator returning any other PHP value.
php_entity! {
    handle: std::rc::Rc<dyn IOInterface>,
    type_error: "an IOInterface",
    class_of: |io| io.php_class().map(|class| class.as_bytes()).ok_or_else(|| {
        // TODO(plugin): only IO implementations with a generated proxy stub can cross the
        // boundary; the rest are an explicit error until stubs of their own are generated.
        PhpThrow::runtime("no proxy stub class is available for this IO implementation")
    }),
    classes: {
        r"Composer\IO\BaseIO": { ancestor: true },
        r"Composer\IO\ConsoleIO": { constructor: unimplemented },
        r"Composer\IO\NullIO": { constructor: unimplemented },
        r"Composer\IO\BufferIO": { constructor: { handwritten: construct_buffer_io } },
    },
    methods: {
        r"Composer\IO\IOInterface": {
            handwritten: { module: messages, members: [write, writeError] },
            instance: {
                adapters: __php_binding_io_interface,
                target: dyn IOInterface,
                deref: |io| &**io,
            },
            unimplemented: [
                ask,
                askAndHideAnswer,
                askAndValidate,
                askConfirmation,
                getAuthentication,
                getAuthentications,
                hasAuthentication,
                loadConfiguration,
                overwrite,
                overwriteError,
                select,
                setAuthentication,
                writeErrorRaw,
                writeRaw,
            ],
        },
        r"Composer\IO\BaseIO": {
            unimplemented: [
                alert,
                critical,
                debug,
                emergency,
                error,
                info,
                log,
                notice,
                resetAuthentications,
                warning,
            ],
        },
        r"Composer\IO\ConsoleIO": {
            unimplemented: [enableDebugging, getProgressBar, getTable],
        },
        r"Composer\IO\BufferIO": {
            instance: {
                adapters: __php_binding_buffer_io,
                target: BufferIO,
                deref: |io| {
                    io.as_any()
                        .downcast_ref::<BufferIO>()
                        .expect("only a BufferIO crosses as Composer\\IO\\BufferIO")
                },
            },
        },
    },
}

/// `write()` and `writeError()`, whose `$messages` is a string or a list of strings in PHP.
mod messages {
    use crate::io::IOInterface;
    use crate::php_worker::rpc::entity::Call;
    use bstr::{BString, ByteSlice as _};
    use shirabe_php_rpc::{PhpThrow, RpcValue};

    /// Decodes `($messages, bool $newline, int $verbosity)`.
    fn write_args(call: &Call<'_>) -> Result<(Vec<BString>, bool, i64), PhpThrow> {
        let messages = match call.args().first() {
            Some(RpcValue::String(_)) => vec![call.arg::<BString>(0)?],
            _ => call.arg::<Vec<BString>>(0)?,
        };
        Ok((messages, call.arg::<bool>(1)?, call.arg::<i64>(2)?))
    }

    pub(super) fn write(
        io: &std::rc::Rc<dyn IOInterface>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        for message in &messages {
            io.write_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(RpcValue::Null)
    }

    pub(super) fn write_error(
        io: &std::rc::Rc<dyn IOInterface>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        for message in &messages {
            io.write_error_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(RpcValue::Null)
    }
}

// A BufferIO writes into a memory stream of its own and reads from its own string input, so a
// plugin-built one is a complete instance rather than a second view on the graph's IO.
fn construct_buffer_io(call: &mut Call<'_>) -> Result<std::rc::Rc<dyn IOInterface>, PhpThrow> {
    // TODO(plugin): an OutputFormatterInterface is a Symfony object the plugin builds in its
    // own process, and no proxy carries one to the Rust side.
    if !matches!(call.args().get(2), None | Some(RpcValue::Null)) {
        return Err(PhpThrow::runtime(
            "Shirabe does not support passing an output formatter to Composer\\IO\\BufferIO inside the plugin process yet",
        ));
    }
    let io =
        BufferIO::new(call.arg::<BString>(0)?, call.arg::<i64>(1)?, None).map_err(call.failed())?;
    Ok(std::rc::Rc::new(io))
}
