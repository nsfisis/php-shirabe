//! The methods a `Composer\IO\IOInterface` entity answers over RPC.

use crate::io::{BufferIO, ConsoleIO, IOInterface, NORMAL, NullIO};
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use bstr::{BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_symfony_console::output::VERBOSITY_NORMAL;

impl Entity for std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    const EXPECTED: &'static [u8] = b"an IOInterface";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        let borrowed = self.borrow();
        let any = borrowed.as_any();
        if any.is::<BufferIO>() {
            Ok(&forwarding::io::buffer_io::CROSSING)
        } else if any.is::<ConsoleIO>() {
            Ok(&forwarding::io::console_io::CROSSING)
        } else if any.is::<NullIO>() {
            Ok(&forwarding::io::null_io::CROSSING)
        } else {
            // TODO(plugin): only IO implementations with a generated proxy stub can cross the
            // boundary; the rest are an explicit error until stubs of their own are generated.
            Err(PhpThrow::runtime(
                "no proxy stub class is available for this IO implementation",
            ))
        }
    }
}

/// Decodes `($messages, bool $newline, int $verbosity)`: `$messages` is a string or a list of
/// strings in PHP.
fn write_args(call: &Call<'_>) -> Result<(Vec<BString>, bool, i64), PhpThrow> {
    let messages = match call.args().first() {
        Some(PluginValue::String(_)) => vec![call.arg::<BString>(0)?],
        _ => call.arg::<Vec<BString>>(0)?,
    };
    Ok((messages, call.arg_or(1, true)?, call.arg_or(2, NORMAL)?))
}

// TODO(plugin): the remaining IOInterface surface (ask*, authentications, ...) is widened on
// demand. The ask* methods take `InputValue` defaults and validators, which cannot hold an int or
// float default, a non-string choice, or a validator returning any other PHP value.
impl forwarding::io::base_io::Forwarding for forwarding::io::base_io::PhpClass {
    type Handle = std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;

    fn is_interactive(io: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(io.borrow().is_interactive().to_wire())
    }

    fn is_verbose(io: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(io.borrow().is_verbose().to_wire())
    }

    fn is_very_verbose(io: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(io.borrow().is_very_verbose().to_wire())
    }

    fn is_debug(io: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(io.borrow().is_debug().to_wire())
    }

    fn is_decorated(io: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(io.borrow().is_decorated().to_wire())
    }

    fn write(io: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        let io = io.borrow();
        for message in &messages {
            io.write_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(PluginValue::Null)
    }

    fn write_error(io: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (messages, newline, verbosity) = write_args(call)?;
        let io = io.borrow();
        for message in &messages {
            io.write_error_bytes(message.as_bstr(), newline, verbosity);
        }
        Ok(PluginValue::Null)
    }
}

impl forwarding::io::console_io::Forwarding for forwarding::io::console_io::PhpClass {}

impl forwarding::io::null_io::Forwarding for forwarding::io::null_io::PhpClass {}

fn buffer_io(io: &dyn IOInterface) -> &BufferIO {
    io.as_any()
        .downcast_ref::<BufferIO>()
        .expect("only a BufferIO crosses as Composer\\IO\\BufferIO")
}

impl forwarding::io::buffer_io::Forwarding for forwarding::io::buffer_io::PhpClass {
    fn get_output(
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(PluginValue::string(buffer_io(&*io.borrow()).get_output()))
    }

    fn set_user_inputs(
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let inputs = call.arg::<Vec<BString>>(0)?;
        buffer_io(&*io.borrow())
            .set_user_inputs(inputs)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    // A BufferIO writes into a memory stream of its own and reads from its own string input, so a
    // plugin-built one is a complete instance rather than a second view on the graph's IO.
    fn __construct(
        call: &mut Call<'_>,
    ) -> Result<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>, PhpThrow> {
        // TODO(plugin): an OutputFormatterInterface is a Symfony object the plugin builds in its
        // own process, and no proxy carries one to the Rust side.
        if !matches!(call.args().get(2), None | Some(PluginValue::Null)) {
            return Err(PhpThrow::runtime(
                "Shirabe does not support passing an output formatter to Composer\\IO\\BufferIO inside the plugin process yet",
            ));
        }
        let io = BufferIO::new(
            call.arg_or(0, BString::default())?,
            call.arg_or(1, VERBOSITY_NORMAL)?,
            None,
        )
        .map_err(call.failed())?;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(io)))
    }
}
