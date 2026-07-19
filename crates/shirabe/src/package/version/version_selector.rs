//! ref: composer/src/Composer/Package/Version/VersionSelector.php

use crate::filter::platform_requirement_filter::IgnoreAllPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::IgnoreListPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::package::PackageInterfaceHandle;
use crate::package::base_package;
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::version::VersionParser;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositorySetInterface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION, php_regex, strtolower,
    version_compare,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

/// PHP `$showWarnings` (`true` or a `callable(PackageInterface): bool`) as passed to
/// `VersionSelector::findBestCandidate`.
pub enum ShowWarnings {
    Always,
    Predicate(Box<dyn Fn(&PackageInterfaceHandle) -> bool>),
}

#[derive(Debug)]
pub struct VersionSelector {
    repository_set: std::rc::Rc<std::cell::RefCell<dyn RepositorySetInterface>>,
    platform_constraints: IndexMap<String, Vec<AnyConstraint>>,
    parser: Option<VersionParser>,
}

impl VersionSelector {
    pub fn new(
        repository_set: std::rc::Rc<std::cell::RefCell<dyn RepositorySetInterface>>,
        platform_repo: Option<&mut crate::repository::PlatformRepository>,
    ) -> anyhow::Result<Self> {
        let mut platform_constraints: IndexMap<String, Vec<AnyConstraint>> = IndexMap::new();
        if let Some(platform_repo) = platform_repo {
            for package in platform_repo.get_packages()? {
                let constraint = SimpleConstraint::new(
                    "==".to_string(),
                    package.get_version().to_string(),
                    None,
                );
                platform_constraints
                    .entry(package.get_name().to_string())
                    .or_default()
                    .push(constraint.into());
            }
        }
        Ok(Self {
            repository_set,
            platform_constraints,
            parser: None,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn find_best_candidate(
        &mut self,
        package_name: &str,
        target_package_version: Option<&str>,
        preferred_stability: &str,
        platform_requirement_filter: Option<std::rc::Rc<dyn PlatformRequirementFilterInterface>>,
        repo_set_flags: i64,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        show_warnings: ShowWarnings,
    ) -> anyhow::Result<Option<crate::package::PackageInterfaceHandle>> {
        if !base_package::STABILITIES.contains_key(preferred_stability) {
            return Err(shirabe_php_shim::UnexpectedValueException {
                message: format!(
                    "Expected a valid stability name as 3rd argument, got {}",
                    preferred_stability
                ),
                code: 0,
            }
            .into());
        }

        let platform_requirement_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface> =
            match platform_requirement_filter {
                Some(f) => f,
                None => PlatformRequirementFilterFactory::ignore_nothing(),
            };

        let constraint = match target_package_version {
            Some(v) => Some(self.get_parser().parse_constraints(v)?),
            None => None,
        };
        let mut candidates = self.repository_set.borrow().find_packages(
            &strtolower(package_name),
            constraint.clone(),
            repo_set_flags,
        )?;

        let min_priority = *base_package::STABILITIES.get(preferred_stability).unwrap();
        candidates.sort_by(|a, b| {
            // BasePackage::get_stability_priority() is not forwarded by the handle; compute it
            // directly from the stability name.
            let a_priority = *base_package::STABILITIES
                .get(a.get_stability().as_str())
                .unwrap();
            let b_priority = *base_package::STABILITIES
                .get(b.get_stability().as_str())
                .unwrap();

            if min_priority < a_priority && b_priority < a_priority {
                return std::cmp::Ordering::Greater;
            }
            if min_priority < a_priority && a_priority < b_priority {
                return std::cmp::Ordering::Less;
            }
            if min_priority >= a_priority && min_priority < b_priority {
                return std::cmp::Ordering::Less;
            }

            if version_compare(&b.get_version(), &a.get_version(), ">") {
                std::cmp::Ordering::Greater
            } else if version_compare(&b.get_version(), &a.get_version(), "<") {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });

        let is_ignore_all = platform_requirement_filter
            .as_ref()
            .as_any()
            .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
            .is_some();

        let package: Option<crate::package::PackageInterfaceHandle>;
        if !self.platform_constraints.is_empty() && !is_ignore_all {
            let mut already_warned_names: IndexMap<String, bool> = IndexMap::new();
            let mut already_seen_names: IndexMap<String, bool> = IndexMap::new();
            let mut found_package: Option<crate::package::PackageInterfaceHandle> = None;

            for pkg in candidates.iter() {
                let reqs = pkg.get_requires();
                let mut skip = false;
                'reqs: for (name, link) in &reqs {
                    if !PlatformRepository::is_platform_package(name)
                        || platform_requirement_filter.is_ignored(name)
                    {
                        continue;
                    }
                    let reason;
                    if let Some(provided_constraints) = self.platform_constraints.get(name) {
                        for provided_constraint in provided_constraints {
                            if link.get_constraint().matches(provided_constraint) {
                                continue 'reqs;
                            }
                            let list_filter_opt = platform_requirement_filter
                                .as_ref()
                                .as_any()
                                .downcast_ref::<IgnoreListPlatformRequirementFilter>(
                            );
                            if let Some(list_filter) = list_filter_opt
                                && list_filter.is_upper_bound_ignored(name)
                            {
                                let filtered_constraint = list_filter.filter_constraint(
                                    name,
                                    link.get_constraint().clone(),
                                    false,
                                )?;
                                if filtered_constraint.matches(provided_constraint) {
                                    continue 'reqs;
                                }
                            }
                        }
                        reason = "is not satisfied by your platform";
                    } else {
                        reason = "is missing from your platform";
                    }

                    let is_latest_version = !already_seen_names.contains_key(&pkg.get_name());
                    already_seen_names.insert(pkg.get_name().to_string(), true);
                    if let Some(ref io) = io {
                        let should_warn = match &show_warnings {
                            ShowWarnings::Always => true,
                            ShowWarnings::Predicate(f) => f(pkg),
                        };
                        if should_warn {
                            let warn_key = format!("{}/{}", pkg.get_name(), link.get_target());
                            let is_first_warning = !already_warned_names.contains_key(&warn_key);
                            already_warned_names.insert(warn_key, true);
                            let latest = if is_latest_version {
                                "'s latest version"
                            } else {
                                ""
                            };
                            io.write_error3(
                                &format!(
                                    "<warning>Cannot use {}{} {} as it {} {} {} which {}.</>",
                                    pkg.get_pretty_name(),
                                    latest,
                                    pkg.get_pretty_version(),
                                    link.get_description(),
                                    link.get_target(),
                                    link.get_pretty_constraint(),
                                    reason
                                ),
                                true,
                                if is_first_warning {
                                    io_interface::NORMAL
                                } else {
                                    io_interface::VERBOSE
                                },
                            );
                        }
                    }

                    skip = true;
                }

                if skip {
                    continue;
                }

                found_package = Some(pkg.clone());
                break;
            }
            package = found_package;
        } else {
            package = if !candidates.is_empty() {
                Some(candidates.remove(0))
            } else {
                None
            };
        }

        let package = match package {
            None => return Ok(None),
            Some(p) => p,
        };

        let package = if let Some(alias) = package.as_alias() {
            if alias.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                alias.get_alias_of().into()
            } else {
                package
            }
        } else {
            package
        };

        Ok(Some(package))
    }

    pub fn find_recommended_require_version(
        &mut self,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<String> {
        if package.get_name().starts_with("ext-") {
            let php_version = format!(
                "{}.{}.{}",
                PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION
            );
            let package_version = package.get_version();
            let ext_parts: Vec<&str> = package_version.splitn(4, '.').collect();
            let ext_version = ext_parts[..3.min(ext_parts.len())].join(".");
            if php_version == ext_version {
                return Ok("*".to_string());
            }
        }

        let version = package.get_version().to_string();
        if !package.is_dev() {
            return self.transform_version(
                &version,
                &package.get_pretty_version(),
                &package.get_stability(),
            );
        }

        let loader = ArrayLoader::new(Some(self.get_parser().clone()), false);
        let dumper = ArrayDumper::new();
        let extra = loader.get_branch_alias(&dumper.dump(package.clone()))?;
        if let Some(extra) = extra
            && extra != VersionParser::DEFAULT_BRANCH_ALIAS
        {
            let new_extra = Preg::replace(
                php_regex!(r"{^(\d+\.\d+\.\d+)(\.9999999)-dev$}"),
                "$1.0",
                &extra,
            );
            if new_extra != extra {
                let new_extra = new_extra.replace(".9999999", ".0");
                return self.transform_version(&new_extra, &new_extra, "dev");
            }
        }

        Ok(package.get_pretty_version().to_string())
    }

    fn transform_version(
        &self,
        version: &str,
        pretty_version: &str,
        stability: &str,
    ) -> anyhow::Result<String> {
        let semantic_version_parts: Vec<&str> = version.split('.').collect();

        if semantic_version_parts.len() == 4
            && Preg::is_match(php_regex!(r"{^\d+\D?}"), semantic_version_parts[3])
        {
            let mut parts: Vec<String> = semantic_version_parts
                .iter()
                .map(|s| s.to_string())
                .collect();
            let version = if parts[0] == "0" {
                parts.truncate(3);
                parts.join(".")
            } else {
                parts.truncate(2);
                parts.join(".")
            };

            let version = if stability != "stable" {
                format!("{}@{}", version, stability)
            } else {
                version
            };

            Ok(format!("^{}", version))
        } else {
            Ok(pretty_version.to_string())
        }
    }

    fn get_parser(&mut self) -> &VersionParser {
        if self.parser.is_none() {
            self.parser = Some(VersionParser::new());
        }
        self.parser.as_ref().unwrap()
    }
}
