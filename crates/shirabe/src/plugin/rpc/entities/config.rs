//! The methods a `Composer\Config` entity answers over RPC.

use crate::config::Config;
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::PhpMixed;

pub(crate) fn dispatch_config_method(
    config: &std::rc::Rc<std::cell::RefCell<Config>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let key = |position: usize| arg::<BString>(method_name, args, position);
    let flags = |position: usize| arg_or::<i64>(method_name, args, position, 0);
    match method_name.as_bytes() {
        b"get" => {
            let value = config
                .borrow()
                .get_with_flags(&key(0)?, flags(1)?)
                .map_err(|error| error_throw(b"get failed", &error))?;
            Ok(value.to_wire())
        }
        b"all" => {
            let all = config
                .borrow()
                .all(flags(0)?)
                .map_err(|error| error_throw(b"all failed", &error))?;
            Ok(all.to_wire())
        }
        b"raw" => Ok(config.borrow().raw().to_wire()),
        b"has" => Ok(config.borrow().has(&key(0)?).to_wire()),
        b"getRepositories" => Ok(config.borrow().get_repositories().to_wire()),
        b"getSourceOfValue" => {
            let source = config
                .borrow()
                .get_source_of_value(&key(0)?)
                .map_err(|error| error_throw(b"getSourceOfValue failed", &error))?;
            Ok(source.to_wire())
        }
        b"merge" => {
            let values = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            let source =
                arg_or::<BString>(method_name, args, 1, Config::SOURCE_UNKNOWN.to_owned())?;
            config.borrow_mut().merge(&values, &source);
            Ok(PluginValue::Null)
        }
        // TODO(plugin): the config-source surface (getConfigSource / setAuthConfigSource / ...)
        // needs proxy stubs for ConfigSourceInterface implementations, and prohibitUrlByConfig
        // needs the IO argument decoded back into the Rust-side instance; both are widened on
        // demand, driven by explicit errors from real plugins.
        other => Err(unavailable_method("Config", other)),
    }
}
