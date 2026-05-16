//! ref: composer/src/Composer/Package/Loader/RootPackageLoader.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{strtolower, ucfirst, LogicException, RuntimeException, UnexpectedValueException};

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::{BasePackage, STABILITIES, SUPPORTED_LINK_TYPES};
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::validating_array_loader::ValidatingArrayLoader;
use crate::package::package_interface::PackageInterface;
use crate::package::root_alias_package::RootAliasPackage;
use crate::package::root_package::RootPackage;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::repository::repository_factory::RepositoryFactory;
use crate::repository::repository_manager::RepositoryManager;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct RootPackageLoader {
    inner: ArrayLoader,
    manager: RepositoryManager,
    config: Config,
    version_guesser: VersionGuesser,
    io: Option<Box<dyn IOInterface>>,
}

impl RootPackageLoader {
    pub fn new(
        manager: RepositoryManager,
        config: Config,
        parser: Option<VersionParser>,
        version_guesser: Option<VersionGuesser>,
        io: Option<Box<dyn IOInterface>>,
    ) -> Self {
        let inner = ArrayLoader::new(parser);
        let version_guesser = version_guesser.unwrap_or_else(|| {
            let mut process_executor = ProcessExecutor::new(io.as_deref());
            process_executor.enable_async();
            VersionGuesser::new(&config, process_executor, inner.version_parser.clone())
        });
        Self {
            inner,
            manager,
            config,
            version_guesser,
            io,
        }
    }

    pub fn load(
        &mut self,
        config: IndexMap<String, Box<shirabe_php_shim::PhpMixed>>,
        class: &str,
        cwd: Option<&str>,
    ) -> anyhow::Result<Box<dyn PackageInterface>> {
        if class != "Composer\\Package\\RootPackage" {
            shirabe_php_shim::trigger_error(
                "The $class arg is deprecated, please reach out to Composer maintainers ASAP if you still need this.",
                shirabe_php_shim::E_USER_DEPRECATED,
            );
        }

        let mut config = config;

        if !config.contains_key("name") {
            config.insert(
                "name".to_string(),
                Box::new(shirabe_php_shim::PhpMixed::String("__root__".to_string())),
            );
        } else if let Some(err) = ValidatingArrayLoader::has_package_naming_error(
            config["name"].as_string().unwrap_or(""),
            false,
        ) {
            return Err(anyhow::anyhow!(RuntimeException {
                message: format!("Your package name {}", err),
                code: 0,
            }));
        }

        let mut auto_versioned = false;
        if !config.contains_key("version") {
            let mut commit: Option<String> = None;

            if Platform::get_env("COMPOSER_ROOT_VERSION").is_some() {
                let version = self.version_guesser.get_root_version_from_env();
                config.insert(
                    "version".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(version)),
                );
            } else {
                let cwd_str = cwd.map(|s| s.to_string()).unwrap_or_else(|| Platform::get_cwd(true));
                let version_data = self.version_guesser.guess_version(&config, &cwd_str);
                if let Some(data) = version_data {
                    config.insert(
                        "version".to_string(),
                        Box::new(shirabe_php_shim::PhpMixed::String(data.pretty_version.clone())),
                    );
                    config.insert(
                        "version_normalized".to_string(),
                        Box::new(shirabe_php_shim::PhpMixed::String(data.version.clone())),
                    );
                    commit = data.commit;
                }
            }

            if !config.contains_key("version") {
                if let Some(ref io) = self.io {
                    let name = config["name"].as_string().unwrap_or("");
                    let package_type = config
                        .get("type")
                        .and_then(|v| v.as_string())
                        .unwrap_or("");
                    if name != "__root__" && package_type != "project" {
                        io.warning(&format!(
                            "Composer could not detect the root package ({}) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version",
                            name
                        ));
                    }
                }
                config.insert(
                    "version".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String("1.0.0".to_string())),
                );
                auto_versioned = true;
            }

