//! ref: composer/src/Composer/Repository/InstalledRepository.php

use crate::package::version::VersionParser;
use crate::package::{BasePackageHandle, Link, PackageInterfaceHandle};
use crate::repository::{
    CompositeRepository, FindPackageConstraint, LoadPackagesResult, LockArrayRepository,
    PlatformRepository, ProviderInfo, RepositoryInterface, RepositoryInterfaceHandle,
    RootPackageRepository, SearchResult,
};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{array_merge_map, bconcat, implode, legacy_lossy, strtolower};
use shirabe_semver::constraint::{AnyConstraint, MatchAllConstraint, SimpleConstraint};

pub enum NeedleInput {
    Single(BString),
    Multiple(Vec<BString>),
}

pub struct DependentsEntry(
    pub BasePackageHandle,
    pub Link,
    pub Option<Vec<DependentsEntry>>,
);

#[derive(Debug)]
pub struct InstalledRepository {
    inner: CompositeRepository,
}

impl InstalledRepository {
    pub fn new(repositories: Vec<RepositoryInterfaceHandle>) -> Self {
        let mut this = Self {
            inner: CompositeRepository::new(vec![]),
        };
        for repo in repositories {
            this.add_repository(repo);
        }
        this
    }

    pub fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        self.inner.get_repositories()
    }

    pub fn find_packages_with_replacers_and_providers(
        &self,
        name: &BStr,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        let name = strtolower(name);

        let constraint: Option<AnyConstraint> = match constraint {
            None => None,
            Some(FindPackageConstraint::Constraint(c)) => Some(c),
            Some(FindPackageConstraint::String(s)) => {
                let version_parser = VersionParser::new();
                Some(version_parser.parse_constraints(&s).unwrap())
            }
        };

        let mut matches = vec![];
        for repo in self.inner.get_repositories() {
            'candidates: for candidate in repo.get_packages()? {
                if name == candidate.get_name() {
                    if constraint.is_none()
                        || constraint.as_ref().unwrap().matches(
                            &SimpleConstraint::new(
                                "==",
                                BString::from(&candidate.get_version()[..]),
                                None,
                            )
                            .into(),
                        )
                    {
                        matches.push(candidate.clone());
                    }
                    continue;
                }

                let provides_and_replaces = array_merge_map(
                    (*candidate.get_provides()).clone(),
                    (*candidate.get_replaces()).clone(),
                );
                for link in provides_and_replaces.values() {
                    if name == link.get_target()
                        && (constraint.is_none()
                            || constraint.as_ref().unwrap().matches(link.get_constraint()))
                    {
                        matches.push(candidate.clone());
                        continue 'candidates;
                    }
                }
            }
        }

        Ok(matches)
    }

    pub fn get_dependents(
        &mut self,
        needle: NeedleInput,
        constraint: Option<AnyConstraint>,
        invert: bool,
        recurse: bool,
        packages_found: Option<Vec<BString>>,
    ) -> anyhow::Result<Vec<DependentsEntry>> {
        let mut needles: Vec<BString> = match needle {
            NeedleInput::Single(s) => vec![strtolower(&s)],
            NeedleInput::Multiple(v) => v.into_iter().map(|s| strtolower(&s)).collect(),
        };
        let mut results: Vec<DependentsEntry> = vec![];

        let packages_found = packages_found.unwrap_or_else(|| needles.clone());

        let mut root_package: Option<BasePackageHandle> = None;
        for package in self.inner.get_packages()? {
            if package.as_root().is_some() {
                root_package = Some(package);
                break;
            }
        }

        for package in self.inner.get_packages()? {
            let mut links: IndexMap<BString, Link> = (*package.get_requires()).clone();
            let mut packages_in_tree = packages_found.clone();

            if !invert {
                for (k, v) in package.get_replaces().iter() {
                    links
                        .entry(bstr::BString::from(&k.clone()[..]))
                        .or_insert(v.clone());
                }

                let needles_snapshot = needles.clone();
                for link in package.get_replaces().values() {
                    for needle in &needles_snapshot {
                        if link.get_source() == needle.as_bstr()
                            && (constraint.is_none()
                                || link.get_constraint().matches(constraint.as_ref().unwrap()))
                        {
                            if packages_in_tree.contains(&BString::from(link.get_target()).clone())
                            {
                                results.push(DependentsEntry(package.clone(), link.clone(), None));
                                continue;
                            }
                            packages_in_tree.push(bstr::BString::from(link.get_target()));
                            let dependents = if recurse {
                                self.get_dependents(
                                    NeedleInput::Single(bstr::BString::from(link.get_target())),
                                    None,
                                    false,
                                    true,
                                    Some(packages_in_tree.clone()),
                                )?
                            } else {
                                vec![]
                            };
                            results.push(DependentsEntry(
                                package.clone(),
                                link.clone(),
                                Some(dependents),
                            ));
                            needles.push(bstr::BString::from(link.get_target()));
                        }
                    }
                }
            }

            if package.as_root().is_some() {
                for (k, v) in package.get_dev_requires().iter() {
                    links
                        .entry(bstr::BString::from(&k.clone()[..]))
                        .or_insert(v.clone());
                }
            }

            for link in links.values() {
                for needle in &needles {
                    if link.get_target() == needle.as_bstr() {
                        let matches_constraint = constraint
                            .as_ref()
                            .is_none_or(|c| link.get_constraint().matches(c) != invert);
                        if constraint.is_none() || matches_constraint {
                            if packages_in_tree.contains(&BString::from(link.get_source()).clone())
                            {
                                results.push(DependentsEntry(package.clone(), link.clone(), None));
                                continue;
                            }
                            packages_in_tree.push(bstr::BString::from(link.get_source()));
                            let dependents = if recurse {
                                self.get_dependents(
                                    NeedleInput::Single(bstr::BString::from(link.get_source())),
                                    None,
                                    false,
                                    true,
                                    Some(packages_in_tree.clone()),
                                )?
                            } else {
                                vec![]
                            };
                            results.push(DependentsEntry(
                                package.clone(),
                                link.clone(),
                                Some(dependents),
                            ));
                        }
                    }
                }
            }

            if invert && needles.contains(&BString::from(&package.get_name()[..]).clone()) {
                for link in package.get_conflicts().values() {
                    for pkg in self
                        .find_packages(bstr::BStr::new(&*legacy_lossy(link.get_target())), None)?
                    {
                        let version =
                            SimpleConstraint::new("=", BString::from(&pkg.get_version()[..]), None);
                        if link.get_constraint().matches(&version.into()) == invert {
                            results.push(DependentsEntry(package.clone(), link.clone(), None));
                        }
                    }
                }
            }

            for link in package.get_conflicts().values() {
                if needles.contains(&BString::from(link.get_target()).clone()) {
                    for pkg in self
                        .find_packages(bstr::BStr::new(&*legacy_lossy(link.get_target())), None)?
                    {
                        let version =
                            SimpleConstraint::new("=", BString::from(&pkg.get_version()[..]), None);
                        if link.get_constraint().matches(&version.into()) == invert {
                            results.push(DependentsEntry(package.clone(), link.clone(), None));
                        }
                    }
                }
            }

            if invert
                && constraint.is_some()
                && needles.contains(&BString::from(&package.get_name()[..]).clone())
                && constraint.as_ref().unwrap().matches(
                    &SimpleConstraint::new("=", BString::from(&package.get_version()[..]), None)
                        .into(),
                )
            {
                'requires: for link in package.get_requires().values() {
                    if PlatformRepository::is_platform_package(link.get_target()) {
                        if self
                            .find_package(
                                bstr::BStr::new(&*legacy_lossy(link.get_target())),
                                FindPackageConstraint::Constraint(link.get_constraint().clone()),
                            )?
                            .is_some()
                        {
                            continue;
                        }

                        let platform_pkg = self.find_package(
                            bstr::BStr::new(&*legacy_lossy(link.get_target())),
                            FindPackageConstraint::String(bstr::BString::from("*".to_string())),
                        )?;
                        let description = platform_pkg
                            .as_ref()
                            .map(|p| bconcat!("but ", p.get_pretty_version(), " is installed"))
                            .unwrap_or_else(|| {
                                bstr::BString::from("but it is missing".to_string())
                            });
                        results.push(DependentsEntry(
                            package.clone(),
                            Link::new(
                                bstr::BString::from(&package.get_name()[..]),
                                bstr::BString::from(link.get_target()),
                                MatchAllConstraint::new(None).into(),
                                Some(bstr::BString::from(Link::TYPE_REQUIRE.to_string())),
                                bconcat!(link.get_pretty_constraint(), " ", description),
                            ),
                            None,
                        ));

                        continue;
                    }

                    for pkg in self.get_packages()? {
                        if !pkg
                            .get_names(true)
                            .contains(&BString::from(link.get_target()).clone())
                        {
                            continue;
                        }

                        let mut version: AnyConstraint =
                            SimpleConstraint::new("=", BString::from(&pkg.get_version()[..]), None)
                                .into();

                        if link.get_target() != pkg.get_name().as_bstr() {
                            let replaces_and_provides = array_merge_map(
                                (*pkg.get_replaces()).clone(),
                                (*pkg.get_provides()).clone(),
                            );
                            for prov in replaces_and_provides.values() {
                                if link.get_target() == prov.get_target() {
                                    version = prov.get_constraint().clone();
                                    break;
                                }
                            }
                        }

                        if !link.get_constraint().matches(&version) {
                            if let Some(root_pkg) = root_package.as_ref() {
                                let mut root_reqs: IndexMap<BString, Link> =
                                    (*root_pkg.get_requires()).clone();
                                for (k, v) in root_pkg.get_dev_requires().iter() {
                                    root_reqs
                                        .entry(bstr::BString::from(&k.clone()[..]))
                                        .or_insert(v.clone());
                                }
                                for root_req in root_reqs.values() {
                                    if pkg
                                        .get_names(true)
                                        .contains(&BString::from(root_req.get_target()).clone())
                                        && !root_req.get_constraint().matches(link.get_constraint())
                                    {
                                        results.push(DependentsEntry(
                                            package.clone(),
                                            link.clone(),
                                            None,
                                        ));
                                        results.push(DependentsEntry(
                                            root_pkg.clone(),
                                            root_req.clone(),
                                            None,
                                        ));
                                        continue 'requires;
                                    }
                                }

                                results.push(DependentsEntry(package.clone(), link.clone(), None));
                                results.push(DependentsEntry(
                                    root_pkg.clone(),
                                    Link::new(
                                        bstr::BString::from(&root_pkg.get_name()[..]),
                                        bstr::BString::from(link.get_target()),
                                        MatchAllConstraint::new(None).into(),
                                        Some(bstr::BString::from(
                                            Link::TYPE_DOES_NOT_REQUIRE.to_string(),
                                        )),
                                        bconcat!("but ", pkg.get_pretty_version(), " is installed"),
                                    ),
                                    None,
                                ));
                            } else {
                                results.push(DependentsEntry(package.clone(), link.clone(), None));
                            }
                        }

                        continue 'requires;
                    }
                }
            }
        }

        // ksort($results) - no-op for a numerically-indexed Vec
        Ok(results)
    }

    pub fn add_repository(&mut self, repository: RepositoryInterfaceHandle) {
        assert!(
            repository.is::<LockArrayRepository>()
                || repository.is_installed_repository_interface()
                || repository.is::<RootPackageRepository>()
                || repository.is::<PlatformRepository>(),
            "An InstalledRepository can not contain a repository of type {} ({})",
            repository
                .get_repo_name()
                .expect("getRepoName failed while reporting an invalid repository")
                .display_lossy(),
            repository
                .get_repo_name()
                .expect("getRepoName failed while reporting an invalid repository")
                .display_lossy()
        );

        self.inner.add_repository(repository);
    }
}

impl RepositoryInterface for InstalledRepository {
    fn count(&mut self) -> anyhow::Result<usize> {
        self.inner.count()
    }

    fn get_repo_name(&self) -> anyhow::Result<BString> {
        let names: Vec<BString> = self
            .inner
            .get_repositories()
            .iter()
            .map(|repo| repo.get_repo_name())
            .collect::<anyhow::Result<_>>()?;
        Ok(bconcat!("installed repo (", implode(", ", &names), ")"))
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        self.inner.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &BStr,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.inner.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &BStr,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.find_packages(name, constraint)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.inner.get_packages()
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<BString, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
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
        self.inner.search(query, mode, r#type)
    }

    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
        self.inner.get_providers(package_name)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
