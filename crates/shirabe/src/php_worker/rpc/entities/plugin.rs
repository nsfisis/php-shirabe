//! The methods a Rust-implemented `Composer\Plugin\PluginInterface` entity answers over RPC. A
//! PHP-implemented plugin never crosses this way: its entity lives in the child's PHP entity table
//! already.

use crate::plugin::PluginInterface;
use shirabe_php_binding_macros::php_entity;

// TODO(plugin): a Rust-implemented plugin that is also an EventSubscriberInterface crosses as one
// of these two classes, so `instanceof EventSubscriberInterface` is false in the child; Composer's
// own subscriber dispatch runs on the Rust side and never asks.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    expected: "a plugin",
    class: |plugin| Ok(match plugin.borrow().as_capable() {
        Some(_) => br"Shirabe\RustCapablePluginStub",
        None => br"Shirabe\RustPluginStub",
    }),
    classes: [r"Shirabe\RustPluginStub", r"Shirabe\RustCapablePluginStub"],
    types: {
        r"Composer\Plugin\PluginInterface": custom lifecycle,
        r"Composer\Plugin\Capable": custom capable,
    },
}

// TODO(plugin): the lifecycle methods would have to turn the `$composer`/`$io` stubs the child
// passes back into the Rust entities they stand for; nothing calls them, because Composer
// activates a Rust-implemented plugin on the Rust side.
mod lifecycle {
    use crate::php_worker::rpc::entity::Call;
    use crate::plugin::PluginInterface;
    use shirabe_php_rpc::{PhpThrow, RpcValue};

    pub(super) fn activate(
        _: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(call.unsupported())
    }

    pub(super) fn deactivate(
        _: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(call.unsupported())
    }

    pub(super) fn uninstall(
        _: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Err(call.unsupported())
    }
}

mod capable {
    use crate::php_worker::rpc::entity::Call;
    use crate::plugin::PluginInterface;
    use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};

    pub(super) fn get_capabilities(
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let plugin = plugin.borrow();
        let capabilities = plugin
            .as_capable()
            .expect("only a Capable plugin crosses as Shirabe\\RustCapablePluginStub")
            .get_capabilities()
            .map_err(call.failed())?;
        Ok(capabilities.to_rpc_value())
    }
}
