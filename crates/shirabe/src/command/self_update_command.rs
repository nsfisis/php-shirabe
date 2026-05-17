//! ref: composer/src/Composer/Command/SelfUpdateCommand.php

use crate::io::io_interface;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_php_shim::{
    InvalidArgumentException, OPENSSL_ALGO_SHA384, PHP_EOL, PHP_VERSION_ID, Phar, PharException,
    PhpMixed, RuntimeException, UnexpectedValueException, array_map, base64_decode,
    basename_with_suffix, chmod, class_exists, copy, defined, dirname, end_arr, exec,
    extension_loaded, file_exists, file_get_contents, file_put_contents, fileowner, fileperms,
    function_exists, hash_file, in_array, ini_get, is_array, is_file, is_numeric, is_writable,
    iterator_to_array, json_decode, openssl_free_key, openssl_get_md_methods,
    openssl_pkey_get_public, openssl_verify, posix_geteuid, posix_getpwuid, random_int, rename,
    server_argv, sprintf, str_contains, str_replace, strpos, strtolower, strtr, tempnam, unlink,
    usleep, version_compare,
};

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::downloader::filesystem_exception::FilesystemException;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::self_update::keys::Keys;
use crate::self_update::versions::Versions;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct SelfUpdateCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl SelfUpdateCommand {
    const HOMEPAGE: &'static str = "getcomposer.org";
    const OLD_INSTALL_EXT: &'static str = "-old.phar";

    pub fn configure(&mut self) {
        self.inner
            .set_name("self-update")
            .set_aliases(vec!["selfupdate".to_string()])
            .set_description("Updates composer.phar to the latest version")
            .set_definition(vec![
                InputOption::new("rollback", Some(PhpMixed::String("r".to_string())), Some(InputOption::VALUE_NONE), "Revert to an older installation of composer", None, vec![]),
                InputOption::new("clean-backups", None, Some(InputOption::VALUE_NONE), "Delete old backups during an update. This makes the current version of composer the only backup available after the update", None, vec![]),
                InputArgument::new("version", Some(InputArgument::OPTIONAL), "The version to update to", None, vec![]),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None, vec![]),
                InputOption::new("update-keys", None, Some(InputOption::VALUE_NONE), "Prompt user for a key update", None, vec![]),
                InputOption::new("stable", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel", None, vec![]),
                InputOption::new("preview", None, Some(InputOption::VALUE_NONE), "Force an update to the preview channel", None, vec![]),
                InputOption::new("snapshot", None, Some(InputOption::VALUE_NONE), "Force an update to the snapshot channel", None, vec![]),
                InputOption::new("1", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 1.x versions", None, vec![]),
                InputOption::new("2", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 2.x versions", None, vec![]),
                InputOption::new("2.2", None, Some(InputOption::VALUE_NONE), "Force an update to the stable channel, but only use 2.2.x LTS versions", None, vec![]),
                InputOption::new("set-channel-only", None, Some(InputOption::VALUE_NONE), "Only store the channel as the default one and then exit", None, vec![]),
            ])
            .set_help(
                "The <info>self-update</info> command checks getcomposer.org for newer\n\
                versions of composer and if found, installs the latest.\n\
                \n\
                <info>php composer.phar self-update</info>\n\
                \n\
                Read more at https://getcomposer.org/doc/03-cli.md#self-update-selfupdate"
            );
    }

    /// @throws FilesystemException
    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        // TODO(phase-b): __FILE__ / __DIR__ have no direct Rust equivalent
        let file_path: &str = "";
        let dir_path: &str = "";

        if strpos(file_path, "phar:") != Some(0) {
            if str_contains(&strtr(dir_path, "\\", "/"), "vendor/composer/composer") {
                let proj_dir = shirabe_php_shim::dirname_levels(dir_path, 6);
                output.writeln(
                    PhpMixed::String(
                        "<error>This instance of Composer does not have the self-update command.</error>"
                            .to_string(),
                    ),
                    io_interface::NORMAL,
                );
                output.writeln(
                    PhpMixed::String(format!(
                        "<comment>You are running Composer installed as a package in your current project (\"{}\").</comment>",
                        proj_dir
                    )),
                    io_interface::NORMAL,
                );
                output.writeln(
                    PhpMixed::String(
                        "<comment>To update Composer, download a composer.phar from https://getcomposer.org and then run `composer.phar update composer/composer` in your project.</comment>"
                            .to_string(),
                    ),
                    io_interface::NORMAL,
                );
            } else {
                output.writeln(
                    PhpMixed::String(
                        "<error>This instance of Composer does not have the self-update command.</error>"
                            .to_string(),
                    ),
                    io_interface::NORMAL,
                );
                output.writeln(
                    PhpMixed::String(
                        "<comment>This could be due to a number of reasons, such as Composer being installed as a system package on your OS, or Composer being installed as a package in the current project.</comment>"
                            .to_string(),
                    ),
                    io_interface::NORMAL,
                );
            }

            return Ok(1);
        }

        if server_argv().get(0).map(|s| s.as_str()) == Some("Standard input code") {
            return Ok(1);
        }

        // trigger autoloading of a few classes which may be needed when verifying/swapping the phar file
        // to ensure we do not try to load them from the new phar, see https://github.com/composer/composer/issues/10252
        class_exists("Composer\\Util\\Platform");
        class_exists("Composer\\Downloader\\FilesystemException");

        let config = Factory::create_config(None, None)?;

        let base_url = if config.get("disable-tls").as_bool() == Some(true) {
            format!("http://{}", Self::HOMEPAGE)
        } else {
            format!("https://{}", Self::HOMEPAGE)
        };

        let io = self.inner.get_io();
        let http_downloader = Factory::create_http_downloader(io, &config)?;

        let mut versions_util = Versions::new(config.clone(), http_downloader.clone());

        // switch channel if requested
        let mut requested_channel: Option<String> = None;
        for channel in Versions::CHANNELS {
            if input.get_option(channel).as_bool().unwrap_or(false) {
                requested_channel = Some(channel.to_string());
                versions_util.set_channel(channel.to_string(), Some(io))??;
                break;
            }
        }

        if input
            .get_option("set-channel-only")
            .as_bool()
            .unwrap_or(false)
        {
            return Ok(0);
        }

        let cache_dir = config
            .get("cache-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        let rollback_dir = config.get("data-dir").as_string().unwrap_or("").to_string();
        let home = config.get("home").as_string().unwrap_or("").to_string();
        let local_filename = Phar::running(false);
        if local_filename.is_empty() {
            return Err(RuntimeException {
                message: "Could not determine the location of the composer.phar file as it appears you are not running this code from a phar archive.".to_string(),
                code: 0,
            }
            .into());
        }

        if input.get_option("update-keys").as_bool().unwrap_or(false) {
            self.fetch_keys(io, &config)?;

            return Ok(0);
        }

        // ensure composer.phar location is accessible
        if !file_exists(&local_filename) {
            return Err(FilesystemException::new(
                format!(
                    "Composer update failed: the \"{}\" is not accessible",
                    local_filename
                ),
                0,
            )
            .0
            .into());
        }

        // check if current dir is writable and if not try the cache dir from settings
        let tmp_dir = if is_writable(&dirname(&local_filename)) {
            dirname(&local_filename)
        } else {
            cache_dir.clone()
        };

        // check for permissions in local filesystem before start connection process
        if !is_writable(&tmp_dir) {
            return Err(FilesystemException::new(
                format!(
                    "Composer update failed: the \"{}\" directory used to download the temp file could not be written",
                    tmp_dir
                ),
                0,
            )
            .0
            .into());
        }

        // check if composer is running as the same user that owns the directory root, only if POSIX is defined and callable
        if function_exists("posix_getpwuid") && function_exists("posix_geteuid") {
            let composer_user = posix_getpwuid(posix_geteuid());
            let home_dir_owner_id = fileowner(&home);
            if is_array(composer_user.clone()) && home_dir_owner_id.is_some() {
                let home_owner = posix_getpwuid(home_dir_owner_id.unwrap_or(0));
                let composer_user_name = composer_user
                    .as_array()
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                let home_owner_name = home_owner
                    .as_array()
                    .and_then(|m| m.get("name"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                if is_array(home_owner.clone()) && composer_user_name != home_owner_name {
                    io.write_error(
                        PhpMixed::String(format!(
                            "<warning>You are running Composer as \"{}\", while \"{}\" is owned by \"{}\"</warning>",
                            composer_user_name, home, home_owner_name
                        )),
                        true,
                        io_interface::NORMAL,
                    );
                }
            }
        }

        if input.get_option("rollback").as_bool().unwrap_or(false) {
            return self.rollback(output, &rollback_dir, &local_filename);
        }

        if input.get_argument("command").as_string() == Some("self")
            && input.get_argument("version").as_string() == Some("update")
        {
            input.set_argument("version", PhpMixed::Null);
        }

        let latest = versions_util.get_latest(None)??;
        let mut latest_stable = versions_util.get_latest(Some("stable"))??;
        let latest_preview = match versions_util.get_latest(Some("preview"))? {
            Ok(p) => p,
            Err(_e) => latest_stable.clone(),
        };
        let mut latest_version = latest
            .get("version")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let mut update_version = input
            .get_argument("version")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_else(|| latest_version.clone());
        let current_major_version = Preg::replace(r"{^(\d+).*}", "$1", Composer::get_version());
        let update_major_version = Preg::replace(r"{^(\d+).*}", "$1", update_version.clone());
        let preview_major_version = Preg::replace(
            r"{^(\d+).*}",
            "$1",
            latest_preview
                .get("version")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string(),
        );

        if versions_util.get_channel()? == "stable" && input.get_argument("version").is_null() {
            // if requesting stable channel and no specific version, avoid automatically upgrading to the next major
            // simply output a warning that the next major stable is available and let users upgrade to it manually
            if version_compare(&current_major_version, &update_major_version, "<") {
                let skipped_version = update_version.clone();

                versions_util.set_channel(current_major_version.clone(), None)??;

                let new_latest = versions_util.get_latest(None)??;
                latest_stable = versions_util.get_latest(Some("stable"))??;
                latest_version = new_latest
                    .get("version")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                update_version = latest_version.clone();

                io.write_error(
                    PhpMixed::String(format!(
                        "<warning>A new stable major version of Composer is available ({}), run \"composer self-update --{}\" to update to it. See also https://getcomposer.org/{}</warning>",
                        skipped_version, update_major_version, update_major_version
                    )),
                    true,
                    io_interface::NORMAL,
                );
            } else if version_compare(&current_major_version, &preview_major_version, "<") {
                // promote next major version if available in preview
                io.write_error(
                    PhpMixed::String(format!(
                        "<warning>A preview release of the next major version of Composer is available ({}), run \"composer self-update --preview\" to give it a try. See also https://github.com/composer/composer/releases for changelogs.</warning>",
                        latest_preview.get("version").and_then(|v| v.as_string()).unwrap_or("")
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }
        }

        let effective_channel = match requested_channel.as_deref() {
            None => versions_util.get_channel()?,
            Some(c) => c.to_string(),
        };
        if is_numeric(&effective_channel)
            && strpos(
                latest_stable
                    .get("version")
                    .and_then(|v| v.as_string())
                    .unwrap_or(""),
                &effective_channel,
            ) != Some(0)
        {
            io.write_error(
                PhpMixed::String(format!(
                    "<warning>Warning: You forced the install of {} via --{}, but {} is the latest stable version. Updating to it via composer self-update --stable is recommended.</warning>",
                    latest_version,
                    effective_channel,
                    latest_stable.get("version").and_then(|v| v.as_string()).unwrap_or("")
                )),
                true,
                io_interface::NORMAL,
            );
        }
        if latest.contains_key("eol") {
            io.write_error(
                PhpMixed::String(format!(
                    "<warning>Warning: Version {} is EOL / End of Life. {} is the latest stable version. Updating to it via composer self-update --stable is recommended.</warning>",
                    latest_version,
                    latest_stable.get("version").and_then(|v| v.as_string()).unwrap_or("")
                )),
                true,
                io_interface::NORMAL,
            );
        }

        if Preg::is_match(r"{^[0-9a-f]{40}$}", &update_version).unwrap_or(false)
            && update_version != latest_version
        {
            io.write_error(
                PhpMixed::String(
                    "<error>You can not update to a specific SHA-1 as those phars are not available for download</error>"
                        .to_string(),
                ),
                true,
                io_interface::NORMAL,
            );

            return Ok(1);
        }

        let mut channel_string = versions_util.get_channel()?;
        if is_numeric(&channel_string) {
            channel_string.push_str(".x");
        }

        if Composer::VERSION == update_version.as_str() {
            io.write_error(
                PhpMixed::String(sprintf(
                    "<info>You are already using the latest available Composer version %s (%s channel).</info>",
                    &[
                        PhpMixed::String(update_version.clone()),
                        PhpMixed::String(channel_string.clone()),
                    ],
                )),
                true,
                io_interface::NORMAL,
            );

            // remove all backups except for the most recent, if any
            if input.get_option("clean-backups").as_bool().unwrap_or(false) {
                let last_backup = self.get_last_backup_version(&rollback_dir);
                self.clean_backups(&rollback_dir, last_backup.as_deref());
            }

            return Ok(0);
        }

        let temp_filename = format!(
            "{}/{}-temp{}.phar",
            tmp_dir,
            basename_with_suffix(&local_filename, ".phar"),
            random_int(0, 10000000)
        );
        let backup_file = sprintf(
            "%s/%s-%s%s",
            &[
                PhpMixed::String(rollback_dir.clone()),
                PhpMixed::String(strtr(Composer::RELEASE_DATE, " :", "_-")),
                PhpMixed::String(Preg::replace(
                    r"{^([0-9a-f]{7})[0-9a-f]{33}$}",
                    "$1",
                    Composer::VERSION.to_string(),
                )),
                PhpMixed::String(Self::OLD_INSTALL_EXT.to_string()),
            ],
        );

        let updating_to_tag =
            !Preg::is_match(r"{^[0-9a-f]{40}$}", &update_version).unwrap_or(false);

        io.write(
            PhpMixed::String(sprintf(
                "Upgrading to version <info>%s</info> (%s channel).",
                &[
                    PhpMixed::String(update_version.clone()),
                    PhpMixed::String(channel_string.clone()),
                ],
            )),
            true,
            io_interface::NORMAL,
        );
        let remote_filename = format!(
            "{}{}",
            base_url,
            if updating_to_tag {
                format!("/download/{}/composer.phar", update_version)
            } else {
                "/composer.phar".to_string()
            }
        );
        let signature = match http_downloader.get(
            &format!("{}.sig", remote_filename),
            &PhpMixed::Array(indexmap::IndexMap::new()),
        ) {
            Ok(r) => r.get_body().map(|s| s.to_string()),
            Err(e) => {
                if e.get_status_code() == Some(404) {
                    return Err(InvalidArgumentException {
                        message: format!("Version \"{}\" could not be found.", update_version),
                        code: 0,
                    }
                    .into());
                }
                return Err(e.into());
            }
        };
        io.write_error(
            PhpMixed::String("   ".to_string()),
            false,
            io_interface::NORMAL,
        );
        http_downloader.copy(&remote_filename, &temp_filename)?;
        io.write_error(PhpMixed::String(String::new()), true, io_interface::NORMAL);

        if !file_exists(&temp_filename) || signature.is_none() || signature.as_deref() == Some("") {
            io.write_error(
                PhpMixed::String(
                    "<error>The download of the new composer version failed for an unexpected reason</error>"
                        .to_string(),
                ),
                true,
                io_interface::NORMAL,
            );

            return Ok(1);
        }
        let signature = signature.unwrap_or_default();

        // verify phar signature
        if !extension_loaded("openssl") && config.get("disable-tls").as_bool() == Some(true) {
            io.write_error(
                PhpMixed::String(
                    "<warning>Skipping phar signature verification as you have disabled OpenSSL via config.disable-tls</warning>"
                        .to_string(),
                ),
                true,
                io_interface::NORMAL,
            );
        } else {
            if !extension_loaded("openssl") {
                return Err(RuntimeException {
                    message: "The openssl extension is required for phar signatures to be verified but it is not available. If you can not enable the openssl extension, you can disable this error, at your own risk, by setting the 'disable-tls' option to true.".to_string(),
                    code: 0,
                }
                .into());
            }

            let sig_file = format!(
                "file://{}/{}",
                home,
                if updating_to_tag {
                    "keys.tags.pub"
                } else {
                    "keys.dev.pub"
                }
            );
            if !file_exists(&sig_file) {
                file_put_contents(
                    &format!("{}/keys.dev.pub", home),
                    "-----BEGIN PUBLIC KEY-----\n\
MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAnBDHjZS6e0ZMoK3xTD7f\n\
FNCzlXjX/Aie2dit8QXA03pSrOTbaMnxON3hUL47Lz3g1SC6YJEMVHr0zYq4elWi\n\
i3ecFEgzLcj+pZM5X6qWu2Ozz4vWx3JYo1/a/HYdOuW9e3lwS8VtS0AVJA+U8X0A\n\
hZnBmGpltHhO8hPKHgkJtkTUxCheTcbqn4wGHl8Z2SediDcPTLwqezWKUfrYzu1f\n\
o/j3WFwFs6GtK4wdYtiXr+yspBZHO3y1udf8eFFGcb2V3EaLOrtfur6XQVizjOuk\n\
8lw5zzse1Qp/klHqbDRsjSzJ6iL6F4aynBc6Euqt/8ccNAIz0rLjLhOraeyj4eNn\n\
8iokwMKiXpcrQLTKH+RH1JCuOVxQ436bJwbSsp1VwiqftPQieN+tzqy+EiHJJmGf\n\
TBAbWcncicCk9q2md+AmhNbvHO4PWbbz9TzC7HJb460jyWeuMEvw3gNIpEo2jYa9\n\
pMV6cVqnSa+wOc0D7pC9a6bne0bvLcm3S+w6I5iDB3lZsb3A9UtRiSP7aGSo7D72\n\
8tC8+cIgZcI7k9vjvOqH+d7sdOU2yPCnRY6wFh62/g8bDnUpr56nZN1G89GwM4d4\n\
r/TU7BQQIzsZgAiqOGXvVklIgAMiV0iucgf3rNBLjjeNEwNSTTG9F0CtQ+7JLwaE\n\
wSEuAuRm+pRqi8BRnQ/GKUcCAwEAAQ==\n\
-----END PUBLIC KEY-----\n",
                );

                file_put_contents(
                    &format!("{}/keys.tags.pub", home),
                    "-----BEGIN PUBLIC KEY-----\n\
MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA0Vi/2K6apCVj76nCnCl2\n\
MQUPdK+A9eqkYBacXo2wQBYmyVlXm2/n/ZsX6pCLYPQTHyr5jXbkQzBw8SKqPdlh\n\
vA7NpbMeNCz7wP/AobvUXM8xQuXKbMDTY2uZ4O7sM+PfGbptKPBGLe8Z8d2sUnTO\n\
bXtX6Lrj13wkRto7st/w/Yp33RHe9SlqkiiS4MsH1jBkcIkEHsRaveZzedUaxY0M\n\
mba0uPhGUInpPzEHwrYqBBEtWvP97t2vtfx8I5qv28kh0Y6t+jnjL1Urid2iuQZf\n\
noCMFIOu4vksK5HxJxxrN0GOmGmwVQjOOtxkwikNiotZGPR4KsVj8NnBrLX7oGuM\n\
nQvGciiu+KoC2r3HDBrpDeBVdOWxDzT5R4iI0KoLzFh2pKqwbY+obNPS2bj+2dgJ\n\
rV3V5Jjry42QOCBN3c88wU1PKftOLj2ECpewY6vnE478IipiEu7EAdK8Zwj2LmTr\n\
RKQUSa9k7ggBkYZWAeO/2Ag0ey3g2bg7eqk+sHEq5ynIXd5lhv6tC5PBdHlWipDK\n\
tl2IxiEnejnOmAzGVivE1YGduYBjN+mjxDVy8KGBrjnz1JPgAvgdwJ2dYw4Rsc/e\n\
TzCFWGk/HM6a4f0IzBWbJ5ot0PIi4amk07IotBXDWwqDiQTwyuGCym5EqWQ2BD95\n\
RGv89BPD+2DLnJysngsvVaUCAwEAAQ==\n\
-----END PUBLIC KEY-----\n",
                );
            }

            let pubkeyid = openssl_pkey_get_public(&sig_file);
            if matches!(pubkeyid, PhpMixed::Bool(false)) {
                return Err(RuntimeException {
                    message: format!("Failed loading the public key from {}", sig_file),
                    code: 0,
                }
                .into());
            }
            let algo = if defined("OPENSSL_ALGO_SHA384") {
                PhpMixed::Int(OPENSSL_ALGO_SHA384)
            } else {
                PhpMixed::String("SHA384".to_string())
            };
            let md_methods_lower: Vec<String> =
                array_map(|s: &String| strtolower(s), &openssl_get_md_methods());
            if !in_array(
                PhpMixed::String("sha384".to_string()),
                &PhpMixed::List(
                    md_methods_lower
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                ),
                true,
            ) {
                return Err(RuntimeException {
                    message: "SHA384 is not supported by your openssl extension, could not verify the phar file integrity".to_string(),
                    code: 0,
                }
                .into());
            }
            let signature_data = json_decode(&signature, true)?;
            let signature_sha384_str = signature_data
                .as_array()
                .and_then(|m| m.get("sha384"))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            let signature_sha384 = match base64_decode(&signature_sha384_str) {
                Some(s) => s,
                None => {
                    return Err(RuntimeException {
                        message: format!(
                            "Failed loading the phar signature from {}.sig, got {}",
                            remote_filename, signature
                        ),
                        code: 0,
                    }
                    .into());
                }
            };
            let verified = openssl_verify(
                &file_get_contents(&temp_filename).unwrap_or_default(),
                &signature_sha384,
                pubkeyid.clone(),
                algo,
            ) == 1;

            // PHP 8 automatically frees the key instance and deprecates the function
            if PHP_VERSION_ID < 80000 {
                // @phpstan-ignore function.deprecated
                openssl_free_key(pubkeyid);
            }

            if !verified {
                return Err(RuntimeException {
                    message: "The phar signature did not match the file you downloaded, this means your public keys are outdated or that the phar file is corrupt/has been modified".to_string(),
                    code: 0,
                }
                .into());
            }
        }

        // remove saved installations of composer
        if input.get_option("clean-backups").as_bool().unwrap_or(false) {
            self.clean_backups(&rollback_dir, None);
        }

        if !self.set_local_phar(&local_filename, &temp_filename, Some(&backup_file))? {
            // @unlink
            let _ = unlink(&temp_filename);

            return Ok(1);
        }

        if file_exists(&backup_file) {
            io.write_error(
                PhpMixed::String(sprintf(
                    "Use <info>composer self-update --rollback</info> to return to version <comment>%s</comment>",
                    &[PhpMixed::String(Composer::VERSION.to_string())],
                )),
                true,
                io_interface::NORMAL,
            );
        } else {
            io.write_error(
                PhpMixed::String(format!(
                    "<warning>A backup of the current version could not be written to {}, no rollback possible</warning>",
                    backup_file
                )),
                true,
                io_interface::NORMAL,
            );
        }

        Ok(0)
    }

    /// @throws \Exception
    pub(crate) fn fetch_keys(&self, io: &dyn IOInterface, config: &Config) -> Result<()> {
        if !io.is_interactive() {
            return Err(RuntimeException {
                message: "Public keys can not be fetched in non-interactive mode, please run Composer interactively".to_string(),
                code: 0,
            }
            .into());
        }

        io.write(
            PhpMixed::String(
                "Open <info>https://composer.github.io/pubkeys.html</info> to find the latest keys"
                    .to_string(),
            ),
            true,
            io_interface::NORMAL,
        );

        // TODO(phase-b): closure captures none; PHP throws inside the closure on bad input
        let validator: Box<dyn Fn(PhpMixed) -> PhpMixed> =
            Box::new(|value: PhpMixed| -> PhpMixed {
                let value_str = value.as_string().unwrap_or("").to_string();
                if !Preg::is_match(
                    r"{^-----BEGIN PUBLIC KEY-----$}",
                    &shirabe_php_shim::trim(&value_str, None),
                )
                .unwrap_or(false)
                {
                    // TODO(phase-b): closure cannot throw
                    panic!("{}", "Invalid input");
                }

                PhpMixed::String(format!("{}\n", shirabe_php_shim::trim(&value_str, None)))
            });

        let mut dev_key = String::new();
        let mut match_: Option<String> = None;
        loop {
            let m = Preg::is_match_strict_groups(
                r"{(-----BEGIN PUBLIC KEY-----.+?-----END PUBLIC KEY-----)}s",
                &dev_key,
            );
            match_ = m.and_then(|m| m.get(0).cloned());
            if match_.is_some() {
                break;
            }
            dev_key = io
                .ask_and_validate(
                    "Enter Dev / Snapshot Public Key (including lines with -----): ".to_string(),
                    Box::new(|v: PhpMixed| v),
                    None,
                    PhpMixed::Null,
                )
                .as_string()
                .unwrap_or("")
                .to_string();
            loop {
                let line = io.ask(String::new(), PhpMixed::String(String::new()));
                let line_str = line.as_string().unwrap_or("").to_string();
                if line_str.is_empty() {
                    break;
                }
                dev_key.push_str(&format!("{}\n", shirabe_php_shim::trim(&line_str, None)));
                if shirabe_php_shim::trim(&line_str, None) == "-----END PUBLIC KEY-----" {
                    break;
                }
            }
        }
        let _ = &validator;
        let key_path = format!(
            "{}/keys.dev.pub",
            config.get("home").as_string().unwrap_or("")
        );
        file_put_contents(&key_path, match_.as_deref().unwrap_or(""));
        io.write(
            PhpMixed::String(format!(
                "Stored key with fingerprint: {}",
                Keys::fingerprint(&key_path)?
            )),
            true,
            io_interface::NORMAL,
        );

        let mut tags_key = String::new();
        let mut match_: Option<String> = None;
        loop {
            let m = Preg::is_match_strict_groups(
                r"{(-----BEGIN PUBLIC KEY-----.+?-----END PUBLIC KEY-----)}s",
                &tags_key,
            );
            match_ = m.and_then(|m| m.get(0).cloned());
            if match_.is_some() {
                break;
            }
            tags_key = io
                .ask_and_validate(
                    "Enter Tags Public Key (including lines with -----): ".to_string(),
                    Box::new(|v: PhpMixed| v),
                    None,
                    PhpMixed::Null,
                )
                .as_string()
                .unwrap_or("")
                .to_string();
            loop {
                let line = io.ask(String::new(), PhpMixed::String(String::new()));
                let line_str = line.as_string().unwrap_or("").to_string();
                if line_str.is_empty() {
                    break;
                }
                tags_key.push_str(&format!("{}\n", shirabe_php_shim::trim(&line_str, None)));
                if shirabe_php_shim::trim(&line_str, None) == "-----END PUBLIC KEY-----" {
                    break;
                }
            }
        }
        let key_path = format!(
            "{}/keys.tags.pub",
            config.get("home").as_string().unwrap_or("")
        );
        file_put_contents(&key_path, match_.as_deref().unwrap_or(""));
        io.write(
            PhpMixed::String(format!(
                "Stored key with fingerprint: {}",
                Keys::fingerprint(&key_path)?
            )),
            true,
            io_interface::NORMAL,
        );

        io.write(
            PhpMixed::String(format!(
                "Public keys stored in {}",
                config.get("home").as_string().unwrap_or("")
            )),
            true,
            io_interface::NORMAL,
        );

        Ok(())
    }

    /// @throws FilesystemException
    pub(crate) fn rollback(
        &mut self,
        _output: &dyn OutputInterface,
        rollback_dir: &str,
        local_filename: &str,
    ) -> Result<i64> {
        let rollback_version = self.get_last_backup_version(rollback_dir);
        let rollback_version = match rollback_version {
            Some(v) => v,
            None => {
                return Err(UnexpectedValueException {
                    message: format!(
                        "Composer rollback failed: no installation to roll back to in \"{}\"",
                        rollback_dir
                    ),
                    code: 0,
                }
                .into());
            }
        };

        let old_file = format!(
            "{}/{}{}",
            rollback_dir,
            rollback_version,
            Self::OLD_INSTALL_EXT
        );

        if !is_file(&old_file) {
            return Err(FilesystemException::new(
                format!(
                    "Composer rollback failed: \"{}\" could not be found",
                    old_file
                ),
                0,
            )
            .0
            .into());
        }
        if !Filesystem::is_readable(&old_file) {
            return Err(FilesystemException::new(
                format!(
                    "Composer rollback failed: \"{}\" could not be read",
                    old_file
                ),
                0,
            )
            .0
            .into());
        }

        let io = self.inner.get_io();
        io.write_error(
            PhpMixed::String(sprintf(
                "Rolling back to version <info>%s</info>.",
                &[PhpMixed::String(rollback_version.clone())],
            )),
            true,
            io_interface::NORMAL,
        );
        if !self.set_local_phar(local_filename, &old_file, None)? {
            return Ok(1);
        }

        Ok(0)
    }

    /// Checks if the downloaded/rollback phar is valid then moves it
    pub(crate) fn set_local_phar(
        &mut self,
        local_filename: &str,
        new_filename: &str,
        backup_target: Option<&str>,
    ) -> Result<bool> {
        let io = self.inner.get_io();
        let perms = fileperms(local_filename);
        if perms >= 0 {
            // @chmod
            let _ = chmod(new_filename, perms as u32);
        }

        // check phar validity
        let mut error: Option<String> = None;
        if !self.validate_phar(new_filename, &mut error)? {
            io.write_error(
                PhpMixed::String(format!(
                    "<error>The {} file is corrupted ({})</error>",
                    if backup_target.is_some() {
                        "update"
                    } else {
                        "backup"
                    },
                    error.unwrap_or_default()
                )),
                true,
                io_interface::NORMAL,
            );

            if backup_target.is_some() {
                io.write_error(
                    PhpMixed::String(
                        "<error>Please re-run the self-update command to try again.</error>"
                            .to_string(),
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }

            return Ok(false);
        }

        // copy current file into backups dir
        if let Some(target) = backup_target {
            // @copy
            let _ = copy(local_filename, target);
        }

        // PHP try/catch
        let move_result: Result<()> = (|| -> Result<()> {
            if Platform::is_windows() {
                // use copy to apply permissions from the destination directory
                // as rename uses source permissions and may block other users
                copy(new_filename, local_filename);
                let _ = unlink(new_filename);
            } else {
                rename(new_filename, local_filename);
            }

            Ok(())
        })();
        match move_result {
            Ok(()) => Ok(true),
            Err(e) => {
                // see if we can run this operation as an Admin on Windows
                if !is_writable(&dirname(local_filename))
                    && io.is_interactive()
                    && self.is_windows_non_admin_user()
                {
                    return Ok(self.try_as_windows_admin(local_filename, new_filename));
                }

                let _ = unlink(new_filename);
                let action = format!(
                    "Composer {}",
                    if backup_target.is_some() {
                        "update"
                    } else {
                        "rollback"
                    }
                );
                Err(FilesystemException::new(
                    format!(
                        "{} failed: \"{}\" could not be written.{}{}",
                        action, local_filename, PHP_EOL, e
                    ),
                    0,
                )
                .0
                .into())
            }
        }
    }

    pub(crate) fn clean_backups(&self, rollback_dir: &str, except: Option<&str>) {
        let finder = self.get_old_installation_finder(rollback_dir);
        let io = self.inner.get_io();
        let fs = Filesystem::new();

        for file in finder {
            if file.get_basename(Self::OLD_INSTALL_EXT) == except.unwrap_or_default() {
                continue;
            }
            let file_str = file.to_string();
            io.write_error(
                PhpMixed::String(format!("<info>Removing: {}</info>", file_str)),
                true,
                io_interface::NORMAL,
            );
            fs.remove(&file_str);
        }
    }

    pub(crate) fn get_last_backup_version(&self, rollback_dir: &str) -> Option<String> {
        let mut finder = self.get_old_installation_finder(rollback_dir);
        finder.sort_by_name();
        // TODO(phase-b): iterator_to_array → Vec<PhpMixed>; PHP end() returns last value
        let files = iterator_to_array(finder.into_iter().map(|_| PhpMixed::Null));

        if (files.len() as i64) > 0 {
            let last_file = files.last().cloned();
            return last_file
                // PHP: end($files)->getBasename(self::OLD_INSTALL_EXT)
                .and_then(|f| f.as_string().map(|s| s.to_string()));
        }

        None
    }

    pub(crate) fn get_old_installation_finder(&self, rollback_dir: &str) -> Finder {
        Finder::create()
            .depth(0)
            .files()
            .name(&format!("*{}", Self::OLD_INSTALL_EXT))
            .in_(rollback_dir)
    }

    /// Validates the downloaded/backup phar file
    ///
    /// Code taken from getcomposer.org/installer. Any changes should be made
    /// there and replicated here
    pub(crate) fn validate_phar(
        &self,
        phar_file: &str,
        error: &mut Option<String>,
    ) -> Result<bool> {
        if ini_get("phar.readonly").as_deref() == Some("1") {
            return Ok(true);
        }

        // PHP try/catch
        let attempt: Result<bool> = (|| -> Result<bool> {
            // Test the phar validity
            let phar = Phar::new(phar_file.to_string());
            // Free the variable to unlock the file
            drop(phar);
            Ok(true)
        })();
        match attempt {
            Ok(b) => Ok(b),
            Err(e) => {
                // PHP: if (!$e instanceof UnexpectedValueException && !$e instanceof PharException) throw $e;
                let is_unexpected = e.downcast_ref::<UnexpectedValueException>().is_some();
                let is_phar = e.downcast_ref::<PharException>().is_some();
                if !is_unexpected && !is_phar {
                    return Err(e);
                }
                *error = Some(e.to_string());
                Ok(false)
            }
        }
    }

    /// Returns true if this is a non-admin Windows user account
    pub(crate) fn is_windows_non_admin_user(&self) -> bool {
        if !Platform::is_windows() {
            return false;
        }

        // fltmc.exe manages filter drivers and errors without admin privileges
        let mut output: Vec<String> = vec![];
        let mut exit_code: i64 = 0;
        exec("fltmc.exe filters", Some(&mut output), Some(&mut exit_code));

        exit_code != 0
    }

    /// Invokes a UAC prompt to update composer.phar as an admin
    ///
    /// Uses either sudo.exe or VBScript to elevate and run cmd.exe move.
    pub(crate) fn try_as_windows_admin(
        &mut self,
        local_filename: &str,
        new_filename: &str,
    ) -> bool {
        let io = self.inner.get_io();

        io.write_error(
            PhpMixed::String(format!(
                "<error>Unable to write \"{}\". Access is denied.</error>",
                local_filename
            )),
            true,
            io_interface::NORMAL,
        );
        let help_message = "Please run the self-update command as an Administrator.";
        let question =
            "Complete this operation with Administrator privileges [<comment>Y,n</comment>]? ";

        if !io.ask_confirmation(question.to_string(), true) {
            io.write_error(
                PhpMixed::String(format!(
                    "<warning>Operation cancelled. {}</warning>",
                    help_message
                )),
                true,
                io_interface::NORMAL,
            );

            return false;
        }

        let tmp_file = tempnam(&shirabe_php_shim::sys_get_temp_dir(), "");
        let tmp_file = match tmp_file {
            Some(f) => f,
            None => {
                io.write_error(
                    PhpMixed::String(format!("<error>Operation failed. {}</error>", help_message)),
                    true,
                    io_interface::NORMAL,
                );

                return false;
            }
        };

        let mut output: Vec<String> = vec![];
        let mut exit_code: i64 = 0;
        exec(
            "sudo config 2> NUL",
            Some(&mut output),
            Some(&mut exit_code),
        );
        let using_sudo = exit_code == 0;

        let script = if using_sudo {
            format!("{}.bat", tmp_file)
        } else {
            format!("{}.vbs", tmp_file)
        };
        rename(&tmp_file, &script);

        let checksum = hash_file("sha256", new_filename).unwrap_or_default();

        // cmd's internal move is fussy about backslashes
        let source = str_replace("/", "\\", new_filename);
        let destination = str_replace("/", "\\", local_filename);

        let code = if using_sudo {
            sprintf(
                "move \"%s\" \"%s\"",
                &[
                    PhpMixed::String(source.clone()),
                    PhpMixed::String(destination.clone()),
                ],
            )
        } else {
            format!(
                "Set UAC = CreateObject(\"Shell.Application\")\n\
                UAC.ShellExecute \"cmd.exe\", \"/c move /y \"\"{}\"\" \"\"{}\"\"\", \"\", \"runas\", 0",
                source, destination
            )
        };

        file_put_contents(&script, &code);
        let command = if using_sudo {
            sprintf("sudo \"%s\"", &[PhpMixed::String(script.clone())])
        } else {
            sprintf("\"%s\"", &[PhpMixed::String(script.clone())])
        };
        exec(&command, None, None);

        // Allow time for the operation to complete
        usleep(300000);
        // @unlink
        let _ = unlink(&script);

        // see if the file was moved and is still accessible
        let result = Filesystem::is_readable(local_filename)
            && hash_file("sha256", local_filename) == Some(checksum);
        if result {
            io.write_error(
                PhpMixed::String("<info>Operation succeeded.</info>".to_string()),
                true,
                io_interface::NORMAL,
            );
        } else {
            io.write_error(
                PhpMixed::String(format!("<error>Operation failed. {}</error>", help_message)),
                true,
                io_interface::NORMAL,
            );
        }

        result
    }
}

impl BaseCommand for SelfUpdateCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}
