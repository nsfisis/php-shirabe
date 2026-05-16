//! ref: composer/src/Composer/Compiler.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::ca_bundle::ca_bundle::CaBundle;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::seld::phar_utils::linter::Linter;
use shirabe_external_packages::seld::phar_utils::timestamps::Timestamps;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_external_packages::symfony::component::finder::spl_file_info::SplFileInfo;
use shirabe_php_shim::{
    array_search, file_exists, file_get_contents, strcmp, strtr, strtr_array,
    token_get_all, PhpMixed, Phar, RuntimeException, UnexpectedValueException,
    T_COMMENT, T_DOC_COMMENT, T_WHITESPACE,
};

use crate::json::json_file::JsonFile;
use crate::util::git::Git;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct Compiler {
    version: String,
    branch_alias_version: String,
    version_date: chrono::DateTime<chrono::Utc>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            version: String::new(),
            branch_alias_version: String::new(),
            version_date: chrono::Utc::now(),
        }
    }

    /// Compiles composer into a single phar file
    pub fn compile(&mut self, phar_file: &str) -> anyhow::Result<()> {
        let phar_file = if phar_file.is_empty() {
            "composer.phar"
        } else {
            phar_file
        };

        if file_exists(phar_file) {
            shirabe_php_shim::unlink(phar_file);
        }

        let process = ProcessExecutor::new_default();

        let command = Git::build_rev_list_command(&process, &["-n1", "--format=%H", "HEAD"]);
        let mut output = String::new();
        // PHP: dirname(__DIR__, 2) - going up 2 levels from src/Composer to the repo root
        let repo_root = shirabe_php_shim::dirname_levels(file!(), 2);
        if process.execute(&command, &mut output, Some(&repo_root)) != 0 {
            return Err(RuntimeException {
                message: "Can't run git rev-list. You must ensure to run compile from composer git repository clone and that git binary is available.".to_string(),
                code: 0,
            }.into());
        }
        self.version = Git::parse_rev_list_output(&output, &process).trim().to_string();

        let command = Git::build_rev_list_command(&process, &["-n1", "--format=%ci", "HEAD"]);
        let mut output = String::new();
        if process.execute(&command, &mut output, Some(&repo_root)) != 0 {
            return Err(RuntimeException {
                message: "Can't run git rev-list. You must ensure to run compile from composer git repository clone and that git binary is available.".to_string(),
                code: 0,
            }.into());
        }

        let version_date_str = Git::parse_rev_list_output(&output, &process);
        self.version_date = chrono::DateTime::parse_from_str(version_date_str.trim(), "%Y-%m-%d %H:%M:%S %z")
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        let mut git_describe_output = String::new();
        if process.execute(
            &[
                "git".to_string(),
                "describe".to_string(),
                "--tags".to_string(),
                "--exact-match".to_string(),
                "HEAD".to_string(),
            ],
            &mut git_describe_output,
            Some(&repo_root),
        ) == 0
        {
            self.version = git_describe_output.trim().to_string();
        } else {
            // get branch-alias defined in composer.json for dev-main (if any)
            let local_config_path = format!("{}/composer.json", repo_root);
            let file = JsonFile::new(&local_config_path);
            let local_config = file.read()?;
            if let Some(branch_alias) = local_config
                .as_array()
                .and_then(|m| m.get("extra"))
                .and_then(|e| e.as_array())
                .and_then(|m| m.get("branch-alias"))
                .and_then(|b| b.as_array())
                .and_then(|m| m.get("dev-main"))
                .and_then(|v| v.as_string())
            {
                self.branch_alias_version = branch_alias.to_string();
            }
        }

        if self.version.is_empty() {
            return Err(UnexpectedValueException {
                message: "Version detection failed".to_string(),
                code: 0,
            }
            .into());
        }

        let mut phar = Phar::new_phar(phar_file.to_string(), 0, "composer.phar");
        phar.set_signature_algorithm(Phar::SHA512);

        phar.start_buffering();

        let finder_sort =
            |a: &SplFileInfo, b: &SplFileInfo| -> i64 {
                strcmp(
                    &strtr(a.get_real_path(), "\\", "/"),
                    &strtr(b.get_real_path(), "\\", "/"),
                )
            };

        // Add Composer sources
        let mut finder = Finder::new();
        finder
            .files()
            .ignore_vcs(true)
            .name("*.php")
            .not_name("Compiler.php")
            .not_name("ClassLoader.php")
            .not_name("InstalledVersions.php")
            .r#in(&format!("{}/src/Composer/..", repo_root))
            .sort(finder_sort);
        for file in finder.iter() {
            self.add_file(&mut phar, &file, true)?;
        }
        // Add runtime utilities separately to make sure they retain the docblocks as these will get copied into projects
        self.add_file(
            &mut phar,
            &SplFileInfo::new(&format!("{}/src/Composer/Autoload/ClassLoader.php", repo_root)),
            false,
        )?;
        self.add_file(
            &mut phar,
            &SplFileInfo::new(&format!("{}/src/Composer/InstalledVersions.php", repo_root)),
            false,
        )?;

        // Add Composer resources
        let mut finder = Finder::new();
        finder
            .files()
            .r#in(&format!("{}/res", repo_root))
            .sort(finder_sort);
        for file in finder.iter() {
            self.add_file(&mut phar, &file, false)?;
        }

        // Add vendor files
        let mut finder = Finder::new();
        finder
            .files()
            .ignore_vcs(true)
            .not_path(r"/\/(composer\.(?:json|lock)|[A-Z]+\.md(?:own)?|\.gitignore|appveyor.yml|phpunit\.xml\.dist|phpstan\.neon\.dist|phpstan-config\.neon|phpstan-baseline\.neon|UPGRADE.*\.(?:md|txt))$/")
            .not_path(r"/bin\/(jsonlint|validate-json|simple-phpunit|phpstan|phpstan\.phar)(\.bat)?$/")
            .not_path("justinrainbow/json-schema/demo/")
            .not_path("justinrainbow/json-schema/dist/")
            .not_path("justinrainbow/json-schema/bin/")
            .not_path("composer/pcre/extension.neon")
            .not_path("composer/LICENSE")
            .exclude("Tests")
            .exclude("tests")
            .exclude("docs")
            .r#in(&format!("{}/vendor/", repo_root))
            .sort(finder_sort);

        let mut extra_files: IndexMap<String, String> = IndexMap::new();
        let extra_file_paths = vec![
            format!("{}/vendor/composer/installed.json", repo_root),
            format!(
                "{}/vendor/composer/spdx-licenses/res/spdx-exceptions.json",
                repo_root
            ),
            format!(
                "{}/vendor/composer/spdx-licenses/res/spdx-licenses.json",
                repo_root
            ),
            CaBundle::get_bundled_ca_bundle_path(),
            format!(
                "{}/vendor/symfony/console/Resources/bin/hiddeninput.exe",
                repo_root
            ),
            format!(
                "{}/vendor/symfony/console/Resources/completion.bash",
                repo_root
            ),
        ];
        for file_path in &extra_file_paths {
            let real = shirabe_php_shim::realpath(file_path).unwrap_or_default();
            extra_files.insert(file_path.clone(), real);
            if !file_exists(file_path) {
                return Err(RuntimeException {
                    message: format!(
                        "Extra file listed is missing from the filesystem: {}",
                        file_path
                    ),
                    code: 0,
                }
                .into());
            }
        }
        let mut unexpected_files: Vec<String> = vec![];

        for file in finder.iter() {
            if let Some(index) =
                array_search(file.get_real_path(), &extra_files)
            {
                extra_files.shift_remove(&index);
            } else if !Preg::is_match(
                r"{(^LICENSE(?:\.txt)?$|\.php$)}",
                file.get_filename(),
            )? {
                unexpected_files.push(file.to_string());
            }

            if Preg::is_match(r"{\.php[\d.]*$}", file.get_filename())? {
                self.add_file(&mut phar, &file, true)?;
            } else {
                self.add_file(&mut phar, &file, false)?;
            }
        }

        if !extra_files.is_empty() {
            return Err(RuntimeException {
                message: format!(
                    "These files were expected but not added to the phar, they might be excluded or gone from the source package:\n{}",
                    shirabe_php_shim::var_export(&PhpMixed::Null, true) // TODO: var_export of extra_files
                ),
                code: 0,
            }
            .into());
        }
        if !unexpected_files.is_empty() {
            return Err(RuntimeException {
                message: format!(
                    "These files were unexpectedly added to the phar, make sure they are excluded or listed in $extraFiles:\n{}",
                    shirabe_php_shim::var_export(&PhpMixed::Null, true) // TODO: var_export of unexpected_files
                ),
                code: 0,
            }
            .into());
        }

        // Add bin/composer
        self.add_composer_bin(&mut phar)?;

        // Stubs
        phar.set_stub(&self.get_stub());

        phar.stop_buffering();

        // disabled for interoperability with systems without gzip ext
        // $phar->compressFiles(\Phar::GZ);

        let license_file = SplFileInfo::new(&format!("{}/LICENSE", repo_root));
        self.add_file(&mut phar, &license_file, false)?;

        drop(phar);

        // re-sign the phar with reproducible timestamp / signature
        let mut util = Timestamps::new(phar_file);
        util.update_timestamps(&self.version_date);
        util.save(phar_file, Phar::SHA512);

        Linter::lint(
            phar_file,
            &[
                "vendor/symfony/console/Attribute/AsCommand.php",
                "vendor/symfony/polyfill-intl-grapheme/bootstrap80.php",
                "vendor/symfony/polyfill-intl-normalizer/bootstrap80.php",
                "vendor/symfony/polyfill-mbstring/bootstrap80.php",
                "vendor/symfony/polyfill-php73/Resources/stubs/JsonException.php",
                "vendor/symfony/service-contracts/Attribute/SubscribedService.php",
                "vendor/symfony/polyfill-php84/Resources/stubs/Deprecated.php",
                "vendor/symfony/polyfill-php84/Resources/Deprecated.php",
                "vendor/symfony/polyfill-php84/Resources/RoundingMode.php",
                "vendor/symfony/polyfill-php84/bootstrap82.php",
            ],
        );

        Ok(())
    }

    fn get_relative_file_path(&self, file: &SplFileInfo) -> String {
        let real_path = file.get_real_path();
        // PHP: dirname(__DIR__, 2) . DIRECTORY_SEPARATOR - repo root + separator
        let repo_root = shirabe_php_shim::dirname_levels(file!(), 2);
        let path_prefix = format!("{}/", repo_root);

        let relative_path = if let Some(stripped) = real_path.strip_prefix(&path_prefix) {
            stripped.to_string()
        } else {
            real_path.to_string()
        };

        strtr(&relative_path, "\\", "/")
    }

    fn add_file(&self, phar: &mut Phar, file: &SplFileInfo, strip: bool) -> anyhow::Result<()> {
        let path = self.get_relative_file_path(file);
        let content = file_get_contents(file.get_path())
            .unwrap_or_default();
        let mut content = if strip {
            self.strip_whitespace(&content)
        } else if file.get_filename() == "LICENSE" {
            format!("\n{}\n", content)
        } else {
            content
        };

        if path == "src/Composer/Composer.php" {
            let mut replacements: IndexMap<String, String> = IndexMap::new();
            replacements.insert(
                "@package_version@".to_string(),
                self.version.clone(),
            );
            replacements.insert(
                "@package_branch_alias_version@".to_string(),
                self.branch_alias_version.clone(),
            );
            replacements.insert(
                "@release_date@".to_string(),
                self.version_date.format("%Y-%m-%d %H:%M:%S").to_string(),
            );
            content = strtr_array(&content, &replacements);
            content = Preg::replace(
                r"{SOURCE_VERSION = '[^']+';}"
,
                "SOURCE_VERSION = '';",
                &content,
            )?;
        }

        phar.add_from_string(&path, &content);

        Ok(())
    }

    fn add_composer_bin(&self, phar: &mut Phar) -> anyhow::Result<()> {
        let repo_root = shirabe_php_shim::dirname_levels(file!(), 2);
        let content = file_get_contents(&format!("{}/bin/composer", repo_root))
            .unwrap_or_default();
        let content = Preg::replace(r"{^#!/usr/bin/env php\s*}", "", &content)?;
        phar.add_from_string("bin/composer", &content);
        Ok(())
    }

    /// Removes whitespace from a PHP source string while preserving line numbers.
    fn strip_whitespace(&self, source: &str) -> String {
        if !shirabe_php_shim::function_exists("token_get_all") {
            return source.to_string();
        }

        let mut output = String::new();
        for token in token_get_all(source) {
            match &token {
                PhpMixed::String(s) => {
                    output.push_str(s);
                }
                PhpMixed::List(arr) if arr.len() >= 2 => {
                    let token_type = arr[0].as_int().unwrap_or(0);
                    let token_value = arr[1].as_string().unwrap_or("");
                    if token_type == T_COMMENT || token_type == T_DOC_COMMENT {
                        let newline_count =
                            shirabe_php_shim::substr_count(token_value, "\n") as usize;
                        output.push_str(&"\n".repeat(newline_count));
                    } else if token_type == T_WHITESPACE {
                        // reduce wide spaces
                        let whitespace = Preg::replace(r"{[ \t]+}", " ", token_value)
                            .unwrap_or_else(|_| token_value.to_string());
                        // normalize newlines to \n
                        let whitespace = Preg::replace(r"{(?:\r\n|\r|\n)}", "\n", &whitespace)
                            .unwrap_or(whitespace);
                        // trim leading spaces
                        let whitespace = Preg::replace(r"{\n +}", "\n", &whitespace)
                            .unwrap_or(whitespace);
                        output.push_str(&whitespace);
                    } else {
                        output.push_str(token_value);
                    }
                }
                _ => {}
            }
        }

        output
    }

    fn get_stub(&self) -> String {
        let stub = r#"#!/usr/bin/env php
<?php
/*
 * This file is part of Composer.
 *
 * (c) Nils Adermann <naderman@naderman.de>
 *     Jordi Boggiano <j.boggiano@seld.be>
 *
 * For the full copyright and license information, please view
 * the license that is located at the bottom of this file.
 */

// Avoid APC causing random fatal errors per https://github.com/composer/composer/issues/264
if (extension_loaded('apc') && filter_var(ini_get('apc.enable_cli'), FILTER_VALIDATE_BOOLEAN) && filter_var(ini_get('apc.cache_by_default'), FILTER_VALIDATE_BOOLEAN)) {
    if (version_compare(phpversion('apc'), '3.0.12', '>=')) {
        ini_set('apc.cache_by_default', 0);
    } else {
        fwrite(STDERR, 'Warning: APC <= 3.0.12 may cause fatal errors when running composer commands.'.PHP_EOL);
        fwrite(STDERR, 'Update APC, or set apc.enable_cli or apc.cache_by_default to 0 in your php.ini.'.PHP_EOL);
    }
}

if (!class_exists('Phar')) {
    echo 'PHP\'s phar extension is missing. Composer requires it to run. Enable the extension or recompile php without --disable-phar then try again.' . PHP_EOL;
    exit(1);
}

Phar::mapPhar('composer.phar');

"#;

        // add warning once the phar is older than 60 days
        let mut stub = stub.to_string();
        if Preg::is_match(r"{^[a-f0-9]+$}", &self.version).unwrap_or(false) {
            let warning_time = self.version_date.timestamp() + 60 * 86400;
            stub.push_str(&format!(
                "define('COMPOSER_DEV_WARNING_TIME', {});\n",
                warning_time
            ));
        }

        stub.push_str(
            r#"require 'phar://composer.phar/bin/composer';

__HALT_COMPILER();
"#,
        );

        stub
    }
}
