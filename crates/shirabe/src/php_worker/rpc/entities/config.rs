//! The methods a `Composer\Config` entity answers over RPC.

use crate::config::Config;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::PhpMixed;

impl Entity for std::rc::Rc<std::cell::RefCell<Config>> {
    const EXPECTED: &'static [u8] = b"a Config";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::config::CROSSING)
    }
}

// TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...) needs
// proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig needs the IO
// argument decoded back into the Rust-side instance.
impl forwarding::config::Forwarding for forwarding::config::PhpClass {
    type Handle = std::rc::Rc<std::cell::RefCell<Config>>;

    fn get(config: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let value = config
            .borrow()
            .get_with_flags(&call.arg::<BString>(0)?, call.arg_or(1, 0)?)
            .map_err(call.failed())?;
        Ok(value.to_wire())
    }

    fn all(config: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let all = config
            .borrow()
            .all(call.arg_or(0, 0)?)
            .map_err(call.failed())?;
        Ok(all.to_wire())
    }

    fn raw(config: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(config.borrow().raw().to_wire())
    }

    fn has(config: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(config.borrow().has(&call.arg::<BString>(0)?).to_wire())
    }

    fn get_repositories(config: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(config.borrow().get_repositories().to_wire())
    }

    fn get_source_of_value(
        config: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let source = config
            .borrow()
            .get_source_of_value(&call.arg::<BString>(0)?)
            .map_err(call.failed())?;
        Ok(source.to_wire())
    }

    fn merge(config: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let values = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
        let source = call.arg_or::<BString>(1, Config::SOURCE_UNKNOWN.to_owned())?;
        config.borrow_mut().merge(&values, &source);
        Ok(PluginValue::Null)
    }
}
