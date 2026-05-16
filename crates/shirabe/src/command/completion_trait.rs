//! ref: composer/src/Composer/Command/CompletionTrait.php

use crate::composer::Composer;
use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::root_package_repository::RootPackageRepository;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::completion::completion_input::CompletionInput;
use shirabe_php_shim::preg_quote;

pub trait CompletionTrait {
    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Composer;

    fn suggest_prefer_install(&self) -> Vec<String> {
        vec!["dist".to_string(), "source".to_string(), "auto".to_string()]
    }

    fn suggest_root_requirement(&self) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |_input: &CompletionInput| -> Vec<String> {
            let composer = self.require_composer(None, None);

            let requires: Vec<String> = composer
                .get_package()
                .get_requires()
                .keys()
                .cloned()
                .collect();
            let dev_requires: Vec<String> = composer
                .get_package()
                .get_dev_requires()
                .keys()
                .cloned()
                .collect();
            [requires, dev_requires].concat()
        })
    }

    fn suggest_installed_package(
        &self,
        include_root_package: bool,
        include_platform_packages: bool,
    ) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |input: &CompletionInput| -> Vec<String> {
            let composer = self.require_composer(None, None);
            let mut installed_repos: Vec<
                Box<dyn crate::repository::repository_interface::RepositoryInterface>,
            > = Vec::new();

            if include_root_package {
                installed_repos.push(Box::new(RootPackageRepository::new(
                    composer.get_package().clone(),
                )));
            }

            let locker = composer.get_locker();
            if locker.is_locked() {
                installed_repos.push(Box::new(locker.get_locked_repository(true)));
            } else {
                installed_repos.push(Box::new(
                    composer.get_repository_manager().get_local_repository(),
                ));
            }

            let mut platform_hint: Vec<String> = Vec::new();
            if include_platform_packages {
                let platform_repo = if locker.is_locked() {
                    PlatformRepository::new(vec![], locker.get_platform_overrides())
                } else {
                    PlatformRepository::new(vec![], composer.get_config().get("platform"))
                };
                if input.get_completion_value() == "" {
                    // to reduce noise, when no text is yet entered we list only two entries for ext- and lib- prefixes
                    let mut hints_to_find: indexmap::IndexMap<String, i64> =
                        indexmap::IndexMap::new();
                    hints_to_find.insert("ext-".to_string(), 0);
                    hints_to_find.insert("lib-".to_string(), 0);
                    hints_to_find.insert("php".to_string(), 99);
                    hints_to_find.insert("composer".to_string(), 99);

                    'pkg_loop: for pkg in platform_repo.get_packages() {
                        for (hint_prefix, hint_count) in hints_to_find.iter_mut() {
                            if pkg.get_name().starts_with(hint_prefix.as_str()) {
                                if *hint_count == 0 || *hint_count >= 99 {
                                    platform_hint.push(pkg.get_name().to_string());
                                    *hint_count += 1;
                                } else if *hint_count == 1 {
                                    hints_to_find.remove(hint_prefix);
                                    platform_hint.push(format!(
                                        "{}...",
                                        &pkg.get_name()[..pkg
                                            .get_name()
                                            .len()
                                            .saturating_sub(3)
                                            .max(hint_prefix.len() + 1)]
                                    ));
                                }
                                continue 'pkg_loop;
                            }
                        }
                    }
                } else {
                    installed_repos.push(Box::new(platform_repo));
                }
            }

            let installed_repo = InstalledRepository::new(installed_repos);

            let mut result: Vec<String> = installed_repo
                .get_packages()
                .iter()
                .map(|package| package.get_name().to_string())
                .collect();
            result.extend(platform_hint);
            result
        })
    }

    fn suggest_installed_package_types(
        &self,
        include_root_package: bool,
    ) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |_input: &CompletionInput| -> Vec<String> {
            let composer = self.require_composer(None, None);
            let mut installed_repos: Vec<
                Box<dyn crate::repository::repository_interface::RepositoryInterface>,
            > = Vec::new();

            if include_root_package {
                installed_repos.push(Box::new(RootPackageRepository::new(
                    composer.get_package().clone(),
                )));
            }

            let locker = composer.get_locker();
            if locker.is_locked() {
                installed_repos.push(Box::new(locker.get_locked_repository(true)));
            } else {
                installed_repos.push(Box::new(
                    composer.get_repository_manager().get_local_repository(),
                ));
            }

            let installed_repo = InstalledRepository::new(installed_repos);

            let mut types: Vec<String> = installed_repo
                .get_packages()
                .iter()
                .map(|package| package.get_type().to_string())
                .collect();
            types.sort();
            types.dedup();
            types
        })
    }

    fn suggest_available_package(
        &self,
        max: i64,
    ) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |input: &CompletionInput| -> Vec<String> {
            if max < 1 {
                return Vec::new();
            }

            let composer = self.require_composer(None, None);
            let repos =
                CompositeRepository::new(composer.get_repository_manager().get_repositories());

            let mut results: Vec<String>;
            let mut show_vendors = false;
            if !input.get_completion_value().contains('/') {
                let search_results = repos.search(
                    format!("^{}", preg_quote(input.get_completion_value(), None)),
                    RepositoryInterface::SEARCH_VENDOR,
                    None,
                );
                results = search_results.iter().map(|r| r.name.clone()).collect();
                show_vendors = true;
            } else {
                results = Vec::new();
            }

            // if we get a single vendor, we expand it into its contents already
            if results.len() <= 1 {
                let search_results = repos.search(
                    format!("^{}", preg_quote(input.get_completion_value(), None)),
                    RepositoryInterface::SEARCH_NAME,
                    None,
                );
                results = search_results.iter().map(|r| r.name.clone()).collect();
                show_vendors = false;
            }

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
                        a.cmp(b)
                    } else {
                        len_a.cmp(&len_b)
                    }
                });

                let mut pinned: Vec<String> = Vec::new();

                // ensure if the input is an exact match that it is always in the result set
                let completion_input = format!("{}/", input.get_completion_value());
                if let Some(exact_index) = results.iter().position(|x| x == &completion_input) {
                    pinned.push(completion_input);
                    results.remove(exact_index);
                }

                let take_count = (max as usize).saturating_sub(pinned.len());
                let mut final_results = pinned;
                final_results.extend(results.into_iter().take(take_count));
                return final_results;
            }

            results.into_iter().take(max as usize).collect()
        })
    }

    fn suggest_available_package_incl_platform(
        &self,
    ) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |input: &CompletionInput| -> Vec<String> {
            let matches =
                if Preg::is_match(r"{^(ext|lib|php)(-|$)|^com}", input.get_completion_value()) {
                    self.suggest_platform_package()(input)
                } else {
                    Vec::new()
                };

            let max = 99i64 - matches.len() as i64;
            let mut result = matches;
            result.extend(self.suggest_available_package(max)(input));
            result
        })
    }

    fn suggest_platform_package(&self) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |input: &CompletionInput| -> Vec<String> {
            let repos = PlatformRepository::new(
                vec![],
                self.require_composer(None, None)
                    .get_config()
                    .get("platform"),
            );

            let pattern =
                BasePackage::package_name_to_regexp(&format!("{}*", input.get_completion_value()));

            repos
                .get_packages()
                .iter()
                .map(|package| package.get_name().to_string())
                .filter(|name| Preg::is_match(&pattern, name))
                .collect()
        })
    }
}
