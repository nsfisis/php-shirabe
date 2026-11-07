//! The methods a `Composer\Package\PackageInterface` entity answers over RPC.

use crate::package::{
    AliasPackage, AnyPackage, CompletePackageInterface, DisplayMode, Link, Mirror, Package,
    PackageInterfaceHandle, RootPackageInterface, STABILITY_STABLE, stability_priority,
};
use crate::plugin::rpc::codec::{arg, arg_or};
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::table::{package_handle_value, repository_handle_value};
use crate::repository::{PlatformRepository, RepositoryInterfaceHandle};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{PhpMixed, bformat};

/// The read half of the `CompletePackage` / `RootPackage` surface, which `PackageInterface`
/// does not carry. `None` means the method belongs to the base surface below.
fn dispatch_complete_package_getter(
    package: &AnyPackage,
    method_name: &BStr,
) -> Result<Option<PluginValue>, PhpThrow> {
    let unavailable = || package_method_unavailable(method_name, b"this package");
    let complete = || {
        package
            .as_complete_package_interface()
            .ok_or_else(unavailable)
    };
    let root = || package.as_root_package_interface().ok_or_else(unavailable);
    let value = match method_name.as_bytes() {
        b"getScripts" => complete()?.get_scripts().to_wire(),
        b"getRepositories" => complete()?.get_repositories().to_wire(),
        b"getLicense" => complete()?.get_license().to_wire(),
        b"getKeywords" => complete()?.get_keywords().to_wire(),
        b"getDescription" => complete()?.get_description().to_wire(),
        b"getHomepage" => complete()?.get_homepage().to_wire(),
        b"getAuthors" => complete()?.get_authors().to_wire(),
        b"getSupport" => complete()?.get_support().to_wire(),
        b"getFunding" => complete()?.get_funding().to_wire(),
        b"isAbandoned" => PluginValue::Bool(complete()?.is_abandoned()),
        b"getReplacementPackage" => complete()?.get_replacement_package().to_wire(),
        b"getArchiveName" => complete()?.get_archive_name().to_wire(),
        b"getArchiveExcludes" => complete()?.get_archive_excludes().to_wire(),
        b"getAliases" => root()?.get_aliases().to_wire(),
        b"getMinimumStability" => PluginValue::string(root()?.get_minimum_stability()),
        b"getStabilityFlags" => root()?.get_stability_flags().to_wire(),
        b"getReferences" => root()?.get_references().to_wire(),
        b"getPreferStable" => PluginValue::Bool(root()?.get_prefer_stable()),
        b"getConfig" => root()?.get_config().to_wire(),
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn package_method_unavailable(method_name: &BStr, receiver: &[u8]) -> PhpThrow {
    PhpThrow::runtime(bformat!(
        "`{method_name}` is not available on {receiver} over RPC"
    ))
}

/// The base `Package` behind `package`, for `Package`'s own setters. The concrete subclasses
/// inherit them (their PHP overrides in `RootPackage` delegate to the same base state), so the
/// base package answers for every real variant.
fn base_package_mut<'a>(
    package: &'a mut AnyPackage,
    method_name: &BStr,
) -> Result<&'a mut Package, PhpThrow> {
    package
        .as_package_mut()
        .ok_or_else(|| package_method_unavailable(method_name, b"an alias package"))
}

fn complete_package_mut<'a>(
    package: &'a mut AnyPackage,
    method_name: &BStr,
) -> Result<&'a mut dyn CompletePackageInterface, PhpThrow> {
    package
        .as_complete_package_interface_mut()
        .ok_or_else(|| package_method_unavailable(method_name, b"this package"))
}

fn root_package_mut<'a>(
    package: &'a mut AnyPackage,
    method_name: &BStr,
) -> Result<&'a mut dyn RootPackageInterface, PhpThrow> {
    package
        .as_root_package_interface_mut()
        .ok_or_else(|| package_method_unavailable(method_name, b"this package"))
}

