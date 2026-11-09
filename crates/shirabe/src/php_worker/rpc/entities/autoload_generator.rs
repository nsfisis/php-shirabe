//! The methods a `Composer\Autoload\AutoloadGenerator` entity answers over RPC.

use crate::autoload::{AutoloadGeneratorInterface, ClassLoader};
use crate::config::Config;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::installer::InstallationManagerInterface;
use crate::package::{LockerInterface, PackageInterfaceHandle, RootPackageInterfaceHandle};
use crate::php_worker::call::PhpCall;
use crate::php_worker::codec::class_map_to_wire;
use crate::php_worker::rpc::args::{arg, arg_or, arg_throw};
use crate::php_worker::rpc::dispatch::{error_throw, unavailable_method};
use crate::php_worker::rpc::table::{entity_arg, optional_entity_arg};
use crate::php_worker::runtime::new_php_object;
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, ToWire, wire};
use shirabe_php_shim::PhpMixed;

pub(crate) fn dispatch_autoload_generator_method(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let flag = |position: usize| arg_or::<bool>(method_name, args, position, true);
    match method_name.as_bytes() {
        b"setDevMode" => generator.borrow().set_dev_mode(flag(0)?),
        b"setClassMapAuthoritative" => {
            generator
                .borrow()
                .set_class_map_authoritative(arg::<bool>(method_name, args, 0)?)
        }
        b"setApcu" => generator.borrow().set_apcu(
            arg::<bool>(method_name, args, 0)?,
            arg::<Option<BString>>(method_name, args, 1)?,
        ),
        b"setRunScripts" => generator.borrow().set_run_scripts(flag(0)?),
        b"setDryRun" => generator.borrow().set_dry_run(flag(0)?),
        b"setIgnorePlatformRequirements" => {
            // TODO(php-runtime): upstream raises an E_USER_DEPRECATED here
            // ("AutoloadGenerator::setIgnorePlatformRequirements is deprecated since Composer 2.2,
            // use setPlatformRequirementFilter instead."), which Composer's ErrorHandler prints as
            // a deprecation notice; the worker does not run that handler.
            let filter = PlatformRequirementFilterFactory::from_bool_or_list(arg::<PhpMixed>(
                method_name,
                args,
                0,
            )?)
            .map_err(|error| error_throw(b"setIgnorePlatformRequirements failed", &error))?;
            generator.borrow().set_platform_requirement_filter(filter);
        }
        b"dump" => {
            let config =
                entity_arg::<std::rc::Rc<std::cell::RefCell<Config>>>(method_name, args, 0)?;
            let local_repo = arg::<RepositoryInterfaceHandle>(method_name, args, 1)?;
            let root_package = root_package_arg(method_name, args, 2)?;
            let installation_manager = entity_arg::<
                std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            >(method_name, args, 3)?;
            let target_dir = arg::<BString>(method_name, args, 4)?;
            let scan_psr_packages = arg_or::<bool>(method_name, args, 5, false)?;
            let suffix = arg::<Option<BString>>(method_name, args, 6)?;
            let locker = optional_entity_arg::<std::rc::Rc<std::cell::RefCell<dyn LockerInterface>>>(
                method_name,
                args,
                7,
            )?;
            let strict_ambiguous = arg_or::<bool>(method_name, args, 8, false)?;
            let class_map = generator
                .borrow()
                .dump(
                    &config.borrow(),
                    local_repo,
                    root_package,
                    installation_manager,
                    target_dir.as_bstr(),
                    scan_psr_packages,
                    suffix,
                    locker,
                    strict_ambiguous,
                )
                .map_err(|error| error_throw(b"dump failed", &error))?;
            return Ok(class_map_to_wire(&class_map));
        }
        b"buildPackageMap" => {
            let installation_manager = entity_arg::<
                std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>,
            >(method_name, args, 0)?;
            // TODO(type-model): PHP takes any PackageInterface as the root package here, while
            // the Rust signature narrows it to a RootPackageInterface.
            let root_package = root_package_arg(method_name, args, 1)?;
            let packages = arg::<Vec<PackageInterfaceHandle>>(method_name, args, 2)?;
            let package_map = generator
                .borrow()
                .build_package_map(installation_manager, root_package, packages)
                .map_err(|error| error_throw(b"buildPackageMap failed", &error))?;
            return Ok(package_map_value(package_map));
        }
        b"parseAutoloads" => {
            let package_map = package_map_arg(method_name, args, 0)?;
            let root_package = root_package_arg(method_name, args, 1)?;
            let filtered_dev_packages =
                arg_or::<PhpMixed>(method_name, args, 2, PhpMixed::Bool(false))?;
            return Ok(generator
                .borrow()
                .parse_autoloads(package_map, root_package, filtered_dev_packages)
                .to_wire());
        }
        b"createLoader" => {
            let autoloads = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            let vendor_dir = arg::<Option<BString>>(method_name, args, 1)?;
            let loader = generator
                .borrow()
                .create_loader(&autoloads, vendor_dir.clone())
                .map_err(|error| error_throw(b"createLoader failed", &error))?;
            return class_loader_to_php(&loader.borrow(), vendor_dir)
                .map_err(|error| error_throw(b"createLoader failed", &error));
        }
        // TODO(plugin): no PlatformRequirementFilterInterface implementation crosses the boundary:
        // the factory's filters are guarded in the worker, and a plugin-implemented filter has no
        // Rust-side adapter.
        b"setPlatformRequirementFilter" => {
            return Err(PhpThrow::runtime(
                "AutoloadGenerator::setPlatformRequirementFilter() cannot take a filter over RPC yet",
            ));
        }
        other => {
            return Err(unavailable_method("AutoloadGenerator", other));
        }
    }
    Ok(PluginValue::Null)
}

