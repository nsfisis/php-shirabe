//! The methods a `Composer\Repository\RepositoryInterface` entity answers over RPC.

use crate::package::PackageInterfaceHandle;
use crate::php_worker::codec::constraint_from_wire;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use crate::php_worker::rpc::table::package_handle_value;
use crate::repository::{
    FindPackageConstraint, InstalledArrayRepository, InstalledFilesystemRepository,
    LockArrayRepository, RepositoryInterfaceHandle, WritableRepositoryInterface,
};
use bstr::{BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::bformat;

impl Entity for RepositoryInterfaceHandle {
    const EXPECTED: &'static [u8] = b"a repository";

    fn address(&self) -> *const () {
        self.ptr_id() as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        if self.is::<InstalledFilesystemRepository>() {
            Ok(&forwarding::repository::installed_filesystem_repository::CROSSING)
        } else if self.is::<InstalledArrayRepository>() {
            Ok(&forwarding::repository::installed_array_repository::CROSSING)
        } else if self.is::<LockArrayRepository>() {
            Ok(&forwarding::repository::lock_array_repository::CROSSING)
        } else {
            // TODO(plugin): the remaining repository classes get stubs on demand.
            Err(PhpThrow::runtime(
                "no proxy stub class is available for this repository over RPC yet",
            ))
        }
    }
}

/// `string|ConstraintInterface|null`, the constraint `findPackage`/`findPackages` take.
fn find_package_constraint(
    call: &Call<'_>,
    position: usize,
) -> Result<Option<FindPackageConstraint>, PhpThrow> {
    match call.args().get(position) {
        None | Some(PluginValue::Null) => Ok(None),
        Some(PluginValue::String(constraint)) => {
            Ok(Some(FindPackageConstraint::String(constraint.clone())))
        }
        Some(value) => constraint_from_wire(value)
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

// TODO(plugin): the remaining RepositoryInterface surface is widened on demand.
impl forwarding::repository::array_repository::Forwarding
    for forwarding::repository::array_repository::PhpClass
{
    type Handle = RepositoryInterfaceHandle;

    fn has_package(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let package = call.arg::<PackageInterfaceHandle>(0)?;
        let has = repository.has_package(package).map_err(call.failed())?;
        Ok(has.to_wire())
    }

    fn add_package(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let package = call.arg::<PackageInterfaceHandle>(0)?;
        with_writable(repository, call, |writable| writable.add_package(package))?
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn remove_package(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let package = call.arg::<PackageInterfaceHandle>(0)?;
        with_writable(repository, call, |writable| {
            writable.remove_package(package)
        })?
        .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn get_packages(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let packages = repository
            .borrow_mut()
            .get_packages()
            .map_err(call.failed())?;
        Ok(PluginValue::List(
            packages
                .iter()
                .map(|package| package_handle_value(package.as_rc()))
                .collect(),
        ))
    }

    fn find_package(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let name = call.arg::<BString>(0)?;
        let constraint = find_package_constraint(call, 1)?.ok_or_else(|| {
            call.arg_throw(
                1,
                PhpThrow::runtime("expected a constraint string or object, got null"),
            )
        })?;
        let package = repository
            .borrow_mut()
            .find_package(name.as_bstr(), constraint)
            .map_err(call.failed())?;
        Ok(package.map_or(PluginValue::Null, |package| {
            package_handle_value(package.as_rc())
        }))
    }

    fn find_packages(
        repository: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let name = call.arg::<BString>(0)?;
        let constraint = find_package_constraint(call, 1)?;
        let packages = repository
            .borrow_mut()
            .find_packages(name.as_bstr(), constraint)
            .map_err(call.failed())?;
        Ok(PluginValue::List(
            packages
                .iter()
                .map(|package| package_handle_value(package.as_rc()))
                .collect(),
        ))
    }
}

impl forwarding::repository::writable_array_repository::Forwarding
    for forwarding::repository::writable_array_repository::PhpClass
{
    fn get_canonical_packages(
        repository: &RepositoryInterfaceHandle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let mut borrowed = repository.borrow_mut();
        let packages = borrowed
            .as_writable_repository_interface_mut()
            .expect("only a writable repository crosses as a WritableArrayRepository")
            .get_canonical_packages()
            .map_err(call.failed())?;
        Ok(packages.to_wire())
    }

    fn get_dev_package_names(
        repository: &RepositoryInterfaceHandle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let mut borrowed = repository.borrow_mut();
        Ok(borrowed
            .as_writable_repository_interface_mut()
            .expect("only a writable repository crosses as a WritableArrayRepository")
            .get_dev_package_names()
            .to_wire())
    }
}

impl forwarding::repository::filesystem_repository::Forwarding
    for forwarding::repository::filesystem_repository::PhpClass
{
}

impl forwarding::repository::installed_array_repository::Forwarding
    for forwarding::repository::installed_array_repository::PhpClass
{
}

impl forwarding::repository::installed_filesystem_repository::Forwarding
    for forwarding::repository::installed_filesystem_repository::PhpClass
{
}

impl forwarding::repository::lock_array_repository::Forwarding
    for forwarding::repository::lock_array_repository::PhpClass
{
}
