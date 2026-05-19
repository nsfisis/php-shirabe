//! ref: composer/src/Composer/Repository/PackageRepository.php

use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::advisory::security_advisory::SecurityAdvisory;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::validating_array_loader::ValidatingArrayLoader;
use crate::package::version::version_parser::VersionParser;
use crate::repository::advisory_provider_interface::{
    AdvisoryProviderInterface, PartialOrSecurityAdvisory, SecurityAdvisoryResult,
};
use crate::repository::array_repository::ArrayRepository;
use crate::repository::invalid_repository_exception::InvalidRepositoryException;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{Exception, PhpMixed, RuntimeException, var_export};
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

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
            PhpMixed::List(list) => list.into_iter().map(|p| *p).collect(),
            other => vec![other],
        };

        let security_advisories = match config
            .get("security-advisories")
            .cloned()
            .unwrap_or(PhpMixed::Array(IndexMap::new()))
        {
            PhpMixed::Array(map) => map.into_iter().map(|(k, v)| (k, *v)).collect(),
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
            let config_map: IndexMap<String, Box<PhpMixed>> = match package {
                PhpMixed::Array(m) => m.clone(),
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
            // TODO(phase-b): add_package expects Box<dyn PackageInterface>; loader returns Box<dyn BasePackage>
            let _ = package_loaded;
        }
        Ok(Ok(()))
    }

    pub fn get_repo_name(&self) -> String {
        use crate::repository::repository_interface::RepositoryInterface;
        Preg::replace(r"^array ", "package ", &self.inner.get_repo_name())
            .unwrap_or_else(|_| self.inner.get_repo_name())
    }
}

impl AdvisoryProviderInterface for PackageRepository {
    fn has_security_advisories(&self) -> bool {
        !self.security_advisories.is_empty()
    }

    fn get_security_advisories(
        &self,
        package_constraint_map: IndexMap<String, Box<dyn ConstraintInterface>>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        let parser = VersionParser::new();
        let semver_parser = shirabe_semver::version_parser::VersionParser;
        let _ = parser;

        let mut advisories: IndexMap<String, Vec<PartialOrSecurityAdvisory>> = IndexMap::new();
        for (package_name, package_advisories) in &self.security_advisories {
            if !package_constraint_map.contains_key(package_name.as_str()) {
                continue;
            }
            let list = match package_advisories {
                PhpMixed::List(list) => list,
                _ => continue,
            };
            let mut items: Vec<PartialOrSecurityAdvisory> = Vec::new();
            for data in list {
                let data_map: IndexMap<String, PhpMixed> = match data.as_ref() {
                    PhpMixed::Array(m) => m.iter().map(|(k, v)| (k.clone(), *v.clone())).collect(),
                    _ => continue,
                };
                let advisory = match PartialSecurityAdvisory::create(
                    package_name,
                    &data_map,
                    &semver_parser,
                ) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                if !allow_partial_advisories
                    && matches!(advisory, PartialOrSecurityAdvisory::Partial(_))
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
                // TODO(phase-b): affected_versions is a method, not a field, and matches() return type may differ
                let _ = (&advisory, &package_constraint_map);
                items.push(advisory);
            }
            advisories.insert(package_name.clone(), items);
        }

        let names_found: Vec<String> = advisories.keys().cloned().collect();
        let advisories: IndexMap<String, Vec<PartialOrSecurityAdvisory>> = advisories
            .into_iter()
            .filter(|(_, adv)| !adv.is_empty())
            .collect();

        Ok(SecurityAdvisoryResult {
            names_found,
            advisories,
        })
    }
}
