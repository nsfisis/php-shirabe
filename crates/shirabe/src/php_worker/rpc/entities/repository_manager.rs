//! The methods a `Composer\Repository\RepositoryManager` entity answers over RPC.

use crate::php_worker::rpc::dispatch::unavailable_method;
use crate::php_worker::rpc::table::repository_handle_value;
use crate::repository::RepositoryManagerInterface;
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue};

pub(crate) fn dispatch_repository_manager_method(
    rm: &std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getLocalRepository" => repository_handle_value(&rm.borrow().get_local_repository()),
        // TODO(plugin): the remaining RepositoryManager surface is widened on demand, driven
        // by explicit errors from real plugins.
        other => Err(unavailable_method("RepositoryManager", other)),
    }
}
