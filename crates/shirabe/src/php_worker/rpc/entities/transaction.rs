//! The methods a `Composer\DependencyResolver\Transaction` entity answers over RPC.

use crate::dependency_resolver::AnyTransaction;
use crate::php_worker::rpc::dispatch::unavailable_method;
use crate::php_worker::rpc::table::operation_handle_value;
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue};

pub(crate) fn dispatch_transaction_method(
    transaction: &AnyTransaction,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getOperations" => Ok(PluginValue::List(
            transaction
                .get_operations()
                .iter()
                .map(operation_handle_value)
                .collect(),
        )),
        other => Err(unavailable_method("Transaction", other)),
    }
}
