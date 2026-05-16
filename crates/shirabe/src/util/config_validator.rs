//! ref: composer/src/Composer/Util/ConfigValidator.php

use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::json::json_validation_exception::JsonValidationException;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::invalid_package_exception::InvalidPackageException;
use crate::package::loader::validating_array_loader::ValidatingArrayLoader;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::spdx_licenses::spdx_licenses::SpdxLicenses;
use shirabe_external_packages::seld::json_lint::duplicate_key_exception::DuplicateKeyException;
use shirabe_external_packages::seld::json_lint::json_parser::JsonParser;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct ConfigValidator {
    io: Box<dyn IOInterface>,
}

impl ConfigValidator {
    pub const CHECK_VERSION: i64 = 1;

    pub fn new(io: Box<dyn IOInterface>) -> Self {
        Self { io }
    }

    pub fn validate(
        &self,
        file: &str,
        array_loader_validation_flags: i64,
        flags: i64,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut errors: Vec<String> = Vec::new();
        let mut publish_errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // validate json schema
        let mut lax_valid = false;
        let mut manifest: Option<IndexMap<String, PhpMixed>> = None;

        let json = JsonFile::new(file.to_string(), None, Some(&*self.io));
        let schema_result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            manifest = Some(json.read()?);
            json.validate_schema(Some(JsonFile::LAX_SCHEMA))?;
            lax_valid = true;
            json.validate_schema(None)?;
            Ok(())
        })();

        match schema_result {
            Ok(()) => {}
            Err(e) => {
                if let Some(validation_e) = e.downcast_ref::<JsonValidationException>() {
                    for message in validation_e.get_errors() {
                        if lax_valid {
                            publish_errors.push(message.clone());
                        } else {
                            errors.push(message.clone());
                        }
                    }
                } else {
                    errors.push(e.to_string());

                    return (errors, publish_errors, warnings);
                }
            }
        }

        if manifest.is_some() {
            let json_parser = JsonParser::new();
            let contents = shirabe_php_shim::file_get_contents(file).unwrap_or_default();
            let parse_result = json_parser.parse(&contents, JsonParser::DETECT_KEY_CONFLICTS);
            match parse_result {
                Ok(_) => {}
                Err(e) => {
                    if let Some(dup_e) = e.downcast_ref::<DuplicateKeyException>() {
                        let details = dup_e.get_details();
                        warnings.push(format!(
                            "Key {} is a duplicate in {} at line {}",
                            details["key"], file, details["line"]
                        ));
                    }
                }
            }
        }

        let manifest = match manifest {
            Some(m) => m,
            None => return (errors, publish_errors, warnings),
        };

        // validate actual data
        if manifest
            .get("license")
            .map_or(true, |v| matches!(v, PhpMixed::Null))
            || !manifest.contains_key("license")
        {
            warnings.push("No license specified, it is recommended to do so. For closed-source software you may use \"proprietary\" as license.".to_string());
        } else {
            let license_val = manifest.get("license").unwrap();
            let licenses: Vec<String> = match license_val {
                PhpMixed::String(s) => vec![s.clone()],
                PhpMixed::List(list) => list
                    .iter()
                    .filter_map(|v| {
                        if let PhpMixed::String(s) = v.as_ref() {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            };

            // strip proprietary since it's not a valid SPDX identifier, but is accepted by composer
            let licenses: Vec<String> = licenses
                .into_iter()
                .filter(|l| l != "proprietary")
                .collect();

            let license_validator = SpdxLicenses::new();
            for license in &licenses {
                let spdx_license = license_validator.get_license_by_identifier(license);
                if let Some(spdx_license) = spdx_license {
                    if spdx_license[3] {
                        if Preg::is_match(r"{^[AL]?GPL-[123](\.[01])?\+$}i", license) {
                            warnings.push(format!(
                                "License \"{}\" is a deprecated SPDX license identifier, use \"{}-or-later\" instead",
                                license,
                                license.replace('+', "")
                            ));
                        } else if Preg::is_match(r"{^[AL]?GPL-[123](\.[01])?$}i", license) {
                            warnings.push(format!(
                                "License \"{}\" is a deprecated SPDX license identifier, use \"{}-only\" or \"{}-or-later\" instead",
                                license, license, license
                            ));
                        } else {
                            warnings.push(format!(
                                "License \"{}\" is a deprecated SPDX license identifier, see https://spdx.org/licenses/",
                                license
                            ));
                        }
                    }
                }
            }
        }

        if (flags & Self::CHECK_VERSION != 0) && manifest.contains_key("version") {
            warnings.push("The version field is present, it is recommended to leave it out if the package is published on Packagist.".to_string());
        }

        if let Some(PhpMixed::String(name)) = manifest.get("name") {
            if !name.is_empty() && Preg::is_match(r"{[A-Z]}", name) {
                let suggest_name = Preg::replace(
                    r"{(?:([a-z])([A-Z])|([A-Z])([A-Z][a-z]))}",
                    r"\1\3-\2\4",
                    name,
                );
                let suggest_name = suggest_name.to_lowercase();

                publish_errors.push(format!(
                    "Name \"{}\" does not match the best practice (e.g. lower-cased/with-dashes). We suggest using \"{}\" instead. As such you will not be able to submit it to Packagist.",
                    name, suggest_name
                ));
            }
        }

        if let Some(PhpMixed::String(t)) = manifest.get("type") {
            if !t.is_empty() && t == "composer-installer" {
                warnings.push("The package type 'composer-installer' is deprecated. Please distribute your custom installers as plugins from now on. See https://getcomposer.org/doc/articles/plugins.md for plugin documentation.".to_string());
            }
        }

        // check for require-dev overrides
        if let (Some(PhpMixed::Array(require)), Some(PhpMixed::Array(require_dev))) =
            (manifest.get("require"), manifest.get("require-dev"))
        {
            let require_overrides: Vec<String> = require
                .keys()
                .filter(|k| require_dev.contains_key(*k))
                .cloned()
                .collect();

            if !require_overrides.is_empty() {
                let plural = if require_overrides.len() > 1 {
                    "are"
                } else {
                    "is"
                };
                warnings.push(format!(
                    "{} {} required both in require and require-dev, this can lead to unexpected behavior",
                    require_overrides.join(", "),
                    plural
                ));
            }
        }

        // check for meaningless provide/replace satisfying requirements
        for link_type in &["provide", "replace"] {
            if let Some(PhpMixed::Array(link_map)) = manifest.get(*link_type) {
                for require_type in &["require", "require-dev"] {
                    if let Some(PhpMixed::Array(require_map)) = manifest.get(*require_type) {
                        for provide in link_map.keys() {
                            if require_map.contains_key(provide) {
                                warnings.push(format!(
                                    "The package {} in {} is also listed in {} which satisfies the requirement. Remove it from {} if you wish to install it.",
                                    provide, require_type, link_type, link_type
                                ));
                            }
                        }
                    }
                }
            }
        }

        // check for commit references
        let require = match manifest.get("require") {
            Some(PhpMixed::Array(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        let require_dev = match manifest.get("require-dev") {
            Some(PhpMixed::Array(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        let mut packages: IndexMap<String, Box<PhpMixed>> = require;
        packages.extend(require_dev);
        for (package, version) in &packages {
            if let PhpMixed::String(version_str) = version.as_ref() {
                if Preg::is_match(r"{#}", version_str) {
                    warnings.push(format!(
                        "The package \"{}\" is pointing to a commit-ref, this is bad practice and can cause unforeseen issues.",
                        package
                    ));
                }
            }
        }

        // report scripts-descriptions for non-existent scripts
        let scripts_descriptions = match manifest.get("scripts-descriptions") {
            Some(PhpMixed::Array(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        let scripts = match manifest.get("scripts") {
            Some(PhpMixed::Array(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        for (script_name, _) in &scripts_descriptions {
            if !scripts.contains_key(script_name) {
                warnings.push(format!(
                    "Description for non-existent script \"{}\" found in \"scripts-descriptions\"",
                    script_name
                ));
            }
        }

        // report scripts-aliases for non-existent scripts
        let script_aliases = match manifest.get("scripts-aliases") {
            Some(PhpMixed::Array(m)) => m.clone(),
            _ => IndexMap::new(),
        };
        for (script_name, _) in &script_aliases {
            if !scripts.contains_key(script_name) {
                warnings.push(format!(
                    "Aliases for non-existent script \"{}\" found in \"scripts-aliases\"",
                    script_name
                ));
            }
        }

        // check for empty psr-0/psr-4 namespace prefixes
        if let Some(PhpMixed::Array(autoload)) = manifest.get("autoload") {
            if let Some(PhpMixed::Array(psr0)) = autoload.get("psr-0").map(|v| v.as_ref()) {
                if psr0.contains_key("") {
                    warnings.push("Defining autoload.psr-0 with an empty namespace prefix is a bad idea for performance".to_string());
                }
            }
            if let Some(PhpMixed::Array(psr4)) = autoload.get("psr-4").map(|v| v.as_ref()) {
                if psr4.contains_key("") {
                    warnings.push("Defining autoload.psr-4 with an empty namespace prefix is a bad idea for performance".to_string());
                }
            }
        }

        let loader = ValidatingArrayLoader::new(
            ArrayLoader::new(),
            true,
            None,
            array_loader_validation_flags,
        );
        let mut manifest_for_load = manifest.clone();
        if !manifest_for_load.contains_key("version") {
            manifest_for_load.insert("version".to_string(), PhpMixed::String("1.0.0".to_string()));
        }
        if !manifest_for_load.contains_key("name") {
            manifest_for_load.insert(
                "name".to_string(),
                PhpMixed::String("dummy/dummy".to_string()),
            );
        }
        match loader.load(manifest_for_load) {
            Ok(_) => {}
            Err(e) => {
                if let Some(invalid_e) = e.downcast_ref::<InvalidPackageException>() {
                    errors.extend_from_slice(invalid_e.get_errors());
                }
            }
        }

        warnings.extend_from_slice(loader.get_warnings());

        (errors, publish_errors, warnings)
    }
}
