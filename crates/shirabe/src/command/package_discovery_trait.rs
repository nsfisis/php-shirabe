//! ref: composer/src/Composer/Command/PackageDiscoveryTrait.php

use crate::io::io_interface;
use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{
    Exception, InvalidArgumentException, LogicException, PHP_EOL, PhpMixed, array_keys,
    array_slice, array_unshift, array_values, asort, count, explode, file_get_contents, implode,
    in_array, is_array, is_file, is_numeric, is_string, json_decode, levenshtein, sprintf, strlen,
    strpos, trim,
};

use crate::composer::PartialComposerHandle;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::IgnoreAllPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::repository::CompositeRepository;
use crate::repository::PlatformRepository;
use crate::repository::PlatformRepositoryHandle;
use crate::repository::RepositoryFactory;
use crate::repository::RepositorySet;
use crate::repository::{RepositoryInterface, SearchResult};
use crate::util::Filesystem;

/// @internal
pub trait PackageDiscoveryTrait {
    // PHP: private $repos; private $repositorySets;
    // TODO(phase-b): trait fields require an associated state struct in Rust; expose via accessors
    fn get_repos_mut(&mut self) -> &mut Option<CompositeRepository>;
    fn get_repository_sets_mut(&mut self) -> &mut IndexMap<String, RepositorySet>;

    // PHP: trait dependencies (provided by BaseCommand)
    fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;
    fn try_composer(&self) -> Option<PartialComposerHandle>;
    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> PartialComposerHandle;
    fn get_platform_requirement_filter(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> std::rc::Rc<
        dyn crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface,
    >;

    fn normalize_requirements(&self, requires: Vec<String>) -> Vec<IndexMap<String, String>>;

    fn get_repos(&mut self) -> &mut CompositeRepository {
        if self.get_repos_mut().is_none() {
            // PHP: array_merge([new PlatformRepository], RepositoryFactory::defaultReposWithDefaultManager($this->getIO()))
            let mut repos: Vec<crate::repository::RepositoryInterfaceHandle> = vec![
                // TODO(phase-b): PlatformRepository::new() signature
                crate::repository::RepositoryInterfaceHandle::new::<PlatformRepository>(todo!(
                    "PlatformRepository::new()"
                )),
            ];
            let io_owned: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = self.get_io();
            for (_, repo) in RepositoryFactory::default_repos_with_default_manager(io_owned)
                .unwrap()
                .into_iter()
            {
                repos.push(repo);
            }
            *self.get_repos_mut() = Some(CompositeRepository::new(repos));
        }

        self.get_repos_mut().as_mut().unwrap()
    }

    /// @param key-of<BasePackage::STABILITIES>|null $minimumStability
    fn get_repository_set(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        minimum_stability: Option<&str>,
    ) -> &RepositorySet {
        let key = minimum_stability.unwrap_or("default").to_string();

        if !self.get_repository_sets_mut().contains_key(&key) {
            let stability = minimum_stability
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.get_minimum_stability(input));
            let mut repository_set = RepositorySet::new(
                &stability,
                IndexMap::new(),
                vec![],
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            );
            // TODO(phase-b): self.get_repos() returns reference; add_repository takes ownership
            let repos = todo!("self.get_repos() owned/cloned for add_repository");
            let _ = repository_set.add_repository(repos);
            self.get_repository_sets_mut()
                .insert(key.clone(), repository_set);
        }

        self.get_repository_sets_mut().get(&key).unwrap()
    }

    /// @return key-of<BasePackage::STABILITIES>
    fn get_minimum_stability(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> String {
        if input.borrow().has_option("stability") {
            // @phpstan-ignore-line as InitCommand does have this option but not all classes using this trait do
            return VersionParser::normalize_stability(
                &input
                    .borrow()
                    .get_option("stability")
                    .as_string()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "stable".to_string()),
            )
            .unwrap_or_default();
        }

        // @phpstan-ignore-next-line as RequireCommand does not have the option above so this code is reachable there
        let file = Factory::get_composer_file().unwrap_or_default();
        if is_file(&file) && Filesystem::is_readable(&file) {
            let contents = file_get_contents(&file).unwrap_or_default();
            let composer = json_decode(&contents, true).unwrap_or(PhpMixed::Null);
            if is_array(&composer) {
                if let Some(arr) = composer.as_array() {
                    if let Some(ms) = arr.get("minimum-stability") {
                        if let Some(s) = ms.as_string() {
                            return VersionParser::normalize_stability(s).unwrap_or_default();
                        }
                    }
                }
            }
        }

        "stable".to_string()
    }

