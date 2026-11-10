//! The methods a `Composer\DependencyResolver\Operation\OperationInterface` entity answers over RPC.

use crate::dependency_resolver::operation::{
    AnyOperation, InstallOperation, MarkAliasInstalledOperation, MarkAliasUninstalledOperation,
    UninstallOperation, UpdateOperation,
};
use crate::package::{AliasPackageHandle, PackageInterfaceHandle};
use crate::php_worker::rpc::entities::package::package_value;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToPluginValue, codec};

impl Entity for std::rc::Rc<AnyOperation> {
    const EXPECTED: &'static [u8] = b"an operation";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(match &**self {
            AnyOperation::Install(_) => {
                &forwarding::dependency_resolver::operation::install_operation::CROSSING
            }
            AnyOperation::Update(_) => {
                &forwarding::dependency_resolver::operation::update_operation::CROSSING
            }
            AnyOperation::Uninstall(_) => {
                &forwarding::dependency_resolver::operation::uninstall_operation::CROSSING
            }
            AnyOperation::MarkAliasInstalled(_) => {
                &forwarding::dependency_resolver::operation::mark_alias_installed_operation::CROSSING
            }
            AnyOperation::MarkAliasUninstalled(_) => {
                &forwarding::dependency_resolver::operation::mark_alias_uninstalled_operation::CROSSING
            }
        })
    }
}

/// The alias package an alias operation is built for.
fn alias_package(call: &Call<'_>, position: usize) -> Result<AliasPackageHandle, PhpThrow> {
    call.arg::<PackageInterfaceHandle>(position)?
        .as_alias()
        .ok_or_else(|| {
            call.arg_throw(
                position,
                codec::unexpected(
                    "an AliasPackage",
                    call.args().get(position).unwrap_or(&codec::NULL),
                ),
            )
        })
}

fn construct(operation: AnyOperation) -> std::rc::Rc<AnyOperation> {
    std::rc::Rc::new(operation)
}

impl forwarding::dependency_resolver::operation::solver_operation::Forwarding
    for forwarding::dependency_resolver::operation::solver_operation::PhpClass
{
    type Handle = std::rc::Rc<AnyOperation>;

    fn get_operation_type(
        operation: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(operation.get_operation_type().to_plugin_value())
    }

    fn show(operation: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(operation.show(call.arg::<bool>(0)?).to_plugin_value())
    }

    fn __to_string(operation: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(operation.show(false).to_plugin_value())
    }
}

// A solver operation carries no state beyond the packages it names, so a plugin-built one is a
// complete instance rather than a second view on a Rust-side service.
impl forwarding::dependency_resolver::operation::install_operation::Forwarding
    for forwarding::dependency_resolver::operation::install_operation::PhpClass
{
    fn get_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::Install(operation) = &**operation else {
            unreachable!("only an InstallOperation crosses as one");
        };
        package_value(operation.get_package().as_rc())
    }

    fn __construct(call: &mut Call<'_>) -> Result<std::rc::Rc<AnyOperation>, PhpThrow> {
        Ok(construct(AnyOperation::Install(InstallOperation::new(
            call.arg(0)?,
        ))))
    }
}

impl forwarding::dependency_resolver::operation::update_operation::Forwarding
    for forwarding::dependency_resolver::operation::update_operation::PhpClass
{
    fn get_initial_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::Update(operation) = &**operation else {
            unreachable!("only an UpdateOperation crosses as one");
        };
        package_value(operation.get_initial_package().as_rc())
    }

    fn get_target_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::Update(operation) = &**operation else {
            unreachable!("only an UpdateOperation crosses as one");
        };
        package_value(operation.get_target_package().as_rc())
    }

    fn __construct(call: &mut Call<'_>) -> Result<std::rc::Rc<AnyOperation>, PhpThrow> {
        Ok(construct(AnyOperation::Update(UpdateOperation::new(
            call.arg(0)?,
            call.arg(1)?,
        ))))
    }
}

impl forwarding::dependency_resolver::operation::uninstall_operation::Forwarding
    for forwarding::dependency_resolver::operation::uninstall_operation::PhpClass
{
    fn get_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::Uninstall(operation) = &**operation else {
            unreachable!("only an UninstallOperation crosses as one");
        };
        package_value(operation.get_package().as_rc())
    }

    fn __construct(call: &mut Call<'_>) -> Result<std::rc::Rc<AnyOperation>, PhpThrow> {
        Ok(construct(AnyOperation::Uninstall(UninstallOperation::new(
            call.arg(0)?,
        ))))
    }
}

impl forwarding::dependency_resolver::operation::mark_alias_installed_operation::Forwarding
    for forwarding::dependency_resolver::operation::mark_alias_installed_operation::PhpClass
{
    fn get_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::MarkAliasInstalled(operation) = &**operation else {
            unreachable!("only a MarkAliasInstalledOperation crosses as one");
        };
        package_value(operation.get_package().as_rc())
    }

    fn __construct(call: &mut Call<'_>) -> Result<std::rc::Rc<AnyOperation>, PhpThrow> {
        Ok(construct(AnyOperation::MarkAliasInstalled(
            MarkAliasInstalledOperation::new(alias_package(call, 0)?),
        )))
    }
}

impl forwarding::dependency_resolver::operation::mark_alias_uninstalled_operation::Forwarding
    for forwarding::dependency_resolver::operation::mark_alias_uninstalled_operation::PhpClass
{
    fn get_package(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let AnyOperation::MarkAliasUninstalled(operation) = &**operation else {
            unreachable!("only a MarkAliasUninstalledOperation crosses as one");
        };
        package_value(operation.get_package().as_rc())
    }

    fn __construct(call: &mut Call<'_>) -> Result<std::rc::Rc<AnyOperation>, PhpThrow> {
        Ok(construct(AnyOperation::MarkAliasUninstalled(
            MarkAliasUninstalledOperation::new(alias_package(call, 0)?),
        )))
    }
}
