//! The methods a `Composer\DependencyResolver\Operation\OperationInterface` entity answers over RPC.

use crate::dependency_resolver::operation::{
    __php_binding_any_operation, __php_binding_install_operation,
    __php_binding_mark_alias_installed_operation, __php_binding_mark_alias_uninstalled_operation,
    __php_binding_uninstall_operation, __php_binding_update_operation, AnyOperation,
    InstallOperation, MarkAliasInstalledOperation, MarkAliasUninstalledOperation,
    UninstallOperation, UpdateOperation,
};
use shirabe_php_binding_macros::php_entity;

// A solver operation carries no state beyond the packages it names, so a plugin-built one is a
// complete instance rather than a second view on a Rust-side service.
php_entity! {
    handle: std::rc::Rc<AnyOperation>,
    type_error: "an operation",
    class_of: |operation| Ok(match &**operation {
        AnyOperation::Install(_) => br"Composer\DependencyResolver\Operation\InstallOperation",
        AnyOperation::Update(_) => br"Composer\DependencyResolver\Operation\UpdateOperation",
        AnyOperation::Uninstall(_) => {
            br"Composer\DependencyResolver\Operation\UninstallOperation"
        }
        AnyOperation::MarkAliasInstalled(_) => {
            br"Composer\DependencyResolver\Operation\MarkAliasInstalledOperation"
        }
        AnyOperation::MarkAliasUninstalled(_) => {
            br"Composer\DependencyResolver\Operation\MarkAliasUninstalledOperation"
        }
    }),
    classes: {
        r"Composer\DependencyResolver\Operation\SolverOperation": { ancestor: true },
        r"Composer\DependencyResolver\Operation\InstallOperation": {
            constructor: {
                adapters: __php_binding_install_operation,
                into: |operation| std::rc::Rc::new(AnyOperation::Install(operation)),
            },
        },
        r"Composer\DependencyResolver\Operation\UpdateOperation": {
            constructor: {
                adapters: __php_binding_update_operation,
                into: |operation| std::rc::Rc::new(AnyOperation::Update(operation)),
            },
        },
        r"Composer\DependencyResolver\Operation\UninstallOperation": {
            constructor: {
                adapters: __php_binding_uninstall_operation,
                into: |operation| std::rc::Rc::new(AnyOperation::Uninstall(operation)),
            },
        },
        r"Composer\DependencyResolver\Operation\MarkAliasInstalledOperation": {
            constructor: {
                adapters: __php_binding_mark_alias_installed_operation,
                into: |operation| std::rc::Rc::new(AnyOperation::MarkAliasInstalled(operation)),
            },
        },
        r"Composer\DependencyResolver\Operation\MarkAliasUninstalledOperation": {
            constructor: {
                adapters: __php_binding_mark_alias_uninstalled_operation,
                into: |operation| std::rc::Rc::new(AnyOperation::MarkAliasUninstalled(operation)),
            },
        },
    },
    methods: {
        r"Composer\DependencyResolver\Operation\OperationInterface": {
            handwritten: { module: solver_operation, members: [show, __toString] },
            instance: {
                adapters: __php_binding_any_operation,
                target: AnyOperation,
                deref: |operation| &**operation,
            },
        },
        r"Composer\DependencyResolver\Operation\InstallOperation": {
            instance: {
                adapters: __php_binding_install_operation,
                target: InstallOperation,
                deref: |operation| match &**operation {
                    AnyOperation::Install(operation) => operation,
                    _ => unreachable!("only an InstallOperation crosses as one"),
                },
            },
        },
        r"Composer\DependencyResolver\Operation\UpdateOperation": {
            instance: {
                adapters: __php_binding_update_operation,
                target: UpdateOperation,
                deref: |operation| match &**operation {
                    AnyOperation::Update(operation) => operation,
                    _ => unreachable!("only an UpdateOperation crosses as one"),
                },
            },
        },
        r"Composer\DependencyResolver\Operation\UninstallOperation": {
            instance: {
                adapters: __php_binding_uninstall_operation,
                target: UninstallOperation,
                deref: |operation| match &**operation {
                    AnyOperation::Uninstall(operation) => operation,
                    _ => unreachable!("only an UninstallOperation crosses as one"),
                },
            },
        },
        r"Composer\DependencyResolver\Operation\MarkAliasInstalledOperation": {
            instance: {
                adapters: __php_binding_mark_alias_installed_operation,
                target: MarkAliasInstalledOperation,
                deref: |operation| match &**operation {
                    AnyOperation::MarkAliasInstalled(operation) => operation,
                    _ => unreachable!("only a MarkAliasInstalledOperation crosses as one"),
                },
            },
        },
        r"Composer\DependencyResolver\Operation\MarkAliasUninstalledOperation": {
            instance: {
                adapters: __php_binding_mark_alias_uninstalled_operation,
                target: MarkAliasUninstalledOperation,
                deref: |operation| match &**operation {
                    AnyOperation::MarkAliasUninstalled(operation) => operation,
                    _ => unreachable!("only a MarkAliasUninstalledOperation crosses as one"),
                },
            },
        },
    },
}

/// The members of `SolverOperation` its subclasses redeclare with a wider signature.
mod solver_operation {
    use crate::dependency_resolver::operation::AnyOperation;
    use crate::php_worker::rpc::entity::Call;
    use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};

    // TODO(type-model): the subclasses take `$lock` untyped, but only a bool decodes here.
    pub(super) fn show(
        operation: &std::rc::Rc<AnyOperation>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(operation.show(call.arg::<bool>(0)?).to_rpc_value())
    }

    pub(super) fn __to_string(
        operation: &std::rc::Rc<AnyOperation>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(operation.show(false).to_rpc_value())
    }
}
