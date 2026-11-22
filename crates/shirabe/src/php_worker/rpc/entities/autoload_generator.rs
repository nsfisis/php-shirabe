//! The methods a `Composer\Autoload\AutoloadGenerator` entity answers over RPC.

use crate::autoload::{
    __php_binding_autoload_generator_interface, AutoloadGeneratorInterface, ClassLoader,
};
use crate::config::Config;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::installer::InstallationManagerInterface;
use crate::package::{LockerInterface, PackageInterfaceHandle, RootPackageInterfaceHandle};
use crate::php_worker::call::PhpCall;
use crate::php_worker::codec::encode_class_map_generator_class_map;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::runtime::autoloading;
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, ToRpcValue, codec};
use shirabe_php_shim::{E_USER_DEPRECATED, PhpMixed, trigger_error};

php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    type_error: "an AutoloadGenerator",
    classes: {
        r"Composer\Autoload\AutoloadGenerator": { constructor: unimplemented },
    },
    methods: {
        r"Composer\Autoload\AutoloadGenerator": {
            handwritten: {
                module: self,
                members: [
                    setIgnorePlatformRequirements,
                    dump,
                    buildPackageMap,
                    parseAutoloads,
                    createLoader,
                ],
            },
            instance: {
                adapters: __php_binding_autoload_generator_interface,
                target: dyn AutoloadGeneratorInterface,
                borrow: |generator| generator.borrow(),
            },
            refused: {
                setPlatformRequirementFilter: "no platform requirement filter can cross the boundary",
            },
        },
    },
}

fn set_ignore_platform_requirements(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    trigger_error(
        "AutoloadGenerator::setIgnorePlatformRequirements is deprecated since Composer 2.2, use setPlatformRequirementFilter instead.",
        E_USER_DEPRECATED,
    );

    let filter = PlatformRequirementFilterFactory::from_bool_or_list(call.arg::<PhpMixed>(0)?)
        .map_err(call.failed())?;
    generator.borrow().set_platform_requirement_filter(filter);
    Ok(RpcValue::Null)
}

fn dump(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let config = call.entity::<std::rc::Rc<Config>>(0)?;
    let local_repo = call.arg::<RepositoryInterfaceHandle>(1)?;
    let root_package = root_package(call, 2)?;
    let installation_manager =
        call.entity::<std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>>(3)?;
    let target_dir = call.arg::<BString>(4)?;
    let scan_psr_packages = call.arg::<bool>(5)?;
    let suffix = call.arg::<Option<BString>>(6)?;
    let locker = call.optional_entity::<std::rc::Rc<dyn LockerInterface>>(7)?;
    let strict_ambiguous = call.arg::<bool>(8)?;
    let class_map = generator
        .borrow()
        .dump(
            &config,
            local_repo,
            root_package,
            installation_manager,
            target_dir.as_bstr(),
            scan_psr_packages,
            suffix,
            locker,
            strict_ambiguous,
        )
        .map_err(call.failed())?;
    Ok(encode_class_map_generator_class_map(&class_map))
}

fn build_package_map(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let installation_manager =
        call.entity::<std::rc::Rc<std::cell::RefCell<dyn InstallationManagerInterface>>>(0)?;
    // TODO(type-model): PHP takes any PackageInterface as the root package here, while the
    // Rust signature narrows it to a RootPackageInterface.
    let root_package = root_package(call, 1)?;
    let packages = call.arg::<Vec<PackageInterfaceHandle>>(2)?;
    let package_map = generator
        .borrow()
        .build_package_map(installation_manager, root_package, packages)
        .map_err(call.failed())?;
    Ok(package_map_value(package_map))
}

fn parse_autoloads(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let package_map = package_map_arg(call, 0)?;
    let root_package = root_package(call, 1)?;
    let filtered_dev_packages = call.arg::<PhpMixed>(2)?;
    Ok(generator
        .borrow()
        .parse_autoloads(package_map, root_package, filtered_dev_packages)
        .to_rpc_value())
}

fn create_loader(
    generator: &std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>>,
    call: &mut Call<'_>,
) -> Result<RpcValue, PhpThrow> {
    let autoloads = call.arg::<IndexMap<BString, PhpMixed>>(0)?;
    let vendor_dir = call.arg::<Option<BString>>(1)?;
    let loader = generator
        .borrow()
        .create_loader(&autoloads, vendor_dir.clone())
        .map_err(call.failed())?;
    class_loader_to_php(&loader.borrow(), vendor_dir).map_err(call.failed())
}

/// Resolves a package argument the PHP signature types as a root package.
pub(crate) fn root_package(
    call: &Call<'_>,
    position: usize,
) -> Result<RootPackageInterfaceHandle, PhpThrow> {
    call.arg::<PackageInterfaceHandle>(position)?
        .as_root()
        .ok_or_else(|| {
            call.arg_throw(
                position,
                codec::unexpected(
                    "a RootPackageInterface",
                    call.args().get(position).unwrap_or(&codec::NULL),
                ),
            )
        })
}

/// Decodes the `array{0: PackageInterface, 1: string|null}[]` package map `buildPackageMap()`
/// returns.
fn package_map_arg(
    call: &Call<'_>,
    position: usize,
) -> Result<Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>, PhpThrow> {
    call.arg::<Vec<Vec<RpcValue>>>(position)?
        .into_iter()
        .map(|pair| match <[RpcValue; 2]>::try_from(pair) {
            Ok([package, install_path]) => Ok((
                PackageInterfaceHandle::from_rpc_value(&package)?,
                Option::<std::path::PathBuf>::from_rpc_value(&install_path)?,
            )),
            Err(pair) => Err(codec::unexpected(
                "a [package, install path] pair",
                &RpcValue::List(pair),
            )),
        })
        .collect::<Result<_, DecodeError>>()
        .map_err(|throw| call.arg_throw(position, throw))
}

fn package_map_value(
    package_map: Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)>,
) -> RpcValue {
    RpcValue::List(
        package_map
            .into_iter()
            .map(|(package, install_path)| {
                RpcValue::List(vec![package.to_rpc_value(), install_path.to_rpc_value()])
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
) -> anyhow::Result<RpcValue> {
    let handle = autoloading::new_object(
        BStr::new("Composer\\Autoload\\ClassLoader"),
        vec![vendor_dir.to_rpc_value()],
    )?;
    let add = |method: &[u8], prefix: &[u8], paths: &[BString]| {
        PhpCall::Method(&handle, BStr::new(method))
            .run::<RpcValue>(vec![
                RpcValue::string(prefix),
                paths.to_vec().to_rpc_value(),
            ])
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
            .run::<RpcValue>(vec![loader.get_class_map().clone().to_rpc_value()])?;
    }
    Ok(RpcValue::PhpEntityHandle(handle))
}