/// Runs a setter that both `RootPackageInterface` and `Package` declare. `RootAliasPackage`
/// overrides each of these to write through to its alias target, and `RootPackage` reaches the
/// same base state either way, so a root package goes through the interface and any other
/// package through its base `Package`.
///
/// TODO(type-model): choosing the body for the concrete variant belongs on `AnyPackage`, not
/// here. The stub surface already decides which classes carry a method, so the `as_*_mut`
/// accessors' "not available on an alias package" arms are unreachable for a method the alias
/// stubs do not declare, and what is left is a per-variant dispatch that the `is_root` check
/// only approximates: it covers `RootPackage` as well, where the extra hop is equivalent only
/// while that impl keeps delegating to the base package, and nothing checks it. A new variant
/// compiles into the wrong body without a diagnostic.
fn set_on_root_or_base_package<T>(
    package: &mut AnyPackage,
    method_name: &BStr,
    value: T,
    root: impl FnOnce(&mut dyn RootPackageInterface, T) -> anyhow::Result<()>,
    base: impl FnOnce(&mut Package, T),
) -> Result<(), PhpThrow> {
    if package.is_root() {
        root(
            package
                .as_root_package_interface_mut()
                .expect("a root package exposes RootPackageInterface"),
            value,
        )
        .map_err(|error| error_throw(bformat!("{method_name} failed"), &error))?;
    } else {
        base(base_package_mut(package, method_name)?, value);
    }
    Ok(())
}

