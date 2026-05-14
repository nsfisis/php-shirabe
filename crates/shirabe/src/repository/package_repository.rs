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
use shirabe_php_shim::{Exception, PhpMixed};
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
            inner: ArrayRepository::new(),
            config: config_list,
            security_advisories,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<Result<(), InvalidRepositoryException>> {
        self.inner.initialize()?;

        let loader = ValidatingArrayLoader::new(ArrayLoader::new(None, true), true);
        for package in &self.config {
            let package = match loader.load(package) {
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
            self.inner.add_package(package)?;
        }
        Ok(Ok(()))
    }

    pub fn get_repo_name(&self) -> String {
        Preg::replace(r"^array ", "package ", &self.inner.get_repo_name())
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

        let mut advisories: IndexMap<String, Vec<PartialOrSecurityAdvisory>> = IndexMap::new();
        for (package_name, package_advisories) in &self.security_advisories {
            if package_constraint_map.contains_key(package_name.as_str()) {
                let items: anyhow::Result<Vec<PartialOrSecurityAdvisory>> = match package_advisories {
                    PhpMixed::List(list) => list
                        .iter()
                        .filter_map(|data| {
                            let data_map = match data.as_ref() {
                                PhpMixed::Array(m) => m
                                    .iter()
                                    .map(|(k, v)| (k.clone(), *v.clone()))
                                    .collect::<IndexMap<String, PhpMixed>>(),
                                _ => return Ok(None),
                            };
                            let advisory_any =
                                PartialSecurityAdvisory::create(package_name, &data_map, &parser)
                                    .ok()?;
                            let advisory =
                                if let Ok(full) = advisory_any.downcast::<SecurityAdvisory>() {
                                    PartialOrSecurityAdvisory::Full(*full)
                                } else if let Ok(partial) =
                                    advisory_any.downcast::<PartialSecurityAdvisory>()
                                {
                                    PartialOrSecurityAdvisory::Partial(*partial)
                                } else {
                                    return Ok(None);
                                };
                            if !allow_partial_advisories
                                && matches!(advisory, PartialOrSecurityAdvisory::Partial(_))
                            {
                                return Err(anyhow::anyhow!(RuntimeException { message: format!("Advisory for {} could not be loaded as a full advisory from {}\n{}", package_name, self.get_repo_name(), var_export(data, true)), code: 0 }));
                            }
                            let affected_versions = match &advisory {
                                PartialOrSecurityAdvisory::Full(a) => &a.affected_versions,
                                PartialOrSecurityAdvisory::Partial(a) => &a.affected_versions,
                            };
                            if !affected_versions
                                .matches(package_constraint_map[package_name.as_str()].as_ref())
                            {
                                return Ok(None);
                            }
                            Ok(Some(advisory))
                        })
                        .collect(),
                    _ => vec![],
                };
                advisories.insert(package_name.clone(), items?);
            }
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
