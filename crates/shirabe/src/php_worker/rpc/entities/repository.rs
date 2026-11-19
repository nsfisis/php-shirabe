//! The methods a `Composer\Repository\RepositoryInterface` entity answers over RPC.

use crate::package::PackageInterfaceHandle;
use crate::php_worker::codec::constraint_from_rpc_value;
use crate::php_worker::rpc::entities::package::package_value;
use crate::php_worker::rpc::entity::Call;
use crate::repository::{
    FindPackageConstraint, InstalledArrayRepository, InstalledFilesystemRepository,
    LockArrayRepository, RepositoryInterfaceHandle, WritableRepositoryInterface,
};
use bstr::{BString, ByteSlice as _};
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
use shirabe_php_shim::bformat;

// TODO(plugin): the remaining RepositoryInterface surface is widened on demand.
php_entity! {
    handle: RepositoryInterfaceHandle,
    type_error: "a repository",
    identity: |repository| repository.ptr_id() as *const (),
    class_of: |repository| {
        if repository.is::<InstalledFilesystemRepository>() {
            Ok(br"Composer\Repository\InstalledFilesystemRepository")
        } else if repository.is::<InstalledArrayRepository>() {
            Ok(br"Composer\Repository\InstalledArrayRepository")
        } else if repository.is::<LockArrayRepository>() {
            Ok(br"Composer\Repository\LockArrayRepository")
        } else {
            // TODO(plugin): the remaining repository classes get stubs on demand.
            Err(PhpThrow::runtime(
                "no proxy stub class is available for this repository over RPC yet",
            ))
        }
    },
    classes: {
        r"Composer\Repository\ArrayRepository": { ancestor: true, constructor: unimplemented },
        r"Composer\Repository\WritableArrayRepository": {
            ancestor: true,
            constructor: unimplemented,
        },
        r"Composer\Repository\FilesystemRepository": {
            ancestor: true,
            constructor: unimplemented,
        },
        r"Composer\Repository\InstalledArrayRepository": { constructor: unimplemented },
        r"Composer\Repository\InstalledFilesystemRepository": { constructor: unimplemented },
        r"Composer\Repository\LockArrayRepository": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Repository\RepositoryInterface": {
            handwritten: { module: self },
            unimplemented: [getProviders, getRepoName, loadPackages, search],
        },
        r"Composer\Repository\ArrayRepository": {
            handwritten: { module: self },
            unimplemented: [count],
        },
        r"Composer\Repository\WritableRepositoryInterface": {
            handwritten: { module: self },
            unimplemented: [reload, setDevPackageNames, write],
        },
        r"Composer\Repository\WritableArrayRepository": {
            unimplemented: [getDevMode],
        },
        r"Composer\Repository\InstalledRepositoryInterface": {
            unimplemented: [getDevMode, isFresh],
        },
    },
}

/// `string|ConstraintInterface|null`, the constraint `findPackage`/`findPackages` take.
fn find_package_constraint(
    call: &Call<'_>,
    position: usize,
) -> Result<Option<FindPackageConstraint>, PhpThrow> {
    match call.args().get(position) {
        None | Some(RpcValue::Null) => Ok(None),
        Some(RpcValue::String(constraint)) => {
            Ok(Some(FindPackageConstraint::String(constraint.clone())))
        }
        Some(value) => constraint_from_rpc_value(value)
            .map(|constraint| Some(FindPackageConstraint::Constraint(constraint)))
            .map_err(|throw| call.arg_throw(position, throw)),
    }
}

/// Runs `f` on the writable repository behind `repository`. `ArrayRepository` declares
/// `addPackage()`/`removePackage()` itself, but the Rust model gives only the writable
/// repositories a way to add one.
fn with_writable<T>(
    repository: &RepositoryInterfaceHandle,
    call: &Call<'_>,
    f: impl FnOnce(&mut dyn WritableRepositoryInterface) -> T,
) -> Result<T, PhpThrow> {
    let mut borrowed = repository.borrow_mut();
    let writable = borrowed
        .as_writable_repository_interface_mut()
        .ok_or_else(|| {
            PhpThrow::runtime(bformat!(
                "the repository behind this handle is not writable, so `{}` cannot be called on it",
                call.method()
            ))
        })?;
    Ok(f(writable))
}

fn has_package(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let package = call.arg::<PackageInterfaceHandle>(0)?;
    let has = repository.has_package(package).map_err(call.failed())?;
    Ok(has.to_rpc_value())
}

fn add_package(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let package = call.arg::<PackageInterfaceHandle>(0)?;
    with_writable(repository, call, |writable| writable.add_package(package))?
        .map_err(call.failed())?;
    Ok(RpcValue::Null)
}

fn remove_package(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let package = call.arg::<PackageInterfaceHandle>(0)?;
    with_writable(repository, call, |writable| {
        writable.remove_package(package)
    })?
    .map_err(call.failed())?;
    Ok(RpcValue::Null)
}

fn get_packages(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let packages = repository
        .borrow_mut()
        .get_packages()
        .map_err(call.failed())?;
    Ok(RpcValue::List(
        packages
            .iter()
            .map(|package| package_value(package.as_rc()))
            .collect::<Result<_, _>>()?,
    ))
}

fn find_package(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let name = call.arg::<BString>(0)?;
    let constraint = find_package_constraint(call, 1)?.ok_or_else(|| {
        call.arg_throw(
            1,
            DecodeError::new("expected a constraint string or object, got null"),
        )
    })?;
    let package = repository
        .borrow_mut()
        .find_package(name.as_bstr(), constraint)
        .map_err(call.failed())?;
    match package {
        Some(package) => package_value(package.as_rc()),
        None => Ok(RpcValue::Null),
    }
}

fn find_packages(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let name = call.arg::<BString>(0)?;
    let constraint = find_package_constraint(call, 1)?;
    let packages = repository
        .borrow_mut()
        .find_packages(name.as_bstr(), constraint)
        .map_err(call.failed())?;
    Ok(RpcValue::List(
        packages
            .iter()
            .map(|package| package_value(package.as_rc()))
            .collect::<Result<_, _>>()?,
    ))
}

fn get_canonical_packages(
    repository: &RepositoryInterfaceHandle,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let mut borrowed = repository.borrow_mut();
    let packages = borrowed
        .as_writable_repository_interface_mut()
        .expect("only a writable repository crosses as a WritableArrayRepository")
        .get_canonical_packages()
        .map_err(call.failed())?;
    Ok(packages.to_rpc_value())
}

fn get_dev_package_names(
    repository: &RepositoryInterfaceHandle,
    _: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let mut borrowed = repository.borrow_mut();
    Ok(borrowed
        .as_writable_repository_interface_mut()
        .expect("only a writable repository crosses as a WritableArrayRepository")
        .get_dev_package_names()
        .to_rpc_value())
}
