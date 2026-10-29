//! ref: composer/src/Composer/Package/Loader/RootPackageLoader.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::package::loader::{ArrayLoader, LoaderInterface, ValidatingArrayLoader};
use crate::package::version::{VersionGuesser, VersionGuesserInterface, VersionParser};
use crate::package::{RootPackage, STABILITIES, SUPPORTED_LINK_TYPES};
use crate::repository::{RepositoryFactory, RepositoryManager};
use crate::util::{Platform, ProcessExecutor};
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PhpMixed, RuntimeException, UnexpectedValueException, legacy_lossy,
    path_to_bytes, php_regex, preg_is_match, preg_match, preg_replace, preg_split, strtolower,
    trigger_error,
};

#[derive(Debug)]
pub struct RootPackageLoader {
    inner: ArrayLoader,
    manager: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    version_guesser: Box<dyn VersionGuesserInterface>,
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
}

impl RootPackageLoader {
    pub fn new(
        manager: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        parser: Option<VersionParser>,
        version_guesser: Option<Box<dyn VersionGuesserInterface>>,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> Self {
        let inner = ArrayLoader::new(parser, true);
        let version_guesser = version_guesser.unwrap_or_else(|| {
            let process_executor = ProcessExecutor::new(io.clone());
            process_executor.enable_async();
            Box::new(VersionGuesser::new(
                config.clone(),
                std::rc::Rc::new(std::cell::RefCell::new(process_executor)),
                inner.version_parser.clone(),
                io.clone(),
            ))
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
        config: IndexMap<BString, PhpMixed>,
        class: &str,
        cwd: Option<&str>,
    ) -> anyhow::Result<crate::package::PackageInterfaceHandle> {
        if class != "Composer\\Package\\RootPackage" {
            trigger_error(
                "The $class arg is deprecated, please reach out to Composer maintainers ASAP if you still need this.",
                E_USER_DEPRECATED,
            );
        }

        let mut config = config;

        if !config.contains_key(B("name")) {
            config.insert("name".into(), PhpMixed::string("__root__".to_string()));
        } else if let Some(err) = ValidatingArrayLoader::has_package_naming_error(
            &config[B("name")]
                .as_string()
                .unwrap_or_default()
                .to_str_lossy(),
            false,
        ) {
            return Err(RuntimeException::new(format!("Your package name {}", err)).into());
        }

        let mut auto_versioned = false;
        if !config.contains_key(B("version")) {
            let mut commit: Option<String> = None;

            if Platform::get_env("COMPOSER_ROOT_VERSION").is_some() {
                let version = self.version_guesser.get_root_version_from_env()?;
                config.insert("version".into(), PhpMixed::string(version));
            } else {
                let cwd_str = cwd.map(|s| s.to_string()).unwrap_or_else(|| {
                    legacy_lossy(path_to_bytes(&Platform::get_cwd(true).unwrap_or_default()))
                        .into_owned()
                });
                let version_data = self.version_guesser.guess_version(&config, &cwd_str)?;
                if let Some(data) = version_data {
                    config.insert(
                        "version".into(),
                        PhpMixed::string(data.pretty_version.clone().unwrap_or_default()),
                    );
                    config.insert(
                        "version_normalized".into(),
                        PhpMixed::string(data.version.clone().unwrap_or_default()),
                    );
                    commit = data.commit;
                }
            }

            if !config.contains_key(B("version")) {
                if let Some(ref io) = self.io {
                    let name = config[B("name")].as_string().unwrap_or_default();
                    let package_type = config
                        .get(B("type"))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    if name != "__root__" && package_type != "project" {
                        io.warning(&format!(
                            "Shirabe could not detect the root package ({}) version, defaulting to '1.0.0'. See https://getcomposer.org/root-version",
                            legacy_lossy(&name)
                        ), &[]);
                    }
                }
                config.insert("version".into(), PhpMixed::string("1.0.0".to_string()));
                auto_versioned = true;
            }

            if let Some(commit_hash) = commit {
                let mut source = IndexMap::new();
                source.insert("type".into(), PhpMixed::string(String::new()));
                source.insert("url".into(), PhpMixed::string(String::new()));
                source.insert("reference".into(), PhpMixed::string(commit_hash.clone()));
                config.insert("source".into(), PhpMixed::Array(source));

                let mut dist = IndexMap::new();
                dist.insert("type".into(), PhpMixed::string(String::new()));
                dist.insert("url".into(), PhpMixed::string(String::new()));
                dist.insert("reference".into(), PhpMixed::string(commit_hash));
                config.insert("dist".into(), PhpMixed::Array(dist));
            }
        }

        let package = self.inner.load(
            config.clone(),
            Some("Composer\\Package\\RootPackage".to_string()),
        )?;

        let real_package = if let Some(alias) = package.as_root_alias_package() {
            alias.get_alias_of()
        } else {
            package
                .as_root_package()
                .expect("Expecting a Composer\\Package\\RootPackage at this point")
        };

        if auto_versioned {
            real_package.replace_version(
                real_package.get_version(),
                RootPackage::DEFAULT_PRETTY_VERSION.to_string(),
            );
        }

        if let Some(min_stability) = config
            .get(B("minimum-stability"))
            .and_then(|v| v.as_string())
        {
            real_package.set_minimum_stability(
                VersionParser::normalize_stability(&min_stability.to_str_lossy())
                    .unwrap_or_default(),
            );
        }

        let mut aliases: Vec<IndexMap<String, String>> = vec![];
        let mut stability_flags: IndexMap<String, i64> = IndexMap::new();
        let mut references: IndexMap<String, String> = IndexMap::new();

        for link_type in ["require", "require-dev"] {
            if config.contains_key(B(link_type)) {
                // PHP dynamic dispatch: $realPackage->{'get'.ucfirst($linkInfo['method'])}()
                let parsed_links = match link_type {
                    "require" => real_package.get_requires(),
                    "require-dev" => real_package.get_dev_requires(),
                    _ => unreachable!(),
                };
                let mut links: IndexMap<String, String> = IndexMap::new();
                for link in parsed_links.values() {
                    links.insert(
                        link.get_target().to_string(),
                        legacy_lossy(&link.get_constraint().get_pretty_string()).into_owned(),
                    );
                }
                aliases = self.extract_aliases(&links, aliases);
                stability_flags = Self::extract_stability_flags(
                    &links,
                    &real_package.get_minimum_stability(),
                    stability_flags,
                );
                references = Self::extract_references(&links, references);

                let package_name =
                    legacy_lossy(config[B("name")].as_string().unwrap_or_default()).into_owned();
                if links.contains_key(&package_name) {
                    return Err(RuntimeException::new(format!(
                        "Root package '{}' cannot require itself in its composer.json\nDid you accidentally name your root package after an external package?",
                        package_name
                    )).into());
                }
            }
        }

        for link_type in SUPPORTED_LINK_TYPES.keys() {
            if let Some(section) = config.get(B(link_type))
                && let Some(section_map) = section.as_array()
            {
                for (link_name, _constraint) in section_map {
                    if let Some(err) =
                        // TODO(bytes): package names are validated as UTF-8 text, so non-UTF-8
                        // bytes are replaced.
                        ValidatingArrayLoader::has_package_naming_error(
                            &link_name.to_str_lossy(),
                            true,
                        )
                    {
                        return Err(RuntimeException::new(format!("{}.{}", link_type, err)).into());
                    }
                }
            }
        }

        real_package.set_aliases(aliases);
        real_package.set_stability_flags(stability_flags);
        real_package.set_references(references);

        if let Some(prefer_stable) = config.get(B("prefer-stable")).and_then(|v| v.as_opt()) {
            real_package.set_prefer_stable(prefer_stable.to_bool());
        }

        if let Some(pkg_config) = config.get(B("config")).and_then(|v| v.as_array()) {
            real_package.set_config(
                pkg_config
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        let repos = RepositoryFactory::default_repos(
            None,
            Some(self.config.clone()),
            Some(&mut *self.manager.borrow_mut()),
        )?;
        for (_, repo) in repos {
            self.manager.borrow_mut().add_repository(repo);
        }
        real_package.set_repositories(self.config.borrow().get_repositories());

        Ok(package)
    }

    fn extract_aliases(
        &self,
        requires: &IndexMap<String, String>,
        mut aliases: Vec<IndexMap<String, String>>,
    ) -> Vec<IndexMap<String, String>> {
        for (req_name, req_version) in requires {
            if let Some(m) = preg_match(
                php_regex!(r"{(?:^|\| *|, *)([^,\s#|]+)(?:#[^ ]+)? +as +([^,\s|]+)(?:$| *\|| *,)}"),
                req_version,
            ) {
                let m1 = legacy_lossy(m.get(1).unwrap_or_default()).into_owned();
                let m2 = legacy_lossy(m.get(2).unwrap_or_default()).into_owned();
                let mut alias = IndexMap::new();
                alias.insert(
                    "package".to_string(),
                    legacy_lossy(&strtolower(req_name)).into_owned(),
                );
                alias.insert(
                    "version".to_string(),
                    self.inner
                        .version_parser
                        .normalize(&m1, Some(req_version))
                        .unwrap_or_default(),
                );
                alias.insert("alias".to_string(), m2.clone());
                alias.insert(
                    "alias_normalized".to_string(),
                    self.inner
                        .version_parser
                        .normalize(&m2, Some(req_version))
                        .unwrap_or_default(),
                );
                aliases.push(alias);
            } else if req_version.contains(" as ") {
                return {
                    panic!(
                        "{}",
                        UnexpectedValueException::new(format!(
                                "Invalid alias definition in \"{}\": \"{}\". Aliases should be in the form \"exact-version as other-exact-version\".",
                                req_name, req_version
                            ))
                        .get_message()
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

            let or_split = preg_split(php_regex!(r"{\s*\|\|?\s*}"), req_version.trim());
            for or_constraint in &or_split {
                let and_split = shirabe_semver::split_and_constraints(or_constraint);
                for and_constraint in and_split {
                    constraints.push(legacy_lossy(&and_constraint).into_owned());
                }
            }

            let stability_names: Vec<&str> = stabilities.keys().copied().collect();
            let pattern = format!("{{^[^@]*?@({})$}}i", stability_names.join("|"));

            let mut matched = false;
            for constraint in &constraints {
                if let Some(m) = preg_match(&pattern, constraint) {
                    let name = legacy_lossy(&strtolower(req_name)).into_owned();
                    let m1 = legacy_lossy(m.get(1).unwrap_or_default()).into_owned();
                    let normalized_m1 = VersionParser::normalize_stability(&m1).unwrap_or_default();
                    let stability = stabilities[normalized_m1.as_str()];

                    if stability_flags.get(&name).is_some_and(|&f| f > stability) {
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
                    preg_replace(php_regex!(r"{^([^,\s@]+) as .+$}"), "$1", constraint);
                if preg_is_match(php_regex!(r"{^[^,\s@]+$}"), &req_version_stripped) {
                    let stability_name =
                        VersionParser::parse_stability(&legacy_lossy(&req_version_stripped));
                    if stability_name != "stable" {
                        let name = legacy_lossy(&strtolower(req_name)).into_owned();
                        let stability = stabilities[stability_name.as_str()];
                        if stability_flags.get(&name).is_some_and(|&f| f > stability)
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
            let req_version = legacy_lossy(&preg_replace(
                php_regex!(r"{^([^,\s@]+) as .+$}"),
                "$1",
                req_version,
            ))
            .into_owned();
            if let Some(m) = preg_match(php_regex!(r"{^[^,\s@]+?#([a-f0-9]+)$}"), &req_version)
                && VersionParser::parse_stability(&req_version) == "dev"
            {
                let name = legacy_lossy(&strtolower(req_name)).into_owned();
                references.insert(
                    name,
                    legacy_lossy(m.get(1).unwrap_or_default()).into_owned(),
                );
            }
        }
        references
    }
}
