//! The methods a `Composer\IO\IOInterface` entity answers over RPC.

use crate::io::{
    __php_binding_buffer_io, __php_binding_io_interface_immutable, BufferIO, IOInterface,
    IOInterfaceImmutable,
};
use crate::php_worker::rpc::entity::Call;
use bstr::{BString, ByteSlice as _};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::{PhpThrow, RpcValue};

// TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is widened on
// demand. The ask* methods take `InputValue` defaults and validators, which cannot hold an int or
// float default, a non-string choice, or a validator returning any other PHP value.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    expected: "an IOInterface",
    class: |io| io.borrow().php_class().map(|class| class.as_bytes()).ok_or_else(|| {
        // TODO(plugin): only IO implementations with a generated proxy stub can cross the
        // boundary; the rest are an explicit error until stubs of their own are generated.
        PhpThrow::runtime("no proxy stub class is available for this IO implementation")
    }),
    classes: [
        r"Composer\IO\BaseIO",
        r"Composer\IO\ConsoleIO",
        r"Composer\IO\NullIO",
        r"Composer\IO\BufferIO",
    ],
    types: {
        r"Composer\IO\IOInterface": custom messages only [write, writeError],
        r"Composer\IO\IOInterface": instance __php_binding_io_interface_immutable
            for dyn IOInterfaceImmutable {
            via: |io| std::cell::Ref::map(io.borrow(), |io| {
                io as &(dyn IOInterfaceImmutable + 'static)
            }),
        },
        r"Composer\IO\BufferIO": instance __php_binding_buffer_io for BufferIO {
            via: |io| std::cell::Ref::map(io.borrow(), |io| {
                io.as_any()
                    .downcast_ref::<BufferIO>()
                    .expect("only a BufferIO crosses as Composer\\IO\\BufferIO")
            }),
        },
    },
    constructors: {
        r"Composer\IO\BufferIO": custom construct_buffer_io,
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
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        let io = io.borrow();
        for message in &messages {
            io.write_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(RpcValue::Null)
    }

    pub(super) fn write_error(
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        let io = io.borrow();
        for message in &messages {
            io.write_error_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(RpcValue::Null)
    }
}

// A BufferIO writes into a memory stream of its own and reads from its own string input, so a
// plugin-built one is a complete instance rather than a second view on the graph's IO.
fn construct_buffer_io(
    call: &mut Call<'_>,
) -> Result<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>, PhpThrow> {
    // TODO(plugin): an OutputFormatterInterface is a Symfony object the plugin builds in its
    // own process, and no proxy carries one to the Rust side.
    if !matches!(call.args().get(2), None | Some(RpcValue::Null)) {
        return Err(PhpThrow::runtime(
            "Shirabe does not support passing an output formatter to Composer\\IO\\BufferIO inside the plugin process yet",
        ));
    }
    let io =
        BufferIO::new(call.arg::<BString>(0)?, call.arg::<i64>(1)?, None).map_err(call.failed())?;
    Ok(std::rc::Rc::new(std::cell::RefCell::new(io)))
}
