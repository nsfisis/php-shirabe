//! ref: composer/src/Composer/Repository/InstalledRepository.php

use indexmap::IndexMap;
use shirabe_php_shim::LogicException;
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;

use crate::package::base_package::BasePackage;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::lock_array_repository::LockArrayRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};
use crate::repository::root_package_repository::RootPackageRepository;

pub enum NeedleInput {
    Single(String),
    Multiple(Vec<String>),
}

pub struct DependentsEntry(
    pub Box<BasePackage>,
    pub Link,
    pub Option<Vec<DependentsEntry>>,
);

#[derive(Debug)]
pub struct InstalledRepository {
    inner: CompositeRepository,
}

impl InstalledRepository {
    pub fn new(repositories: Vec<Box<dyn RepositoryInterface>>) -> anyhow::Result<Self> {
        let mut this = Self {
            inner: CompositeRepository::new(vec![]),
        };
        for repo in repositories {
            this.add_repository(repo)?;
        }
        Ok(this)
    }

    pub fn find_packages_with_replacers_and_providers(
        &self,
        name: String,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<BasePackage>> {
        let name = name.to_lowercase();

        let constraint: Option<Box<dyn ConstraintInterface>> = match constraint {
            None => None,
            Some(FindPackageConstraint::Constraint(c)) => Some(c),
            Some(FindPackageConstraint::String(s)) => {
                let version_parser = VersionParser::new();
                // TODO(phase-b): Arc<dyn ConstraintInterface + Send + Sync> -> Box<dyn ConstraintInterface>
                Some(Box::new(version_parser.parse_constraints(&s).unwrap()))
            }
        };

        let mut matches = vec![];
        for repo in self.inner.get_repositories() {
            'candidates: for candidate in repo.get_packages() {
                if name == candidate.get_name() {
                    if constraint.is_none()
                        || constraint
                            .as_ref()
                            .unwrap()
                            .matches(&Constraint::new("==", candidate.get_version()))
                    {
                        matches.push(candidate);
                    }
                    continue;
                }

                let mut provides_and_replaces: Vec<&Link> = vec![];
                for link in candidate.get_provides().values() {
                    provides_and_replaces.push(link);
                }
                for link in candidate.get_replaces().values() {
                    provides_and_replaces.push(link);
                }
                for link in provides_and_replaces {
                    if name == link.get_target()
                        && (constraint.is_none()
                            || constraint.as_ref().unwrap().matches(link.get_constraint()))
                    {
                        matches.push(candidate);
                        continue 'candidates;
                    }
                }
            }
        }

        matches
    }