    /// @param array<string> $requires
    ///
    /// @return array<string>
    /// @throws \Exception
    fn determine_requirements(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        mut requires: Vec<String>,
        platform_repo: Option<&PlatformRepositoryHandle>,
        preferred_stability: &str,
        use_best_version_constraint: bool,
        fixed: bool,
    ) -> Result<Vec<String>> {
        if requires.len() > 0 {
            let requires_norm = self.normalize_requirements(requires.clone());
            let mut result: Vec<String> = vec![];
            let io = self.get_io();

            for mut requirement in requires_norm {
                if requirement.contains_key("version")
                    && Preg::is_match(
                        r"{^\d+(\.\d+)?$}",
                        requirement.get("version").map(|s| s.as_str()).unwrap_or(""),
                    )
                    .unwrap_or(false)
                {
                    io.write_error3(
                        &format!(
                            "<warning>The \"{}\" constraint for \"{}\" appears too strict and will likely not match what you want. See https://getcomposer.org/constraints</warning>",
                            requirement.get("version").map(|s| s.as_str()).unwrap_or(""),
                            requirement.get("name").map(|s| s.as_str()).unwrap_or(""),
                        ),
                        true,
                        io_interface::NORMAL,
                    );
                }

                if !requirement.contains_key("version") {
                    // determine the best version automatically
                    let (name, version): (String, String) = self
                        .find_best_version_and_name_for_package(
                            io.clone(),
                            input.clone(),
                            requirement.get("name").map(|s| s.as_str()).unwrap_or(""),
                            platform_repo,
                            preferred_stability,
                            fixed,
                        )?;

                    // replace package name from packagist.org
                    requirement.insert("name".to_string(), name);

                    if use_best_version_constraint {
                        requirement.insert("version".to_string(), version.clone());
                        io.write_error3(
                            &sprintf(
                                "Using version <info>%s</info> for <info>%s</info>",
                                &[
                                    PhpMixed::String(version),
                                    PhpMixed::String(
                                        requirement.get("name").cloned().unwrap_or_default(),
                                    ),
                                ],
                            ),
                            true,
                            io_interface::NORMAL,
                        );
                    } else {
                        requirement.insert("version".to_string(), "guess".to_string());
                    }
                }

                result.push(format!(
                    "{} {}",
                    requirement.get("name").map(|s| s.as_str()).unwrap_or(""),
                    requirement.get("version").map(|s| s.as_str()).unwrap_or(""),
                ));
            }

            return Ok(result);
        }

        let version_parser = VersionParser::new();

        // Collect existing packages
        let composer = self.try_composer();
        let composer_ref = composer.as_ref().map(|c| c.borrow_partial());
        let repository_manager = composer_ref
            .as_ref()
            .map(|c| c.get_repository_manager().clone());
        let repository_manager_ref = repository_manager.as_ref().map(|rm| rm.borrow());
        let installed_repo = repository_manager_ref
            .as_ref()
            .map(|rm| rm.get_local_repository());
        let mut existing_packages: Vec<String> = vec![];
        if let Some(repo) = &installed_repo {
            for package in repo.get_packages()? {
                existing_packages.push(package.get_name().to_string());
            }
        }
        // PHP: unset($composer, $installedRepo);
        drop(installed_repo);
        drop(repository_manager_ref);
        drop(repository_manager);
        drop(composer_ref);
        drop(composer);

        let io = self.get_io();
        loop {
            let package_input = io.ask("Search for a package: ".to_string(), PhpMixed::Null);
            let mut package = match package_input.as_string() {
                Some(s) => s.to_string(),
                None => break,
            };
            let mut matches: Vec<SearchResult> = self.get_repos().search(
                package.clone(),
                crate::repository::repository_interface::SEARCH_FULLTEXT,
                None,
            )?;

            if count(&PhpMixed::List(
                matches.iter().map(|_| Box::new(PhpMixed::Null)).collect(),
            )) > 0
            {
                // Remove existing packages from search results.
                matches.retain(|found_package| {
                    !in_array(
                        PhpMixed::String(found_package.name.clone()),
                        &PhpMixed::List(
                            existing_packages
                                .iter()
                                .map(|s| Box::new(PhpMixed::String(s.clone())))
                                .collect(),
                        ),
                        true,
                    )
                });
                // PHP: $matches = array_values($matches); — already a Vec in Rust
                let mut exact_match = false;
                for r#match in &matches {
                    if r#match.name == package {
                        exact_match = true;
                        break;
                    }
                }

                // no match, prompt which to pick
                if !exact_match {
                    let providers: IndexMap<String, crate::repository::ProviderInfo> =
                        self.get_repos().get_providers(package.clone())?;
                    if count(&PhpMixed::List(
                        providers.iter().map(|_| Box::new(PhpMixed::Null)).collect(),
                    )) > 0
                    {
                        // PHP: array_unshift($matches, ['name' => $package, 'description' => '']);
                        matches.insert(
                            0,
                            SearchResult {
                                name: package.clone(),
                                description: Some(String::new()),
                                abandoned: None,
                                url: None,
                            },
                        );
                    }

                    let mut choices: Vec<String> = vec![];
                    for (position, found_package) in matches.iter().enumerate() {
                        let mut abandoned = String::new();
                        if let Some(ai) = &found_package.abandoned {
                            let replacement = match ai {
                                crate::repository::AbandonedInfo::Replacement(r) => {
                                    sprintf("Use %s instead", &[PhpMixed::String(r.clone())])
                                }
                                crate::repository::AbandonedInfo::Abandoned => {
                                    "No replacement was suggested".to_string()
                                }
                            };
                            abandoned = sprintf(
                                "<warning>Abandoned. %s.</warning>",
                                &[PhpMixed::String(replacement)],
                            );
                        }

                        choices.push(sprintf(
                            " <info>%5s</info> %s %s",
                            &[
                                PhpMixed::String(format!("[{}]", position)),
                                PhpMixed::String(found_package.name.clone()),
                                PhpMixed::String(abandoned),
                            ],
                        ));
                    }

                    io.write_error3("", true, io_interface::NORMAL);
                    io.write_error3(
                        &sprintf(
                            "Found <info>%s</info> packages matching <info>%s</info>",
                            &[
                                PhpMixed::Int(matches.len() as i64),
                                PhpMixed::String(package.clone()),
                            ],
                        ),
                        true,
                        io_interface::NORMAL,
                    );
                    io.write_error3("", true, io_interface::NORMAL);

                    for choice in &choices {
                        io.write_error3(choice, true, io_interface::NORMAL);
                    }
                    io.write_error3("", true, io_interface::NORMAL);

                    let matches_clone = matches.clone();
                    let version_parser_clone = version_parser.clone();
                    let validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>> = Box::new(
                        move |selection_mixed: PhpMixed| -> anyhow::Result<PhpMixed> {
                            let selection = selection_mixed.as_string().unwrap_or("").to_string();
                            if "" == selection {
                                return Ok(PhpMixed::Bool(false));
                            }

                            if is_numeric(&PhpMixed::String(selection.clone())) {
                                let idx: usize = selection.parse().unwrap_or(0);
                                if let Some(p) = matches_clone.get(idx) {
                                    return Ok(PhpMixed::String(p.name.clone()));
                                }
                            }

                            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                            if Preg::is_match_strict_groups3(
                                r"{^\s*(?P<name>[\S/]+)(?:\s+(?P<version>\S+))?\s*$}",
                                &selection,
                                Some(&mut m),
                            )
                            .unwrap_or(false)
                            {
                                if let Some(v) =
                                    m.get(&CaptureKey::ByName("version".to_string())).cloned()
                                {
                                    // parsing `acme/example ~2.3`
                                    // validate version constraint
                                    version_parser_clone.parse_constraints(&v)?;

                                    return Ok(PhpMixed::String(format!(
                                        "{} {}",
                                        m.get(&CaptureKey::ByName("name".to_string()))
                                            .cloned()
                                            .unwrap_or_default(),
                                        v,
                                    )));
                                }

                                // parsing `acme/example`
                                return Ok(PhpMixed::String(
                                    m.get(&CaptureKey::ByName("name".to_string()))
                                        .cloned()
                                        .unwrap_or_default(),
                                ));
                            }

                            Err(Exception {
                                message: "Not a valid selection".to_string(),
                                code: 0,
                            }
                            .into())
                        },
                    );

                    package = io
                        .ask_and_validate(
                            "Enter package # to add, or the complete package name if it is not listed: ".to_string(),
                            validator,
                            Some(3),
                            PhpMixed::String(String::new()),
                        )?
                        .as_string()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                }

                // no constraint yet, determine the best version automatically
                if !package.is_empty() && strpos(&package, " ").is_none() {
                    let validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>> =
                        Box::new(|input_mixed: PhpMixed| -> anyhow::Result<PhpMixed> {
                            let input = trim(input_mixed.as_string().unwrap_or(""), None);
                            if strlen(&input) > 0 {
                                Ok(PhpMixed::String(input))
                            } else {
                                Ok(PhpMixed::Bool(false))
                            }
                        });

                    let constraint_mixed = io.ask_and_validate(
                        "Enter the version constraint to require (or leave blank to use the latest version): ".to_string(),
                        validator,
                        Some(3),
                        PhpMixed::String(String::new()),
                    )?;

                    let constraint: String = match &constraint_mixed {
                        PhpMixed::Bool(false) => {
                            let (_name, c): (String, String) = self
                                .find_best_version_and_name_for_package(
                                    io.clone(),
                                    input.clone(),
                                    &package,
                                    platform_repo,
                                    preferred_stability,
                                    fixed,
                                )?;

                            io.write_error3(
                                &sprintf(
                                    "Using version <info>%s</info> for <info>%s</info>",
                                    &[
                                        PhpMixed::String(c.clone()),
                                        PhpMixed::String(package.clone()),
                                    ],
                                ),
                                true,
                                io_interface::NORMAL,
                            );

                            c
                        }
                        PhpMixed::String(s) => s.clone(),
                        _ => String::new(),
                    };

                    package = format!("{} {}", package, constraint);
                }

                if !package.is_empty() {
                    requires.push(package.clone());
                    existing_packages.push(explode(" ", &package)[0].clone());
                }
            }
        }

        Ok(requires)
    }

    /// Given a package name, this determines the best version to use in the require key.
    ///
    /// This returns a version with the ~ operator prefixed when possible.
    ///
    /// @throws \InvalidArgumentException
    /// @return array{string, string}     name version
    fn find_best_version_and_name_for_package(
        &mut self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        name: &str,
        platform_repo: Option<&PlatformRepositoryHandle>,
        preferred_stability: &str,
        fixed: bool,
    ) -> Result<(String, String)> {
        // handle ignore-platform-reqs flag if present
        let platform_requirement_filter = if input.borrow().has_option("ignore-platform-reqs")
            && input.borrow().has_option("ignore-platform-req")
        {
            self.get_platform_requirement_filter(input.clone())
        } else {
            PlatformRequirementFilterFactory::ignore_nothing()
        };

        // find the latest version allowed in this repo set
        let repo_set = self.get_repository_set(input.clone(), None);
        // TODO(phase-b): VersionSelector::new takes owned RepositorySet; we have a shared reference
        let mut version_selector: VersionSelector =
            todo!("VersionSelector::new with owned repo_set");
        let effective_minimum_stability = self.get_minimum_stability(input.clone());

        let package = version_selector.find_best_candidate(
            name,
            None,
            preferred_stability,
            // TODO(phase-b): Box<dyn ...> cannot be cloned; original PHP shares reference
            Some(PlatformRequirementFilterFactory::ignore_nothing()),
            0,
            None,
            shirabe_php_shim::PhpMixed::Null,
        )?;

        if package.is_none() {
            // platform packages can not be found in the pool in versions other than the local platform's has
            // so if platform reqs are ignored we just take the user's word for it
            if platform_requirement_filter.is_ignored(name) {
                return Ok((name.to_string(), "*".to_string()));
            }

            // Check if it is a virtual package provided by others
            let providers = repo_set.get_providers(name)?;
            if count(&PhpMixed::List(
                providers.iter().map(|_| Box::new(PhpMixed::Null)).collect(),
            )) > 0
            {
                let mut constraint = "*".to_string();
                if input.borrow().is_interactive() {
                    let providers_count = providers.len();
                    let name_owned = name.to_string();
                    let validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>> =
                        Box::new(move |value_mixed: PhpMixed| -> anyhow::Result<PhpMixed> {
                            let value = value_mixed.as_string().unwrap_or("").to_string();
                            let parser = VersionParser::new();
                            parser.parse_constraints(&value)?;

                            Ok(PhpMixed::String(value))
                        });
                    constraint = self
                        .get_io()
                        .ask_and_validate(
                            format!(
                                "Package \"<info>{}</info>\" does not exist but is provided by {} packages. Which version constraint would you like to use? [<info>*</info>] ",
                                name_owned, providers_count,
                            ),
                            validator,
                            Some(3),
                            PhpMixed::String("*".to_string()),
                        )?
                        .as_string()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                }

                return Ok((name.to_string(), constraint));
            }

            // Check whether the package requirements were the problem
            let is_ignore_all = platform_requirement_filter
                .as_ref()
                .as_any()
                .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
                .is_some();
            if !is_ignore_all {
                let candidate = version_selector.find_best_candidate(
                    name,
                    None,
                    preferred_stability,
                    Some(PlatformRequirementFilterFactory::ignore_all()),
                    0,
                    None,
                    shirabe_php_shim::PhpMixed::Null,
                )?;
                if let Some(candidate) = candidate {
                    return Err(InvalidArgumentException {
                        message: sprintf(
                            &format!(
                                "Package %s has requirements incompatible with your PHP version, PHP extensions and Composer version{}",
                                self.get_platform_exception_details(
                                    candidate.clone(),
                                    platform_repo,
                                )?,
                            ),
                            &[PhpMixed::String(name.to_string())],
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
            // Check whether the minimum stability was the problem but the package exists
            let package_at_unacceptable = version_selector.find_best_candidate(
                name,
                None,
                preferred_stability,
                // TODO(phase-b): Box<dyn ...> cannot be cloned; reusing factory result
                Some(PlatformRequirementFilterFactory::ignore_nothing()),
                RepositorySet::ALLOW_UNACCEPTABLE_STABILITIES,
                None,
                shirabe_php_shim::PhpMixed::Null,
            )?;
            if let Some(package) = package_at_unacceptable {
                // we must first verify if a valid package would be found in a lower priority repository
                let all_repos_package = version_selector.find_best_candidate(
                    name,
                    None,
                    preferred_stability,
                    Some(PlatformRequirementFilterFactory::ignore_nothing()),
                    RepositorySet::ALLOW_SHADOWED_REPOSITORIES,
                    None,
                    shirabe_php_shim::PhpMixed::Null,
                )?;
                if let Some(all_repos_package) = all_repos_package {
                    return Err(InvalidArgumentException {
                        message: format!(
                            "Package {} exists in {} and {} which has a higher repository priority. The packages from the higher priority repository do not match your minimum-stability and are therefore not installable. That repository is canonical so the lower priority repo's packages are not installable. See https://getcomposer.org/repoprio for details and assistance.",
                            name,
                            all_repos_package
                                .get_repository()
                                .map(|r| r.get_repo_name())
                                .unwrap_or_default(),
                            package
                                .get_repository()
                                .map(|r| r.get_repo_name())
                                .unwrap_or_default(),
                        ),
                        code: 0,
                    }
                    .into());
                }

                return Err(InvalidArgumentException {
                    message: sprintf(
                        "Could not find a version of package %s matching your minimum-stability (%s). Require it with an explicit version constraint allowing its desired stability.",
                        &[
                            PhpMixed::String(name.to_string()),
                            PhpMixed::String(effective_minimum_stability.clone()),
                        ],
                    ),
                    code: 0,
                }
                .into());
            }
            // Check whether the PHP version was the problem for all versions
            if !is_ignore_all {
                let candidate = version_selector.find_best_candidate(
                    name,
                    None,
                    preferred_stability,
                    Some(PlatformRequirementFilterFactory::ignore_all()),
                    RepositorySet::ALLOW_UNACCEPTABLE_STABILITIES,
                    None,
                    shirabe_php_shim::PhpMixed::Null,
                )?;
                if let Some(candidate) = candidate {
                    let mut additional = String::new();
                    let no_match = version_selector.find_best_candidate(
                        name,
                        None,
                        preferred_stability,
                        Some(PlatformRequirementFilterFactory::ignore_all()),
                        0,
                        None,
                        shirabe_php_shim::PhpMixed::Null,
                    )?;
                    if no_match.is_none() {
                        additional = format!(
                            "{}{}Additionally, the package was only found with a stability of \"{}\" while your minimum stability is \"{}\".",
                            PHP_EOL,
                            PHP_EOL,
                            candidate.get_stability(),
                            effective_minimum_stability,
                        );
                    }

                    return Err(InvalidArgumentException {
                        message: sprintf(
                            &format!(
                                "Could not find package %s in any version matching your PHP version, PHP extensions and Composer version{}%s",
                                self.get_platform_exception_details(
                                    candidate.clone(),
                                    platform_repo,
                                )?,
                            ),
                            &[
                                PhpMixed::String(name.to_string()),
                                PhpMixed::String(additional),
                            ],
                        ),
                        code: 0,
                    }
                    .into());
                }
            }

            // Check for similar names/typos
            let similar = self.find_similar(name)?;
            if count(&PhpMixed::List(
                similar.iter().map(|_| Box::new(PhpMixed::Null)).collect(),
            )) > 0
            {
                if in_array(
                    PhpMixed::String(name.to_string()),
                    &PhpMixed::List(
                        similar
                            .iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    ),
                    true,
                ) {
                    return Err(InvalidArgumentException {
                        message: sprintf(
                            "Could not find package %s. It was however found via repository search, which indicates a consistency issue with the repository.",
                            &[PhpMixed::String(name.to_string())],
                        ),
                        code: 0,
                    }
                    .into());
                }

                if input.borrow().is_interactive() {
                    let result_mixed = io.select(
                        format!(
                            "<error>Could not find package {}.</error>\nPick one of these or leave empty to abort:",
                            name,
                        ),
                        similar.iter().map(|s| s.clone()).collect(),
                        PhpMixed::Bool(false),
                        PhpMixed::Int(1),
                        "No package named \"%s\" is installed.".to_string(),
                        false,
                    );
                    if let Some(idx_str) = result_mixed.as_string() {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            if let Some(selected) = similar.get(idx) {
                                return self.find_best_version_and_name_for_package(
                                    io.clone(),
                                    input,
                                    selected,
                                    platform_repo,
                                    preferred_stability,
                                    fixed,
                                );
                            }
                        }
                    }
                }

                return Err(InvalidArgumentException {
                    message: sprintf(
                        &format!(
                            "Could not find package %s.\n\nDid you mean {}?\n    %s",
                            if similar.len() > 1 {
                                "one of these"
                            } else {
                                "this"
                            },
                        ),
                        &[
                            PhpMixed::String(name.to_string()),
                            PhpMixed::String(implode("\n    ", &similar)),
                        ],
                    ),
                    code: 0,
                }
                .into());
            }

            return Err(InvalidArgumentException {
                message: sprintf(
                    "Could not find a matching version of package %s. Check the package spelling, your version constraint and that the package is available in a stability which matches your minimum-stability (%s).",
                    &[
                        PhpMixed::String(name.to_string()),
                        PhpMixed::String(effective_minimum_stability),
                    ],
                ),
                code: 0,
            }
            .into());
        }

        let package = package.unwrap();
        Ok((
            package.get_pretty_name().to_string(),
            if fixed {
                package.get_pretty_version().to_string()
            } else {
                version_selector.find_recommended_require_version(package.clone())?
            },
        ))
    }

    /// @return array<string>
    fn find_similar(&mut self, package: &str) -> Result<Vec<String>> {
        let results: Vec<SearchResult> = match (|| -> Result<Vec<SearchResult>> {
            if self.get_repos_mut().is_none() {
                return Err(LogicException {
                    message: "findSimilar was called before $this->repos was initialized"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            self.get_repos_mut()
                .as_mut()
                .unwrap()
                .search(package.to_string(), 0, None)
        })() {
            Ok(r) => r,
            Err(e) => {
                // PHP: if ($e instanceof \LogicException) throw $e;
                if e.downcast_ref::<LogicException>().is_some() {
                    return Err(e);
                }

                // ignore search errors
                return Ok(vec![]);
            }
        };
        let mut similar_packages: IndexMap<String, i64> = IndexMap::new();

        let composer_for_installed = self.require_composer(None, None);
        let composer_for_installed = composer_for_installed.borrow_partial();
        let repository_manager = composer_for_installed.get_repository_manager().clone();
        let repository_manager = repository_manager.borrow();
        let installed_repo = repository_manager.get_local_repository();

        for result in &results {
            // TODO(phase-b): installed_repo.find_package signature mismatch with FindPackageConstraint
            if installed_repo
                .find_package(
                    &result.name,
                    crate::repository::FindPackageConstraint::String("*".to_string()),
                )?
                .is_some()
            {
                // Ignore installed package
                continue;
            }
            similar_packages.insert(result.name.clone(), levenshtein(package, &result.name));
        }
        asort(&mut similar_packages);

        Ok(array_keys(&array_slice(&similar_packages, 0, Some(5))))
    }

    fn get_platform_exception_details(
        &self,
        candidate: PackageInterfaceHandle,
        platform_repo: Option<&PlatformRepositoryHandle>,
    ) -> anyhow::Result<String> {
        let mut details: Vec<String> = vec![];
        let platform_repo = match platform_repo {
            None => return Ok(String::new()),
            Some(p) => p,
        };
        let mut platform_repo = platform_repo.borrow_mut();

        for link in candidate.get_requires().values() {
            if !PlatformRepository::is_platform_package(link.get_target()) {
                continue;
            }
            let platform_pkg = platform_repo.find_package(
                link.get_target(),
                crate::repository::FindPackageConstraint::String("*".to_string()),
            )?;
            let platform_pkg = match platform_pkg {
                None => {
                    if platform_repo.is_platform_package_disabled(link.get_target()) {
                        details.push(format!(
                            "{} {} requires {} {} but it is disabled by your platform config. Enable it again with \"composer config platform.{} --unset\".",
                            candidate.get_pretty_name(),
                            candidate.get_pretty_version(),
                            link.get_target(),
                            link.get_pretty_constraint(),
                            link.get_target(),
                        ));
                    } else {
                        details.push(format!(
                            "{} {} requires {} {} but it is not present.",
                            candidate.get_pretty_name(),
                            candidate.get_pretty_version(),
                            link.get_target(),
                            link.get_pretty_constraint(),
                        ));
                    }
                    continue;
                }
                Some(p) => p,
            };
            if !link
                .get_constraint()
                .matches(&shirabe_semver::constraint::AnyConstraint::Simple(
                    shirabe_semver::constraint::SimpleConstraint::new(
                        "==".to_string(),
                        platform_pkg.get_version().to_string(),
                        None,
                    ),
                ))
            {
                let mut platform_pkg_version = platform_pkg.get_pretty_version().to_string();
                let platform_extra = platform_pkg.get_extra();
                let has_config_platform = platform_extra.contains_key("config.platform");
                let is_complete = platform_pkg.as_complete().is_some();
                if has_config_platform && is_complete {
                    // TODO(phase-b): platform_pkg.get_description() via CompletePackageInterface
                    platform_pkg_version = format!(
                        "{} ({})",
                        platform_pkg_version,
                        todo!("platform_pkg.get_description()")
                    );
                }
                details.push(format!(
                    "{} {} requires {} {} which does not match your installed version {}.",
                    candidate.get_pretty_name(),
                    candidate.get_pretty_version(),
                    link.get_target(),
                    link.get_pretty_constraint(),
                    platform_pkg_version,
                ));
            }
        }

        if count(&PhpMixed::List(
            details.iter().map(|_| Box::new(PhpMixed::Null)).collect(),
        )) == 0
        {
            return Ok(String::new());
        }

        Ok(format!(
            ":{}  - {}",
            PHP_EOL,
            implode(&format!("{}  - ", PHP_EOL), &details)
        ))
    }
}
