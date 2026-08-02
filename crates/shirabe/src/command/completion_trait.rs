//! ref: composer/src/Composer/Command/CompletionTrait.php

use crate::command::BaseCommand;
use crate::console::input::SuggestedValues;
use crate::package::base_package;
use crate::repository::CompositeRepository;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RootPackageRepository;
use crate::repository::repository_interface::{SEARCH_NAME, SEARCH_VENDOR, SearchResult};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{PhpMixed, php_regex, preg_quote};

/// Adds completion to arguments and options.
///
/// PHP declares `use CompletionTrait;` per command with private methods; the blanket impl over
/// every `BaseCommand` (including `dyn BaseCommand`, which the suggestion closures receive as
/// `this`) is observationally equivalent.
pub trait CompletionTrait: BaseCommand {
    /// Suggestion values for "prefer-install" option
    fn suggest_prefer_install(&self) -> SuggestedValues {
        SuggestedValues::List(vec![
            "dist".to_string(),
            "source".to_string(),
            "auto".to_string(),
        ])
    }

    /// Suggest package names from root requirements.
    fn suggest_root_requirement(&self) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(|this, _input, _suggestions| {
            let composer = this.require_composer(None, None)?;
            let composer = crate::composer::composer_full(&composer);

            let package = composer.get_package();
            let mut names: Vec<String> = package.get_requires().keys().cloned().collect();
            names.extend(package.get_dev_requires().keys().cloned());
            Ok(names)
        }))
    }

    /// Suggest package names from installed.
    fn suggest_installed_package(
        &self,
        include_root_package: bool,
        include_platform_packages: bool,
    ) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(move |this, input, _suggestions| {
            let composer = this.require_composer(None, None)?;
            let composer = crate::composer::composer_full(&composer);
            let mut installed_repos: Vec<RepositoryInterfaceHandle> = vec![];

            if include_root_package {
                installed_repos.push(RepositoryInterfaceHandle::new(RootPackageRepository::new(
                    crate::package::RootPackageInterfaceHandle::dup(composer.get_package()),
                )));
            }

            let locker = composer.get_locker();
            if locker.borrow_mut().is_locked() {
                installed_repos.push(locker.borrow_mut().get_locked_repository(true)?.into());
            } else {
                installed_repos.push(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_local_repository(),
                );
            }

            let mut platform_hint: Vec<String> = vec![];
            if include_platform_packages {
                let mut platform_repo = if locker.borrow_mut().is_locked() {
                    let overrides: IndexMap<String, PhpMixed> = locker
                        .borrow_mut()
                        .get_platform_overrides()?
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::String(v)))
                        .collect();
                    PlatformRepository::new(vec![], overrides)?
                } else {
                    let platform_cfg = composer.get_config().borrow().get("platform");
                    let overrides: IndexMap<String, PhpMixed> = platform_cfg
                        .as_array()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();
                    PlatformRepository::new(vec![], overrides)?
                };
                if input.get_completion_value().is_empty() {
                    // to reduce noise, when no text is yet entered we list only two entries for ext- and lib- prefixes
                    let mut hints_to_find: IndexMap<String, i64> = IndexMap::from([
                        ("ext-".to_string(), 0),
                        ("lib-".to_string(), 0),
                        ("php".to_string(), 99),
                        ("composer".to_string(), 99),
                    ]);
                    // PHP: continue 2
                    'pkgs: for pkg in platform_repo.get_packages()? {
                        // PHP's inner foreach iterates a by-value copy of $hintsToFind while
                        // mutating the original.
                        for (hint_prefix, hint_count) in hints_to_find.clone() {
                            if pkg.get_name().starts_with(&hint_prefix) {
                                if hint_count == 0 || hint_count >= 99 {
                                    platform_hint.push(pkg.get_name());
                                    *hints_to_find.get_mut(&hint_prefix).unwrap() += 1;
                                } else if hint_count == 1 {
                                    hints_to_find.shift_remove(&hint_prefix);
                                    let name = pkg.get_name();
                                    let end = std::cmp::max(
                                        name.len() as i64 - 3,
                                        hint_prefix.len() as i64 + 1,
                                    );
                                    platform_hint.push(format!(
                                        "{}...",
                                        shirabe_php_shim::substr(&name, 0, Some(end))
                                    ));
                                }
                                continue 'pkgs;
                            }
                        }
                    }
                } else {
                    installed_repos.push(RepositoryInterfaceHandle::new(platform_repo));
                }
            }

            let mut installed_repo = InstalledRepository::new(installed_repos);

            let mut names: Vec<String> = installed_repo
                .get_packages()?
                .into_iter()
                .map(|package| package.get_name())
                .collect();
            names.extend(platform_hint);
            Ok(names)
        }))
    }

    /// Suggest package types from installed.
    fn suggest_installed_package_types(&self, include_root_package: bool) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(move |this, _input, _suggestions| {
            let composer = this.require_composer(None, None)?;
            let composer = crate::composer::composer_full(&composer);
            let mut installed_repos: Vec<RepositoryInterfaceHandle> = vec![];

            if include_root_package {
                installed_repos.push(RepositoryInterfaceHandle::new(RootPackageRepository::new(
                    crate::package::RootPackageInterfaceHandle::dup(composer.get_package()),
                )));
            }

            let locker = composer.get_locker();
            if locker.borrow_mut().is_locked() {
                installed_repos.push(locker.borrow_mut().get_locked_repository(true)?.into());
            } else {
                installed_repos.push(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_local_repository(),
                );
            }

            let mut installed_repo = InstalledRepository::new(installed_repos);

            // array_values(array_unique(array_map(getType, ...))) — array_unique keeps the
            // first occurrence in order.
            let mut types: Vec<String> = vec![];
            for package in installed_repo.get_packages()? {
                let r#type = package.get_type();
                if !types.contains(&r#type) {
                    types.push(r#type);
                }
            }
            Ok(types)
        }))
    }

    /// Suggest package names available on all configured repositories.
    ///
    /// PHP defaults `$max` to 99; callers pass it explicitly here.
    fn suggest_available_package(&self, max: i64) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(move |this, input, _suggestions| {
            if max < 1 {
                return Ok(vec![]);
            }

            let composer = this.require_composer(None, None)?;
            let composer = crate::composer::composer_full(&composer);
            let repository_manager = composer.get_repository_manager();
            let repository_manager = repository_manager.borrow();
            let mut repos = CompositeRepository::new(repository_manager.get_repositories().clone());

            let mut results: Vec<SearchResult> = vec![];
            let mut show_vendors = false;
            if !input.get_completion_value().contains('/') {
                results = repos.search(
                    format!("^{}", preg_quote(&input.get_completion_value(), None)),
                    SEARCH_VENDOR,
                    None,
                )?;
                show_vendors = true;
            }

            // if we get a single vendor, we expand it into its contents already
            if results.len() <= 1 {
                results = repos.search(
                    format!("^{}", preg_quote(&input.get_completion_value(), None)),
                    SEARCH_NAME,
                    None,
                )?;
                show_vendors = false;
            }

            // array_column($results, 'name')
            let results: Vec<String> = results.into_iter().map(|result| result.name).collect();

            if show_vendors {
                let mut results: Vec<String> = results
                    .into_iter()
                    .map(|name| format!("{}/", name))
                    .collect();

                // sort shorter results first to avoid auto-expanding the completion to a longer string than needed
                results.sort_by(|a, b| {
                    let len_a = a.len();
                    let len_b = b.len();
                    if len_a == len_b {
                        return a.cmp(b);
                    }
                    len_a.cmp(&len_b)
                });

                let mut pinned: Vec<String> = vec![];

                // ensure if the input is an exact match that it is always in the result set
                let completion_input = format!("{}/", input.get_completion_value());
                // PHP: array_search(..., true) + array_splice($results, $exactIndex, 1)
                if let Some(exact_index) = results.iter().position(|r| *r == completion_input) {
                    pinned.push(completion_input);
                    results.remove(exact_index);
                }

                let take = (max as usize).saturating_sub(pinned.len());
                pinned.extend(results.into_iter().take(take));
                return Ok(pinned);
            }

            Ok(results.into_iter().take(max as usize).collect())
        }))
    }

    /// Suggest package names available on all configured repositories or
    /// platform packages from the ones available on the currently-running PHP
    fn suggest_available_package_incl_platform(&self) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(|this, input, suggestions| {
            let matches = if Preg::is_match(
                php_regex!(r"{^(ext|lib|php)(-|$)|^com}"),
                &input.get_completion_value(),
            ) {
                this.suggest_platform_package()
                    .call(this, input, suggestions)?
            } else {
                vec![]
            };

            let mut merged = matches.clone();
            merged.extend(
                this.suggest_available_package(99 - matches.len() as i64)
                    .call(this, input, suggestions)?,
            );
            Ok(merged)
        }))
    }

    /// Suggest platform packages from the ones available on the currently-running PHP
    fn suggest_platform_package(&self) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(|this, input, _suggestions| {
            let composer = this.require_composer(None, None)?;
            let composer = crate::composer::composer_full(&composer);
            let platform_cfg = composer.get_config().borrow().get("platform");
            let overrides: IndexMap<String, PhpMixed> = platform_cfg
                .as_array()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let mut repos = PlatformRepository::new(vec![], overrides)?;

            let pattern =
                base_package::package_name_to_regexp(&format!("{}*", input.get_completion_value()));

            // array_filter(array_map(getName, ...), isMatch)
            let mut names: Vec<String> = vec![];
            for package in repos.get_packages()? {
                let name = package.get_name();
                if Preg::is_match(pattern.clone(), &name) {
                    names.push(name);
                }
            }
            Ok(names)
        }))
    }
}

impl<T: BaseCommand + ?Sized> CompletionTrait for T {}
