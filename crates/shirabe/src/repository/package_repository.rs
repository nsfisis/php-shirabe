//! ref: composer/src/Composer/Repository/PackageRepository.php

use crate::advisory::{AnySecurityAdvisory, PartialSecurityAdvisory};
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::package::loader::ValidatingArrayLoader;
use crate::package::version::VersionParser;
use crate::repository::ArrayRepository;
use crate::repository::InvalidRepositoryException;
use crate::repository::RepositoryInterfaceWeakHandle;
use crate::repository::{
    AdvisoryProviderInterface, FindPackageConstraint, LoadPackagesResult, ProviderInfo,
    RepositoryInterface, SearchResult, SecurityAdvisoryResult,
};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{Exception, PhpMixed, RuntimeException, php_regex, var_export};
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct PackageRepository {
    inner: ArrayRepository,
    config: Vec<PhpMixed>,
    security_advisories: IndexMap<String, PhpMixed>,
}

impl PackageRepository {
    pub fn new(config: IndexMap<String, PhpMixed>) -> Self {
        let package = config.get("package").cloned().unwrap_or(PhpMixed::Null);
        let config_list: Vec<PhpMixed> = match package {
            PhpMixed::List(list) => list.into_iter().collect(),
            other => vec![other],
        };

        let security_advisories = match config
            .get("security-advisories")
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
            let config_map: IndexMap<String, PhpMixed> = match package {
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
                        shirabe_php_shim::json_encode(package).unwrap_or_default()
                    );
                    return Ok(Err(InvalidRepositoryException(Exception {
                        message: msg,
                        code: 0,
                    })));
                }
            };
            self.inner.add_package(package_loaded)?;
        }
        Ok(Ok(()))
    }

    pub fn get_repo_name(&self) -> String {
        use crate::repository::RepositoryInterface;
        Preg::replace(
            php_regex!(r"{^array }"),
            "package ",
            &self.inner.get_repo_name(),
        )
    }

    // In PHP the inherited ArrayRepository methods lazily call the overridden initialize() to load
    // the configured packages. Without virtual dispatch we trigger that load here before delegating
    // to the inner repository; ArrayRepository's own lazy check then sees the populated array and
    // skips re-initializing it.
    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.inner.is_initialized() {
            self.initialize()?.map_err(anyhow::Error::new)?;
        }
        Ok(())
    }
}

impl RepositoryInterface for PackageRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where the lazy package load
    // is driven by the overridden initialize(). Here each one first ensures that load has happened
    // (see ensure_initialized), then delegates to the inner ArrayRepository.
    fn count(&self) -> anyhow::Result<usize> {
        self.ensure_initialized()?;
        self.inner.count()
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        // TODO(phase-d): hasPackage returns bool and cannot surface an initialization error; a
        // failed load leaves the inner repository with whatever packages were added before the
        // failure.
        let _ = self.ensure_initialized();
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.ensure_initialized()?;
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
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
        package_name_map: IndexMap<String, Option<shirabe_semver::constraint::AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
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
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.ensure_initialized()?;
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        self.ensure_initialized()?;
        self.inner.get_providers(package_name)
    }

    fn get_repo_name(&self) -> String {
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
        package_constraint_map: IndexMap<String, AnyConstraint>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        let parser = VersionParser::new();

        let mut advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
        for (package_name, package_advisories) in &self.security_advisories {
            let Some(package_constraint) = package_constraint_map.get(package_name) else {
                continue;
            };

            let list = match package_advisories {
                PhpMixed::List(list) => list,
                _ => continue,
            };
            let mut items: Vec<AnySecurityAdvisory> = Vec::new();
            for data in list {
                let data_map: IndexMap<String, PhpMixed> = match data {
                    PhpMixed::Array(m) => m.clone(),
                    _ => continue,
                };
                let advisory =
                    match PartialSecurityAdvisory::create(package_name, &data_map, &parser) {
                        Ok(a) => a,
                        Err(_) => continue,
                    };
                if !allow_partial_advisories && matches!(advisory, AnySecurityAdvisory::Partial(_))
                {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "Advisory for {} could not be loaded as a full advisory from {}\n{}",
                            package_name,
                            self.get_repo_name(),
                            var_export(data, true)
                        ),
                        code: 0,
                    }));
                }

                if !advisory.affected_versions().matches(package_constraint) {
                    continue;
                }

                items.push(advisory);
            }
            advisories.insert(package_name.clone(), items);
        }

        let names_found: Vec<String> = advisories.keys().cloned().collect();
        let advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = advisories
            .into_iter()
            .filter(|(_, adv)| !adv.is_empty())
            .collect();

        Ok(SecurityAdvisoryResult {
            names_found,
            advisories,
        })
    }
}
