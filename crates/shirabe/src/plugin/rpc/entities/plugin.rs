//! The methods a Rust-implemented `Composer\Plugin\PluginInterface` entity answers over RPC.

use crate::plugin::plugin_interface::PluginInterface;
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::bconcat;

pub(crate) fn dispatch_plugin_method(
    plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getCapabilities" => {
            let plugin = plugin.borrow();
            let capable = plugin.as_capable().ok_or_else(|| {
                PhpThrow::runtime(bconcat!(
                    "plugin ",
                    plugin.get_class_name(),
                    " does not implement Capable"
                ))
            })?;
            let capabilities = capable
                .get_capabilities()
                .map_err(|error| error_throw(b"getCapabilities failed", &error))?;
            Ok(capabilities.to_wire())
        }
        // TODO(plugin): the lifecycle methods would have to turn the `$composer`/`$io` stubs the
        // child passes back into the Rust handles they stand for; nothing calls them, because
        // Composer activates a Rust-implemented plugin on the Rust side.
        other => Err(unavailable_method("plugin", other)),
    }
}
