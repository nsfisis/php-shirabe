//! The methods a `Composer\DependencyResolver\Operation\OperationInterface` entity answers over RPC.

use crate::dependency_resolver::operation::AnyOperation;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::table::{operation_stub_class, package_handle_value};
use bstr::{BStr, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::bconcat;

pub(crate) fn dispatch_operation_method(
    operation: &AnyOperation,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match (method_name.as_bytes(), operation) {
        (b"getOperationType", _) => Ok(operation.get_operation_type().to_wire()),
        (b"show", _) => Ok(operation.show(arg::<bool>(method_name, args, 0)?).to_wire()),
        (b"__toString", _) => Ok(operation.show(false).to_wire()),
        (b"getPackage", AnyOperation::Install(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::Uninstall(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::MarkAliasInstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getPackage", AnyOperation::MarkAliasUninstalled(op)) => {
            Ok(package_handle_value(op.get_package().as_rc()))
        }
        (b"getInitialPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_initial_package().as_rc()))
        }
        (b"getTargetPackage", AnyOperation::Update(op)) => {
            Ok(package_handle_value(op.get_target_package().as_rc()))
        }
        (other, _) => Err(PhpThrow::runtime(bconcat!(
            "the operation method `",
            other,
            "` is not available on a ",
            operation_stub_class(operation),
            " over RPC yet"
        ))),
    }
}