    pub fn get_dependents(
        &self,
        needle: NeedleInput,
        constraint: Option<Box<dyn ConstraintInterface>>,
        invert: bool,
        recurse: bool,
        packages_found: Option<Vec<String>>,
    ) -> Vec<DependentsEntry> {
        let mut needles: Vec<String> = match needle {
            NeedleInput::Single(s) => vec![s.to_lowercase()],
            NeedleInput::Multiple(v) => v.into_iter().map(|s| s.to_lowercase()).collect(),
        };
        let mut results: Vec<DependentsEntry> = vec![];

        let mut packages_found = packages_found.unwrap_or_else(|| needles.clone());

        let mut root_package: Option<Box<BasePackage>> = None;
        for package in self.inner.get_packages() {
            if package.as_any().is::<dyn RootPackageInterface>() {
                root_package = Some(package);
                break;
            }
        }

        for package in self.inner.get_packages() {
            let mut links: IndexMap<String, Link> = package.get_requires();
            let mut packages_in_tree = packages_found.clone();

            if !invert {
                for (k, v) in package.get_replaces() {
                    links.entry(k).or_insert(v);
                }

                let needles_snapshot = needles.clone();
                for link in package.get_replaces().values() {
                    for needle in &needles_snapshot {
                        if link.get_source() == needle.as_str() {
                            if constraint.is_none()
                                || link
                                    .get_constraint()
                                    .matches(constraint.as_ref().unwrap().as_ref())
                            {
                                if packages_in_tree.contains(&link.get_target().to_string()) {
                                    results.push(DependentsEntry(
                                        package.clone_box(),
                                        link.clone(),
                                        None,
                                    ));
                                    continue;
                                }
                                packages_in_tree.push(link.get_target().to_string());
                                let dependents = if recurse {
                                    self.get_dependents(
                                        NeedleInput::Single(link.get_target().to_string()),
                                        None,
                                        false,
                                        true,
                                        Some(packages_in_tree.clone()),
                                    )
                                } else {
                                    vec![]
                                };
                                results.push(DependentsEntry(
                                    package.clone_box(),
                                    link.clone(),
                                    Some(dependents),
                                ));
                                needles.push(link.get_target().to_string());
                            }
                        }
                    }
                }
            }

            if package.as_any().is::<dyn RootPackageInterface>() {
                for (k, v) in package.get_dev_requires() {
                    links.entry(k).or_insert(v);
                }
            }

            for link in links.values() {
                for needle in &needles {
                    if link.get_target() == needle.as_str() {
                        let matches_constraint = constraint.as_ref().map_or(true, |c| {
                            link.get_constraint().matches(c.as_ref()) == !invert
                        });
                        if constraint.is_none() || matches_constraint {
                            if packages_in_tree.contains(&link.get_source().to_string()) {
                                results.push(DependentsEntry(
                                    package.clone_box(),
                                    link.clone(),
                                    None,
                                ));
                                continue;
                            }
                            packages_in_tree.push(link.get_source().to_string());
                            let dependents = if recurse {
                                self.get_dependents(
                                    NeedleInput::Single(link.get_source().to_string()),
                                    None,
                                    false,
                                    true,
                                    Some(packages_in_tree.clone()),
                                )
                            } else {
                                vec![]
                            };
                            results.push(DependentsEntry(
                                package.clone_box(),
                                link.clone(),
                                Some(dependents),
                            ));
                        }
                    }
                }
            }

            if invert && needles.contains(&package.get_name().to_string()) {
                for link in package.get_conflicts().values() {
                    for pkg in self.find_packages(link.get_target().to_string(), None) {
                        let version = Constraint::new("=", pkg.get_version());
                        if link.get_constraint().matches(&version) == invert {
                            results.push(DependentsEntry(
                                package.clone_box(),
                                link.clone(),
                                None,
                            ));
                        }
                    }
                }
            }

            for link in package.get_conflicts().values() {
                if needles.contains(&link.get_target().to_string()) {
                    for pkg in self.find_packages(link.get_target().to_string(), None) {
                        let version = Constraint::new("=", pkg.get_version());
                        if link.get_constraint().matches(&version) == invert {
                            results.push(DependentsEntry(
                                package.clone_box(),
                                link.clone(),
                                None,
                            ));
                        }
                    }
                }
            }

            if invert
                && constraint.is_some()
                && needles.contains(&package.get_name().to_string())
                && constraint
                    .as_ref()
                    .unwrap()
                    .matches(&Constraint::new("=", package.get_version()))
            {
                'requires: for link in package.get_requires().values() {
                    if PlatformRepository::is_platform_package(link.get_target()) {
                        if self
                            .find_package(
                                link.get_target().to_string(),
                                FindPackageConstraint::Constraint(
                                    link.get_constraint().clone_box(),
                                ),
                            )
                            .is_some()
                        {
                            continue;
                        }

                        let platform_pkg = self.find_package(
                            link.get_target().to_string(),
                            FindPackageConstraint::String("*".to_string()),
                        );
                        let description = platform_pkg
                            .as_ref()
                            .map(|p| format!("but {} is installed", p.get_pretty_version()))
                            .unwrap_or_else(|| "but it is missing".to_string());
                        results.push(DependentsEntry(
                            package.clone_box(),
                            Link::new(
                                package.get_name().to_string(),
                                link.get_target().to_string(),
                                Box::new(MatchAllConstraint::new()),
                                Some(Link::TYPE_REQUIRE.to_string()),
                                Some(format!(
                                    "{} {}",
                                    link.get_pretty_constraint().unwrap_or_default(),
                                    description
                                )),
                            ),
                            None,
                        ));

                        continue;
                    }

                    for pkg in self.get_packages() {
                        if !pkg.get_names().contains(&link.get_target().to_string()) {
                            continue;
                        }

                        let mut version: Box<dyn ConstraintInterface> =
                            Box::new(Constraint::new("=", pkg.get_version()));

                        if link.get_target() != pkg.get_name() {
                            let mut replaces_and_provides: IndexMap<String, Link> =
                                pkg.get_replaces();
                            for (k, v) in pkg.get_provides() {
                                replaces_and_provides.entry(k).or_insert(v);
                            }
                            for prov in replaces_and_provides.values() {
                                if link.get_target() == prov.get_target() {
                                    version = prov.get_constraint().clone_box();
                                    break;
                                }
                            }
                        }

                        if !link.get_constraint().matches(version.as_ref()) {
                            if let Some(root_pkg) = root_package.as_ref() {
                                let mut root_reqs: IndexMap<String, Link> = root_pkg.get_requires();
                                for (k, v) in root_pkg.get_dev_requires() {
                                    root_reqs.entry(k).or_insert(v);
                                }
                                for root_req in root_reqs.values() {
                                    if pkg.get_names().contains(&root_req.get_target().to_string())
                                        && !root_req
                                            .get_constraint()
                                            .matches(link.get_constraint())
                                    {
                                        results.push(DependentsEntry(
                                            package.clone_box(),
                                            link.clone(),
                                            None,
                                        ));
                                        results.push(DependentsEntry(
                                            root_pkg.clone_box(),
                                            root_req.clone(),
                                            None,
                                        ));
                                        continue 'requires;
                                    }
                                }

                                results.push(DependentsEntry(
                                    package.clone_box(),
                                    link.clone(),
                                    None,
                                ));
                                results.push(DependentsEntry(
                                    root_pkg.clone_box(),
                                    Link::new(
                                        root_pkg.get_name().to_string(),
                                        link.get_target().to_string(),
                                        Box::new(MatchAllConstraint::new()),
                                        Some(Link::TYPE_DOES_NOT_REQUIRE.to_string()),
                                        Some(format!(
                                            "but {} is installed",
                                            pkg.get_pretty_version()
                                        )),
                                    ),
                                    None,
                                ));
                            } else {
                                results.push(DependentsEntry(
                                    package.clone_box(),
                                    link.clone(),
                                    None,
                                ));
                            }
                        }

                        continue 'requires;
                    }
                }
            }
        }

