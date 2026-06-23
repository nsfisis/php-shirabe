//! ref: composer/src/Composer/Repository/PackageRepository.php

use crate::advisory::SecurityAdvisory;
use crate::advisory::{AnySecurityAdvisory, PartialSecurityAdvisory};
use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::loader::ArrayLoader;
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
use shirabe_php_shim::{Exception, PhpMixed, RuntimeException, var_export};
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

    pub fn initialize(&mut self) -> anyhow::Result<Result<(), InvalidRepositoryException>> {
        self.inner.initialize();

        let mut loader =
            ValidatingArrayLoader::new(Box::new(ArrayLoader::new(None, true)), true, None, 0);
        for package in &self.config {
            let config_map: IndexMap<String, PhpMixed> = match package {
                PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => IndexMap::new(),
            };
            let package_loaded = match loader.load(config_map, "") {
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
        Preg::replace(r"^array ", "package ", &self.inner.get_repo_name())
    }
}

impl RepositoryInterface for PackageRepository {
    // The structural methods are inherited from ArrayRepository in PHP, where they trigger the
    // overridden initialize() that loads packages from config. Wiring that virtual dispatch is a
    // Phase C concern; the advisory paths below are what is exercised so far.
    fn count(&self) -> anyhow::Result<usize> {
        todo!()
    }

    fn has_package(&self, _package: PackageInterfaceHandle) -> bool {
        todo!()
    }

    fn find_package(
        &mut self,
        _name: &str,
        _constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        todo!()
    }

    fn find_packages(
        &mut self,
        _name: &str,
        _constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        todo!()
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        todo!()
    }

    fn load_packages(
        &mut self,
        _package_name_map: IndexMap<String, Option<shirabe_semver::constraint::AnyConstraint>>,
        _acceptable_stabilities: IndexMap<String, i64>,
        _stability_flags: IndexMap<String, i64>,
        _already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        todo!()
    }

    fn search(
        &mut self,
        _query: String,
        _mode: i64,
        _type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        todo!()
    }

    fn get_providers(
        &mut self,
        _package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        todo!()
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