pub(crate) fn dispatch_package_method(
    package: &std::rc::Rc<std::cell::RefCell<AnyPackage>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let links_arg = || arg_or::<IndexMap<BString, Link>>(method_name, args, 0, IndexMap::new());
    match method_name.as_bytes() {
        b"setId" => {
            let id = arg::<i64>(method_name, args, 0)?;
            package.borrow_mut().as_package_interface_mut().set_id(id);
        }
        b"setInstallationSource" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_installation_source(value);
        }
        b"setSourceReference" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_source_reference(value);
        }
        b"setSourceUrl" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_source_url(value);
        }
        b"setDistUrl" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_dist_url(value);
        }
        b"setDistType" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_dist_type(value);
        }
        b"setDistReference" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_dist_reference(value);
        }
        b"setSourceDistReferences" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?.ok_or_else(|| {
                PhpThrow::runtime("setSourceDistReferences expects a reference string")
            })?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_source_dist_references(value);
        }
        b"setSourceMirrors" => {
            let mirrors = arg::<Option<Vec<Mirror>>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_source_mirrors(mirrors);
        }
        b"setDistMirrors" => {
            let mirrors = arg::<Option<Vec<Mirror>>>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_dist_mirrors(mirrors);
        }
        b"setRepository" => {
            let repository = arg::<RepositoryInterfaceHandle>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_repository(repository)
                .map_err(|error| error_throw(b"setRepository failed", &error))?;
        }
        b"setTransportOptions" => {
            let options =
                arg_or::<IndexMap<BString, PhpMixed>>(method_name, args, 0, IndexMap::new())?;
            package
                .borrow_mut()
                .as_package_interface_mut()
                .set_transport_options(options);
        }
        b"setRequires" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            links_arg()?,
            |p, links| p.set_requires(links),
            |p, links| p.set_requires(links),
        )?,
        b"setDevRequires" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            links_arg()?,
            |p, links| p.set_dev_requires(links),
            |p, links| p.set_dev_requires(links),
        )?,
        b"setConflicts" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            links_arg()?,
            |p, links| p.set_conflicts(links),
            |p, links| p.set_conflicts(links),
        )?,
        b"setProvides" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            links_arg()?,
            |p, links| p.set_provides(links),
            |p, links| p.set_provides(links),
        )?,
        b"setReplaces" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            links_arg()?,
            |p, links| p.set_replaces(links),
            |p, links| p.set_replaces(links),
        )?,
        b"setAutoload" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?,
            |p, autoload| {
                p.set_autoload(autoload);
                Ok(())
            },
            |p, autoload| p.set_autoload(autoload),
        )?,
        b"setDevAutoload" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?,
            |p, autoload| {
                p.set_dev_autoload(autoload);
                Ok(())
            },
            |p, autoload| p.set_dev_autoload(autoload),
        )?,
        b"setSuggests" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            arg::<IndexMap<BString, BString>>(method_name, args, 0)?,
            |p, suggests| {
                p.set_suggests(suggests);
                Ok(())
            },
            |p, suggests| p.set_suggests(suggests),
        )?,
        b"setExtra" => set_on_root_or_base_package(
            &mut package.borrow_mut(),
            method_name,
            arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?,
            |p, extra| {
                p.set_extra(extra);
                Ok(())
            },
            |p, extra| p.set_extra(extra),
        )?,
        b"setType" => {
            let value = arg::<BString>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_type(value);
        }
        b"setTargetDir" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_target_dir(value);
        }
        b"setBinaries" => {
            let value = arg::<Vec<BString>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_binaries(value);
        }
        b"setSourceType" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_source_type(value);
        }
        b"setDistSha1Checksum" => {
            let value = arg::<Option<BString>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_dist_sha1_checksum(value);
        }
        b"setIncludePaths" => {
            let value = arg::<Vec<BString>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_include_paths(value);
        }
        b"setPhpExt" => {
            let value = arg::<Option<IndexMap<BString, PhpMixed>>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_php_ext(value);
        }
        b"setNotificationUrl" => {
            let value = arg::<BString>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_notification_url(value);
        }
        b"setIsDefaultBranch" => {
            let value = arg::<bool>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_is_default_branch(value);
        }
        b"replaceVersion" => {
            let version = arg::<BString>(method_name, args, 0)?;
            let pretty_version = arg::<BString>(method_name, args, 1)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?
                .replace_version(version, pretty_version);
        }
        b"setReleaseDate" => {
            let date = arg::<Option<chrono::DateTime<chrono::Utc>>>(method_name, args, 0)?;
            base_package_mut(&mut package.borrow_mut(), method_name)?.set_release_date(date);
        }
        b"setScripts" => {
            let value = arg::<IndexMap<BString, Vec<BString>>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_scripts(value);
        }
        b"setRepositories" => {
            let value = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_repositories(value);
        }
        b"setLicense" => {
            let value = arg::<Vec<BString>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_license(value);
        }
        b"setKeywords" => {
            let value = arg::<Vec<BString>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_keywords(value);
        }
        b"setDescription" => {
            let value = arg::<BString>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_description(value);
        }
        b"setHomepage" => {
            let value = arg::<BString>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_homepage(value);
        }
        b"setAuthors" => {
            let value = arg::<Vec<IndexMap<BString, BString>>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_authors(value);
        }
        b"setSupport" => {
            let value = arg::<IndexMap<BString, BString>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_support(value);
        }
        b"setFunding" => {
            let value = arg::<Vec<IndexMap<BString, PhpMixed>>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_funding(value);
        }
        b"setAbandoned" => {
            let value = arg::<PhpMixed>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_abandoned(value);
        }
        b"setArchiveName" => {
            let value = arg::<BString>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?.set_archive_name(value);
        }
        b"setArchiveExcludes" => {
            let value = arg::<Vec<BString>>(method_name, args, 0)?;
            complete_package_mut(&mut package.borrow_mut(), method_name)?
                .set_archive_excludes(value);
        }
        b"setStabilityFlags" => {
            let value = arg::<IndexMap<BString, i64>>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_stability_flags(value);
        }
        b"setMinimumStability" => {
            let value = arg::<BString>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_minimum_stability(value);
        }
        b"setPreferStable" => {
            let value = arg::<bool>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_prefer_stable(value);
        }
        b"setConfig" => {
            let value = arg::<IndexMap<BString, PhpMixed>>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_config(value);
        }
        b"setReferences" => {
            let value = arg::<IndexMap<BString, BString>>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_references(value);
        }
        b"setAliases" => {
            let value = arg::<Vec<IndexMap<BString, BString>>>(method_name, args, 0)?;
            root_package_mut(&mut package.borrow_mut(), method_name)?.set_aliases(value);
        }
        b"equals" => {
            let other = arg::<PackageInterfaceHandle>(method_name, args, 0)?;
            let this = PackageInterfaceHandle::from_rc_unchecked(package.clone());
            return Ok(this.equals(&other).to_wire());
        }
        // The subclasses narrow `getAliasOf`'s return type to their own alias target, but every
        // variant holds the one entity.
        b"getAliasOf" => {
            return Ok(package_handle_value(
                alias_package(&package.borrow(), method_name)?
                    .get_alias_of()
                    .as_rc(),
            ));
        }
        b"isRootPackageAlias" => {
            return Ok(alias_package(&package.borrow(), method_name)?
                .is_root_package_alias()
                .to_wire());
        }
        b"hasSelfVersionRequires" => {
            return Ok(alias_package(&package.borrow(), method_name)?
                .has_self_version_requires()
                .to_wire());
        }
        b"setRootPackageAlias" => {
            let value = arg::<bool>(method_name, args, 0)?;
            package
                .borrow_mut()
                .as_alias_package_mut()
                .ok_or_else(|| package_method_unavailable(method_name, b"this package"))?
                .set_root_package_alias(value);
        }
        _ => return dispatch_package_getter(&package.borrow(), method_name, args),
    }
    Ok(PluginValue::Null)
}

fn alias_package<'a>(
    package: &'a AnyPackage,
    method_name: &BStr,
) -> Result<&'a AliasPackage, PhpThrow> {
    package
        .as_alias_package()
        .ok_or_else(|| package_method_unavailable(method_name, b"this package"))
}