        // ksort($results) - no-op for a numerically-indexed Vec
        results
    }

    pub fn add_repository(
        &mut self,
        repository: Box<dyn RepositoryInterface>,
    ) -> anyhow::Result<()> {
        if repository.as_any().is::<LockArrayRepository>()
            || repository.as_any().is::<dyn InstalledRepositoryInterface>()
            || repository.as_any().is::<RootPackageRepository>()
            || repository.as_any().is::<PlatformRepository>()
        {
            self.inner.add_repository(repository);
            return Ok(());
        }

        Err(anyhow::anyhow!(LogicException {
            message: format!(
                "An InstalledRepository can not contain a repository of type {} ({})",
                std::any::type_name_of_val(&*repository),
                repository.get_repo_name(),
            ),
            code: 0,
        }))
    }
}

impl shirabe_php_shim::Countable for InstalledRepository {
    fn count(&self) -> i64 {
        self.inner.count()
    }
}

impl RepositoryInterface for InstalledRepository {
    fn get_repo_name(&self) -> String {
        let names: Vec<String> = self
            .inner
            .get_repositories()
            .iter()
            .map(|repo| repo.get_repo_name())
            .collect();
        format!("installed repo ({})", names.join(", "))
    }

    fn has_package(&self, package: &dyn PackageInterface) -> bool {
        self.inner.has_package(package)
    }

    fn find_package(
        &self,
        name: String,
        constraint: FindPackageConstraint,
    ) -> Option<Box<BasePackage>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &self,
        name: String,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<BasePackage>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&self) -> Vec<Box<BasePackage>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult {
        self.inner.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
        self.inner.get_providers(package_name)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