/// Resolves a package argument the PHP signature types as a root package.
pub(crate) fn root_package_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<RootPackageInterfaceHandle, PhpThrow> {
    arg::<PackageInterfaceHandle>(method, args, position)?
        .as_root()
        .ok_or_else(|| {
            arg_throw(
                method,
                position,
                wire::unexpected(
                    "a RootPackageInterface",
                    args.get(position).unwrap_or(&wire::NULL),
                ),
            )
        })
}

/// Decodes the `array{0: PackageInterface, 1: string|null}[]` package map `buildPackageMap()`
/// returns.
fn package_map_arg(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>, PhpThrow> {
    arg::<Vec<Vec<PluginValue>>>(method, args, position)?
        .into_iter()
        .map(|pair| match <[PluginValue; 2]>::try_from(pair) {
            Ok([package, install_path]) => Ok((
                PackageInterfaceHandle::from_wire(&package)?,
                Option::<std::path::PathBuf>::from_wire(&install_path)?,
            )),
            Err(pair) => Err(wire::unexpected(
                "a [package, install path] pair",
                &PluginValue::List(pair),
            )),
        })
        .collect::<Result<_, PhpThrow>>()
        .map_err(|throw| arg_throw(method, position, throw))
}

fn package_map_value(
    package_map: Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>,
) -> PluginValue {
    PluginValue::List(
        package_map
            .into_iter()
            .map(|(package, install_path)| {
                PluginValue::List(vec![package.to_wire(), install_path.to_wire()])
            })
            .collect(),
    )
}

/// `ClassLoader` is the class the worker bootstraps its own autoloading with, so the child holds
/// the real implementation and the loader is built there: a fresh instance receives the state the
/// Rust-side loader ended up with, through the same methods `createLoader()` fills it with.
fn class_loader_to_php(
    loader: &ClassLoader,
    vendor_dir: Option<BString>,
) -> anyhow::Result<PluginValue> {
    let handle = new_php_object(
        BStr::new("Composer\\Autoload\\ClassLoader"),
        vec![vendor_dir.to_wire()],
    )?;
    let add = |method: &[u8], prefix: &[u8], paths: &[BString]| {
        PhpCall::Method(&handle, BStr::new(method))
            .run::<PluginValue>(vec![PluginValue::string(prefix), paths.to_vec().to_wire()])
            .map(|_| ())
    };
    for (prefix, paths) in loader.get_prefixes() {
        add(b"add", prefix.as_ref(), &paths)?;
    }
    if !loader.get_fallback_dirs().is_empty() {
        add(b"add", b"".as_slice(), loader.get_fallback_dirs())?;
    }
    for (prefix, paths) in loader.get_prefixes_psr4() {
        add(b"addPsr4", prefix.as_ref(), paths)?;
    }
    if !loader.get_fallback_dirs_psr4().is_empty() {
        add(b"addPsr4", b"".as_slice(), loader.get_fallback_dirs_psr4())?;
    }
    if !loader.get_class_map().is_empty() {
        PhpCall::Method(&handle, BStr::new("addClassMap"))
            .run::<PluginValue>(vec![loader.get_class_map().clone().to_wire()])?;
    }
    Ok(PluginValue::PhpHandle(handle))
}