            if let Some(commit_hash) = commit {
                let mut source = IndexMap::new();
                source.insert(
                    "type".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(String::new())),
                );
                source.insert(
                    "url".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(String::new())),
                );
                source.insert(
                    "reference".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(commit_hash.clone())),
                );
                config.insert(
                    "source".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::Array(source)),
                );

                let mut dist = IndexMap::new();
                dist.insert(
                    "type".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(String::new())),
                );
                dist.insert(
                    "url".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(String::new())),
                );
                dist.insert(
                    "reference".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::String(commit_hash)),
                );
                config.insert(
                    "dist".to_string(),
                    Box::new(shirabe_php_shim::PhpMixed::Array(dist)),
                );
            }
        }

        let package = self.inner.load(config.clone(), "Composer\\Package\\RootPackage")?;

        let real_package: &mut RootPackage = if let Some(alias_pkg) =
            package.as_any_mut().downcast_mut::<RootAliasPackage>()
        {
            alias_pkg
                .get_alias_of_mut()
                .as_any_mut()
                .downcast_mut::<RootPackage>()
                .ok_or_else(|| {
                    anyhow::anyhow!(LogicException {
                        message: "Expecting a Composer\\Package\\RootPackage at this point"
                            .to_string(),
                        code: 0,
                    })
                })?
        } else if let Some(root_pkg) = package.as_any_mut().downcast_mut::<RootPackage>() {
            root_pkg
        } else {
            return Err(anyhow::anyhow!(LogicException {
                message: "Expecting a Composer\\Package\\RootPackage at this point".to_string(),
                code: 0,
            }));
        };

        if auto_versioned {
            real_package
                .replace_version(real_package.get_version().to_string(), RootPackage::DEFAULT_PRETTY_VERSION.to_string());
        }

        if let Some(min_stability) = config
            .get("minimum-stability")
            .and_then(|v| v.as_string())
        {
            real_package
                .set_minimum_stability(VersionParser::normalize_stability(min_stability).to_string());
        }

        let mut aliases: Vec<IndexMap<String, String>> = vec![];
        let mut stability_flags: IndexMap<String, i64> = IndexMap::new();
        let mut references: IndexMap<String, String> = IndexMap::new();

        for link_type in ["require", "require-dev"] {
            if config.contains_key(link_type) {
                let link_info = &SUPPORTED_LINK_TYPES[link_type];
                let method = format!("get_{}", link_info.method);
                let links: IndexMap<String, String> = real_package
                    .call_get_links_method(&method)
                    .iter()
                    .map(|(target, link)| {
                        (
                            target.clone(),
                            link.get_constraint()
                                .map(|c| c.get_pretty_string().to_string())
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                aliases = self.extract_aliases(&links, aliases);
                stability_flags = Self::extract_stability_flags(
                    &links,
                    real_package.get_minimum_stability(),
                    stability_flags,
                );
                references = Self::extract_references(&links, references);

                let package_name = config["name"].as_string().unwrap_or("").to_string();
                if links.contains_key(&package_name) {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "Root package '{}' cannot require itself in its composer.json\nDid you accidentally name your root package after an external package?",
                            package_name
                        ),
                        code: 0,
                    }));
                }
            }
        }

        for link_type in SUPPORTED_LINK_TYPES.keys() {
            if let Some(section) = config.get(*link_type) {
                if let Some(section_map) = section.as_array() {
                    for (link_name, _constraint) in section_map {
                        if let Some(err) =
                            ValidatingArrayLoader::has_package_naming_error(link_name, true)
                        {
                            return Err(anyhow::anyhow!(RuntimeException {
                                message: format!("{}.{}", link_type, err),
                                code: 0,
                            }));
                        }
                    }
                }
            }
        }

        real_package.set_aliases(aliases);
        real_package.set_stability_flags(stability_flags);
        real_package.set_references(references);

        if let Some(prefer_stable) = config.get("prefer-stable").and_then(|v| v.as_bool()) {
            real_package.set_prefer_stable(prefer_stable);
        }

        if let Some(pkg_config) = config.get("config").and_then(|v| v.as_array()) {
            real_package.set_config(
                pkg_config
                    .iter()
                    .map(|(k, v)| (k.clone(), (**v).clone()))
                    .collect(),
            );
        }

        let repos = RepositoryFactory::default_repos(None, &self.config, &mut self.manager)?;
        for repo in repos {
            self.manager.add_repository(repo);
        }
        real_package.set_repositories(self.config.get_repositories());

        Ok(package)
    }

    fn extract_aliases(
        &self,
        requires: &IndexMap<String, String>,
        mut aliases: Vec<IndexMap<String, String>>,
    ) -> Vec<IndexMap<String, String>> {
        for (req_name, req_version) in requires {
            if let Some(m) = Preg::is_match_strict_groups(
                r"(?:^|\| *|, *)([^,\s#|]+)(?:#[^ ]+)? +as +([^,\s|]+)(?:$| *\|| *,)",
                req_version,
            )
            .unwrap_or(None)
            {
                let mut alias = IndexMap::new();
                alias.insert("package".to_string(), strtolower(req_name));
                alias.insert(
                    "version".to_string(),
                    self.inner
                        .version_parser
                        .normalize(&m[1], req_version)
                        .unwrap_or_default(),
                );
                alias.insert("alias".to_string(), m[2].clone());
                alias.insert(
                    "alias_normalized".to_string(),
                    self.inner
                        .version_parser
                        .normalize(&m[2], req_version)
                        .unwrap_or_default(),
                );
                aliases.push(alias);
            } else if req_version.contains(" as ") {
                return {
                    panic!(
                        "{}",
                        UnexpectedValueException {
                            message: format!(
                                "Invalid alias definition in \"{}\": \"{}\". Aliases should be in the form \"exact-version as other-exact-version\".",
                                req_name, req_version
                            ),
                            code: 0,
                        }
                        .message
                    )
                };
            }
        }
        aliases
    }

    pub fn extract_stability_flags(
        requires: &IndexMap<String, String>,
        minimum_stability: &str,
        mut stability_flags: IndexMap<String, i64>,
    ) -> IndexMap<String, i64> {
        let stabilities = &*STABILITIES;
        let minimum_stability_val = stabilities[minimum_stability];

        for (req_name, req_version) in requires {
            let mut constraints: Vec<String> = vec![];

            let or_split = Preg::split(r"\s*\|\|?\s*", req_version.trim()).unwrap_or_default();
            for or_constraint in &or_split {
                let and_split = Preg::split(
                    r"(?<!^|as|[=>< ,]) *(?<!-)[, ](?!-) *(?!,|as|$)",
                    or_constraint,
                )
                .unwrap_or_default();
                for and_constraint in and_split {
                    constraints.push(and_constraint);
                }
            }

            let stability_names: Vec<&str> = stabilities.keys().copied().collect();
            let pattern = format!(
                "^[^@]*?@({})$",
                stability_names.join("|")
            );

            let mut matched = false;
            for constraint in &constraints {
                if let Some(Some(m)) =
                    Preg::is_match_strict_groups(&pattern, constraint).ok()
                {
                    let name = strtolower(req_name);
                    let stability = stabilities[VersionParser::normalize_stability(&m[1])];

                    if stability_flags.get(&name).copied().unwrap_or(i64::MAX) > stability {
                        continue;
                    }
                    stability_flags.insert(name, stability);
                    matched = true;
                }
            }

            if matched {
                continue;
            }

            for constraint in &constraints {
                let req_version_stripped =
                    Preg::replace(r"^([^,\s@]+) as .+$", "$1", constraint).unwrap_or_default();
                if Preg::is_match(r"^[^,\s@]+$", &req_version_stripped).unwrap_or(false) {
                    let stability_name = VersionParser::parse_stability(&req_version_stripped);
                    if stability_name != "stable" {
                        let name = strtolower(req_name);
                        let stability = stabilities[stability_name];
                        if stability_flags.get(&name).copied().unwrap_or(i64::MAX) > stability
                            || minimum_stability_val > stability
                        {
                            continue;
                        }
                        stability_flags.insert(name, stability);
                    }
                }
            }
        }

        stability_flags
    }

    pub fn extract_references(
        requires: &IndexMap<String, String>,
        mut references: IndexMap<String, String>,
    ) -> IndexMap<String, String> {
        for (req_name, req_version) in requires {
            let req_version =
                Preg::replace(r"^([^,\s@]+) as .+$", "$1", req_version).unwrap_or_default();
            if let Some(Some(m)) =
                Preg::is_match_strict_groups(r"^[^,\s@]+?#([a-f0-9]+)$", &req_version).ok()
            {
                if VersionParser::parse_stability(&req_version) == "dev" {
                    let name = strtolower(req_name);
                    references.insert(name, m[1].clone());
                }
            }
        }
        references
    }
}
