//! The methods a Rust-implemented `Composer\Plugin\PluginInterface` entity answers over RPC. A
//! PHP-implemented plugin never crosses this way: its entity lives in the child's P table already.

use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use crate::plugin::PluginInterface;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToPluginValue};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn PluginInterface>> {
    const EXPECTED: &'static [u8] = b"a plugin";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    // TODO(plugin): a Rust-implemented plugin that is also an EventSubscriberInterface crosses as
    // one of these two classes, so `instanceof EventSubscriberInterface` is false in the child;
    // Composer's own subscriber dispatch runs on the Rust side and never asks.
    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(match self.borrow().as_capable() {
            Some(_) => &forwarding::shirabe::rust_capable_plugin_stub::CROSSING,
            None => &forwarding::shirabe::rust_plugin_stub::CROSSING,
        })
    }
}

// TODO(plugin): the lifecycle methods would have to turn the `$composer`/`$io` stubs the child
// passes back into the Rust handles they stand for; nothing calls them, because Composer
// activates a Rust-implemented plugin on the Rust side.
impl forwarding::shirabe::rust_plugin_stub::Forwarding
    for forwarding::shirabe::rust_plugin_stub::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>;
}

impl forwarding::shirabe::rust_capable_plugin_stub::Forwarding
    for forwarding::shirabe::rust_capable_plugin_stub::PhpClass
{
    fn get_capabilities(
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let plugin = plugin.borrow();
        let capabilities = plugin
            .as_capable()
            .expect("only a Capable plugin crosses as Shirabe\\RustCapablePluginStub")
            .get_capabilities()
            .map_err(call.failed())?;
        Ok(capabilities.to_plugin_value())
    }
}
