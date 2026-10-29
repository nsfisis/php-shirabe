//! ref: composer/src/Composer/Repository/PackageRepository.php

use crate::advisory::{AnySecurityAdvisory, PartialSecurityAdvisory};
use crate::package::loader::{ArrayLoader, LoaderInterface, ValidatingArrayLoader};
use crate::package::version::VersionParser;
use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::{
    AdvisoryProviderInterface, ArrayRepository, FindPackageConstraint, InvalidRepositoryException,
    LoadPackagesResult, ProviderInfo, RepositoryInterface, RepositoryInterfaceWeakHandle,
    SearchResult, SecurityAdvisoryResult,
};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, json_encode, legacy_lossy, php_regex, preg_replace, var_export,
};
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct PackageRepository {
    inner: ArrayRepository,
    config: Vec<PhpMixed>,
    security_advisories: IndexMap<BString, PhpMixed>,
}

impl PackageRepository {
    pub fn new(config: IndexMap<BString, PhpMixed>) -> Self {
        let package = config.get(B("package")).cloned().unwrap_or(PhpMixed::Null);
        let config_list: Vec<PhpMixed> = match package {
            PhpMixed::List(list) => list.into_iter().collect(),
            other => vec![other],
        };

        let security_advisories = match config
            .get(B("security-advisories"))
            .cloned()
            .unwrap_or(PhpMixed::Array(IndexMap::new()))
        {
            PhpMixed::Array(map) => map,
            _ => IndexMap::new(),
        };

        Self {
            inner: ArrayRepository::new(vec![])
                .expect("ArrayRepository::new with empty vec cannot fail"),
            config: config_list,
            security_advisories,
        }
    }

    pub fn initialize(&self) -> anyhow::Result<Result<(), InvalidRepositoryException>> {
        self.inner.initialize();

        let loader =
            ValidatingArrayLoader::new(Box::new(ArrayLoader::new(None, true)), true, None, 0);
        for package in &self.config {
            let config_map: IndexMap<BString, PhpMixed> = match package {
                PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => IndexMap::new(),
            };
            let package_loaded = match loader.load(
                config_map,
                Some("Composer\\Package\\CompletePackage".to_string()),
            ) {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!(
                        "A repository of type \"package\" contains an invalid package definition: {}\n\nInvalid package definition:\n{}",
                        e,
                        json_encode(package).unwrap_or_default()
                    );
                    return Ok(Err(InvalidRepositoryException::new(
                        legacy_lossy(&msg).into_owned(),
                    )));
                }
            };
            self.inner.add_package(package_loaded)?;
        }
        Ok(Ok(()))
    }

    pub fn get_repo_name(&self) -> anyhow::Result<BString> {
        use crate::repository::RepositoryInterface;
        // PHP: parent::getRepoName() counts through the late-bound $this->initialize(),
        // which resolves to PackageRepository::initialize (loading the config packages).
        self.ensure_initialized()?;
        Ok(BString::from(
            &preg_replace(
                php_regex!(r"{^array }"),
                "package ",
                &self.inner.get_repo_name()?,
            )[..],
        ))
    }

    // In PHP the inherited ArrayRepository methods lazily call the overridden initialize() to load
    // the configured packages. Without virtual dispatch we trigger that load here before delegating
    // to the inner repository; ArrayRepository's own lazy check then sees the populated array and
    // skips re-initializing it.
    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?.map_err(anyhow::Error::from)?;
        }
        Ok(())
    }
}

impl RepositoryInterface for PackageRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where the lazy package load
    // is driven by the overridden initialize(). Here each one first ensures that load has happened
    // (see ensure_initialized), then delegates to the inner ArrayRepository.
    fn count(&mut self) -> anyhow::Result<usize> {
        self.ensure_initialized()?;
        self.inner.count()
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.ensure_initialized()?;
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &BStr,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &BStr,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<BString, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        self.ensure_initialized()?;
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(
        &mut self,
        query: BString,
        mode: i64,
        r#type: Option<BString>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> anyhow::Result<BString> {
        PackageRepository::get_repo_name(self)
    }

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        Some(self)
    }

    fn as_advisory_provider_mut(&mut self) -> Option<&mut dyn AdvisoryProviderInterface> {
        Some(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_self_handle(&self, weak: RepositoryInterfaceWeakHandle) {
        self.inner.set_self_handle(weak);
    }
}

impl AdvisoryProviderInterface for PackageRepository {
    fn has_security_advisories(&mut self) -> anyhow::Result<bool> {
        Ok(!self.security_advisories.is_empty())
    }

    fn get_security_advisories(
        &mut self,
        package_constraint_map: IndexMap<BString, AnyConstraint>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        let parser = VersionParser::new();

        let mut advisories = IndexMap::new();
        for (package_name, package_advisories) in &self.security_advisories {
            let package_name: &BStr = package_name.as_bstr();
            let Some(package_constraint) = package_constraint_map.get(package_name) else {
                continue;
            };

            let list = match package_advisories {
                PhpMixed::List(list) => list,
                _ => continue,
            };
            let mut items: Vec<std::rc::Rc<AnySecurityAdvisory>> = Vec::new();
            for data in list {
                let data_map: IndexMap<BString, PhpMixed> = match data {
                    PhpMixed::Array(m) => m.clone(),
                    _ => continue,
                };
                let advisory = match PartialSecurityAdvisory::create(
                    bstr::BStr::new(&*legacy_lossy(package_name)),
                    &data_map,
                    &parser,
                ) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                if !allow_partial_advisories && matches!(advisory, AnySecurityAdvisory::Partial(_))
                {
                    return Err(RuntimeException::new(format!(
                        "Advisory for {} could not be loaded as a full advisory from {}\n{}",
                        package_name.display_lossy(),
                        self.get_repo_name()?.display_lossy(),
                        var_export(data, true).display_lossy()
                    ))
                    .into());
                }

                if !advisory.affected_versions().matches(package_constraint) {
                    continue;
                }

                items.push(std::rc::Rc::new(advisory));
            }
            advisories.insert(BString::from(package_name), items);
        }

        let names_found: Vec<BString> = advisories.keys().cloned().collect();
        let advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> = advisories
            .into_iter()
            .filter(|(_, adv)| !adv.is_empty())
            .collect();

        Ok(SecurityAdvisoryResult {
            names_found,
            advisories,
        })
    }
}