fn dispatch_package_getter(
    package: &AnyPackage,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    if let Some(value) = dispatch_complete_package_getter(package, method_name)? {
        return Ok(value);
    }

    let package = package.as_package_interface();
    match method_name.as_bytes() {
        b"getName" => Ok(package.get_name().to_wire()),
        b"getPrettyName" => Ok(package.get_pretty_name().to_wire()),
        b"getNames" => {
            let provides = arg_or::<bool>(method_name, args, 0, true)?;
            Ok(package.get_names(provides).to_wire())
        }
        b"getId" => Ok(package.get_id().to_wire()),
        b"isDev" => Ok(package.is_dev().to_wire()),
        b"getType" => Ok(package.get_type().to_wire()),
        b"getTargetDir" => Ok(package.get_target_dir().to_wire()),
        b"getExtra" => Ok(package.get_extra().to_wire()),
        b"getInstallationSource" => Ok(package.get_installation_source().to_wire()),
        b"getSourceType" => Ok(package.get_source_type().to_wire()),
        b"getSourceUrl" => Ok(package.get_source_url().to_wire()),
        b"getSourceUrls" => Ok(package.get_source_urls().to_wire()),
        b"getSourceReference" => Ok(package.get_source_reference().to_wire()),
        b"getSourceMirrors" => Ok(package.get_source_mirrors().to_wire()),
        b"getDistType" => Ok(package.get_dist_type().to_wire()),
        b"getDistUrl" => Ok(package.get_dist_url().to_wire()),
        b"getDistUrls" => Ok(package.get_dist_urls().to_wire()),
        b"getDistReference" => Ok(package.get_dist_reference().to_wire()),
        b"getDistSha1Checksum" => Ok(package.get_dist_sha1_checksum().to_wire()),
        b"getDistMirrors" => Ok(package.get_dist_mirrors().to_wire()),
        b"getVersion" => Ok(package.get_version().to_wire()),
        b"getPrettyVersion" => Ok(package.get_pretty_version().to_wire()),
        b"getFullPrettyVersion" => {
            let truncate = arg_or::<bool>(method_name, args, 0, true)?;
            let display_mode =
                arg_or::<DisplayMode>(method_name, args, 1, DisplayMode::SourceRefIfDev)?;
            Ok(package
                .get_full_pretty_version(truncate, display_mode)
                .to_wire())
        }
        b"getStability" => Ok(package.get_stability().to_wire()),
        b"getRequires" => Ok((*package.get_requires()).clone().to_wire()),
        b"getConflicts" => Ok((*package.get_conflicts()).clone().to_wire()),
        b"getProvides" => Ok((*package.get_provides()).clone().to_wire()),
        b"getReplaces" => Ok((*package.get_replaces()).clone().to_wire()),
        b"getDevRequires" => Ok((*package.get_dev_requires()).clone().to_wire()),
        b"getSuggests" => Ok(package.get_suggests().to_wire()),
        b"getAutoload" => Ok(package.get_autoload().to_wire()),
        b"getDevAutoload" => Ok(package.get_dev_autoload().to_wire()),
        b"getIncludePaths" => Ok(package.get_include_paths().to_wire()),
        b"getPhpExt" => Ok(match package.get_php_ext() {
            Some(config) => config.to_wire(),
            None => PluginValue::Null,
        }),
        b"getRepository" => match package.get_repository() {
            Some(repository) => repository_handle_value(&repository),
            None => Ok(PluginValue::Null),
        },
        b"getBinaries" => Ok(package.get_binaries().to_wire()),
        b"getUniqueName" => Ok(package.get_unique_name().to_wire()),
        b"getNotificationUrl" => Ok(package.get_notification_url().to_wire()),
        b"__toString" => Ok(package.get_unique_name().to_wire()),
        b"getPrettyString" => Ok(package.get_pretty_string().to_wire()),
        b"isDefaultBranch" => Ok(package.is_default_branch().to_wire()),
        // `BasePackage`'s concrete methods are not forwarded by `PackageInterface`, so both are
        // computed from the interface here, as `VersionSelector` already does for the second.
        b"isPlatform" => Ok(package
            .get_repository()
            .is_some_and(|repository| repository.is::<PlatformRepository>())
            .to_wire()),
        b"getStabilityPriority" => Ok(stability_priority(package.get_stability())
            .unwrap_or(STABILITY_STABLE)
            .to_wire()),
        b"getTransportOptions" => Ok(package.get_transport_options().to_wire()),
        b"getReleaseDate" => Ok(package.get_release_date().to_wire()),
        other => Err(unavailable_method("package", other)),
    }
}
