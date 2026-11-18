<?php

namespace Shirabe;

/**
 * The dispatch table a `CallFunction` name reaches before the PHP functions (see "Entry points" in
 * docs/dev/php-worker.md).
 */
final class EntryPoints
{
    /** @return array<string, callable(array): mixed> */
    public static function table(): array
    {
        return [
            '__shirabe_constant' => static fn($args) => defined($args[0]) ? constant($args[0]) : null,
            '__shirabe_xdebug_active' => static fn($args) => self::xdebugActive(),
            '__shirabe_get_all_ini_files' => static function ($args) {
                $paths = [(string) php_ini_loaded_file()];
                $scanned = php_ini_scanned_files();
                if ($scanned !== false) {
                    $paths = array_merge($paths, array_map('trim', explode(',', $scanned)));
                }
                return $paths;
            },
            // The names reported on arrive with the query: constants as ltrim($class.'::'.$constant, ':'),
            // classes, and the extensions whose info() output is reported when they are loaded.
            '__shirabe_platform' => static function ($args) {
                $query = $args[0];
                $extensions = get_loaded_extensions();

                $extension_versions = [];
                foreach ($extensions as $extension) {
                    $version = phpversion($extension);
                    $extension_versions[$extension] = $version !== false ? $version : '0';
                }

                $extension_info = [];
                foreach ($query['extension_info'] as $extension) {
                    if (in_array($extension, $extensions, true)) {
                        $extension_info[$extension] = PlatformRuntime::getExtensionInfo($extension);
                    }
                }

                // Only the defined constants carry a value.
                $constants = [];
                foreach ($query['constants'] as $constant) {
                    if (defined($constant)) {
                        $constants[$constant] = constant($constant);
                    }
                }

                $classes = [];
                foreach ($query['classes'] as $class) {
                    $classes[$class] = class_exists($class, false);
                }

                // The values below stand in for the PHP objects and calls PlatformRepository reaches
                // through Composer\Platform\Runtime::invoke()/construct(), reduced to the entries it reads.
                $inet_pton_ipv6 = @inet_pton('::');

                $resource_bundle_version = null;
                if (class_exists('ResourceBundle', false)) {
                    $bundle = \ResourceBundle::create('root', 'ICUDATA', false);
                    if ($bundle !== null) {
                        $resource_bundle_version = $bundle->get('Version');
                    }
                }

                return [
                    'extensions' => $extensions,
                    'extension_versions' => $extension_versions,
                    'extension_info' => $extension_info,
                    'constants' => $constants,
                    'classes' => $classes,
                    'curl_version' => extension_loaded('curl') ? curl_version()['version'] : null,
                    'inet_pton_ipv6' => $inet_pton_ipv6 === false ? null : $inet_pton_ipv6,
                    'resource_bundle_version' => $resource_bundle_version,
                    'intl_char_unicode_version' => class_exists('IntlChar', false) ? \IntlChar::getUnicodeVersion() : null,
                    'imagick_version_string' => extension_loaded('imagick') ? (new \Imagick())->getVersion()['versionString'] : null,
                ];
            },
            // The extensions, functions and ini settings reported on arrive with the query.
            '__shirabe_diagnose' => static function ($args) {
                $query = $args[0];
                $extensions = [];
                foreach ($query['extensions'] as $extension) {
                    $extensions[$extension] = extension_loaded($extension);
                }

                $functions = [];
                foreach ($query['functions'] as $function) {
                    $functions[$function] = function_exists($function);
                }

                $ini_settings = [];
                foreach ($query['ini_settings'] as $setting) {
                    $value = ini_get($setting);
                    $ini_settings[$setting] = $value === false ? null : $value;
                }

                // curl_version() only exists while the extension is loaded; DiagnoseCommand reads these
                // details only after its own extension_loaded('curl') check.
                $curl = null;
                if (extension_loaded('curl')) {
                    $version = curl_version();
                    $curl = [
                        'version' => (string) ($version['version'] ?? ''),
                        'libz_version' => $version['libz_version'] ?? null,
                        'brotli_version' => $version['brotli_version'] ?? null,
                        'ssl_version' => $version['ssl_version'] ?? null,
                        'features' => $version['features'] ?? null,
                        'version_zstd' => defined('CURL_VERSION_ZSTD') ? CURL_VERSION_ZSTD : null,
                        'version_http2' => defined('CURL_VERSION_HTTP2') ? CURL_VERSION_HTTP2 : null,
                        'has_http_version_2_0' => defined('CURL_HTTP_VERSION_2_0'),
                        'version_http3' => defined('CURL_VERSION_HTTP3') ? CURL_VERSION_HTTP3 : null,
                    ];
                }

                ob_start();
                phpinfo(INFO_GENERAL);
                $phpinfo = (string) ob_get_clean();

                return [
                    'php_version' => PHP_VERSION,
                    'php_version_id' => PHP_VERSION_ID,
                    'php_binary' => defined('PHP_BINARY') ? PHP_BINARY : null,
                    'openssl_version_text' => defined('OPENSSL_VERSION_TEXT') ? OPENSSL_VERSION_TEXT : null,
                    'openssl_version_number' => defined('OPENSSL_VERSION_NUMBER') ? OPENSSL_VERSION_NUMBER : 0,
                    'has_hhvm_version' => defined('HHVM_VERSION'),
                    'has_php_windows_version_build' => defined('PHP_WINDOWS_VERSION_BUILD'),
                    'xdebug_active' => self::xdebugActive(),
                    'ioncube_loader_iversion' => extension_loaded('ionCube Loader') ? ioncube_loader_iversion() : 0,
                    'ioncube_loader_version' => extension_loaded('ionCube Loader') ? ioncube_loader_version() : '',
                    'phpinfo_general' => $phpinfo,
                    'curl' => $curl,
                    'extensions' => $extensions,
                    'functions' => $functions,
                    'ini_settings' => $ini_settings,
                ];
            },
            '__shirabe_eval' => static fn($args) => eval($args[0]),
            '__shirabe_require' => static function ($args) {
                require_once $args[0];
                // The required file may have registered further prepending autoloaders (a Composer
                // vendor/autoload.php prepends its ClassLoader); proxied FQCNs must stay resolvable to
                // the stub classes, so the stub autoloader is moved back to the front of the stack.
                RpcRuntime::ensureStubAutoloaderPriority();
                return true;
            },
            // Tries to open the embedded runtime bundle and maps the phar to $alias.
            // Returns whether the extraction succeeds or not.
            '__shirabe_open_runtime_bundle' => static function ($args) {
                [$executable, $alias, $sentinel, $bundleId] = $args[0];
                if (!extension_loaded('phar') || !in_array('phar', stream_get_wrappers(), true)) {
                    return false;
                }
                try {
                    \Phar::loadPhar($executable, $alias);
                    $read = @file_get_contents("phar://{$alias}/{$sentinel}");
                } catch (\Throwable $e) {
                    $read = false;
                }
                // The phar bundle is unsigned, so the worker process is started with
                // phar.require_hash=0. Verification result is saved and not verified
                // again for the same phar file, so it is safe to restore the settings
                // here.
                ini_set('phar.require_hash', '1');
                return $read === $bundleId;
            },
            '__shirabe_enable_script_autoloader' => static function ($args) {
                RpcRuntime::enableScriptAutoloader();
                return true;
            },
            // The body of \Composer\Autoload\composerRequire (AutoloadGenerator.php), sharing its
            // $GLOBALS guard so files already required by a real Composer autoloader in this process
            // are not required twice.
            '__shirabe_composer_require' => static function ($args) {
                [$fileIdentifier, $file] = $args;
                if (empty($GLOBALS['__composer_autoload_files'][$fileIdentifier])) {
                    $GLOBALS['__composer_autoload_files'][$fileIdentifier] = true;

                    require $file;
                }
                return true;
            },
            // Mirrors the tail of FilesystemRepository::write into this child: the unconditional
            // `InstalledVersions::reload($versions)` plus the reflection-based selfDir /
            // installedIsLocalDir restore. Skipped only when the class is not even autoloadable here
            // (the Composer PHP runtime was never loaded): without it no code in this process can
            // observe InstalledVersions at all.
            '__shirabe_installed_versions_reload' => static function ($args) {
                [$versions, $repoDir] = $args;
                if (!class_exists('Composer\\InstalledVersions')) {
                    return true;
                }
                \Composer\InstalledVersions::reload($versions);
                try {
                    $reflProp = new \ReflectionProperty(\Composer\InstalledVersions::class, 'selfDir');
                    (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
                    $reflProp->setValue(null, strtr($repoDir, '\\', '/'));

                    $reflProp = new \ReflectionProperty(\Composer\InstalledVersions::class, 'installedIsLocalDir');
                    (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
                    $reflProp->setValue(null, true);
                } catch (\ReflectionException $e) {
                    if (preg_match('{Property .*? does not exist}i', $e->getMessage()) !== 1) {
                        throw $e;
                    }
                    // noop, an outdated InstalledVersions class simply lacks the properties
                }
                return true;
            },
            // The InstalledVersions part of Factory::createComposer, run once the Composer PHP runtime is
            // loaded into this child. The project's installed.php is loaded only while no
            // InstalledVersions class is defined here yet, as in Composer.
            '__shirabe_safely_load_installed_versions' => static function ($args) {
                [$installedVersionsPath] = $args;
                if (!class_exists('Composer\\InstalledVersions', false)) {
                    // force loading the class at this point so it is loaded from the composer phar and not from the vendor dir
                    // as we cannot guarantee integrity of that file
                    if (class_exists('Composer\\InstalledVersions')) {
                        \Composer\Repository\FilesystemRepository::safelyLoadInstalledVersions($installedVersionsPath);
                    }
                }
                return true;
            },
            // An already-fulfilled promise for a Rust-side call whose PHP signature declares
            // PromiseInterface and that has the value before a promise handle exists.
            '__shirabe_resolved_promise' => static function ($args) {
                if (!function_exists('React\\Promise\\resolve')) {
                    throw new \RuntimeException(
                        'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
                    );
                }
                return PromiseRegistry::park(\React\Promise\resolve($args[0]));
            },
            // The failure half: a promise that failed before it could be registered surfaces to the
            // caller as a rejection it handles, the way it does under Composer, rather than as a
            // throw out of the call that started it.
            '__shirabe_rejected_promise' => static function ($args) {
                if (!function_exists('React\\Promise\\reject')) {
                    throw new \RuntimeException(
                        'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
                    );
                }
                return PromiseRegistry::park(\React\Promise\reject(
                    ThrowableRecord::revive($args[0])
                ));
            },
            // The settle pushes of a Rust-owned promise, sent from inside the tick that saw it
            // finish. The callbacks plugin code chained on the promise run before these return.
            '__shirabe_resolve_promise' => static function ($args) {
                PromiseRegistry::resolve((int) $args[0], $args[1]);
                return null;
            },
            '__shirabe_reject_promise' => static function ($args) {
                [$id, $record] = $args;
                PromiseRegistry::reject((int) $id, ThrowableRecord::revive($record));
                return null;
            },
            // Replays environment writes the Rust side made after this process was spawned, which only
            // inherited the environment as it stood then. The three storages PHP exposes are distinct
            // (docs/dev/env-vars-porting.md), so each write names the one it targets; a null value is an
            // unset.
            '__shirabe_sync_env' => static function ($args) {
                foreach ($args[0] as [$storage, $key, $value]) {
                    switch ($storage) {
                        case 'process':
                            if ($value === null) {
                                putenv($key);
                            } else {
                                putenv("{$key}={$value}");
                            }
                            break;
                        case '_ENV':
                            if ($value === null) {
                                unset($_ENV[$key]);
                            } else {
                                $_ENV[$key] = $value;
                            }
                            break;
                        case '_SERVER':
                            if ($value === null) {
                                unset($_SERVER[$key]);
                            } else {
                                $_SERVER[$key] = $value;
                            }
                            break;
                        default:
                            throw new \RuntimeException("__shirabe_sync_env got an unknown storage `{$storage}`");
                    }
                }
                return true;
            },
            // Builds the worker-side Composer\Console\Application (the runtime/ definition, not the
            // real class) from the Rust handoff; the caller keeps the returned handle and runs
            // plugin-provided commands through __shirabe_run_console_application.
            '__shirabe_console_application_boot' => static function ($args) {
                return \Composer\Console\Application::__shirabeBoot($args[0]);
            },
            // Runs one command line (the stringified input of the Rust-side run) through a booted
            // worker-side application; output goes to the inherited stdio, the exit code is the
            // return value, and a command failure propagates as an RPC throw (catchExceptions is
            // off on the booted application).
            '__shirabe_run_console_application' => static function ($args) {
                [$app, $inputString] = $args;
                if (!$app instanceof \Composer\Console\Application) {
                    throw new \RuntimeException('__shirabe_run_console_application expects an application handle');
                }
                return $app->run(new \Symfony\Component\Console\Input\StringInput($inputString));
            },
            // Reads a command's input definition (plus help text and extra usages) as plain data, so
            // the Rust side can mirror it for `help`/`list` rendering without executing anything.
            '__shirabe_read_command_definition' => static function ($args) {
                $command = $args[0];
                if (!$command instanceof \Symfony\Component\Console\Command\Command) {
                    throw new \RuntimeException('__shirabe_read_command_definition expects a command handle');
                }
                $definition = $command->getDefinition();
                $arguments = [];
                foreach ($definition->getArguments() as $argument) {
                    $arguments[] = [
                        'name' => $argument->getName(),
                        'required' => $argument->isRequired(),
                        'is_array' => $argument->isArray(),
                        'description' => $argument->getDescription(),
                        'default' => $argument->getDefault(),
                    ];
                }
                $options = [];
                foreach ($definition->getOptions() as $option) {
                    $options[] = [
                        'name' => $option->getName(),
                        'shortcut' => $option->getShortcut(),
                        'accept_value' => $option->acceptValue(),
                        'is_value_required' => $option->isValueRequired(),
                        'is_array' => $option->isArray(),
                        'is_negatable' => $option->isNegatable(),
                        'description' => $option->getDescription(),
                        'default' => $option->getDefault(),
                    ];
                }
                return [
                    'arguments' => $arguments,
                    'options' => $options,
                    'help' => $command->getHelp(),
                    'usages' => $command->getUsages(),
                ];
            },
            // For testing: round-trips raw serialize() bytes through the PHP core codec, for the codec
            // oracle tests.
            '__shirabe_oracle_roundtrip' => static fn($args) => serialize(
                RpcRuntime::unserializePayload($args[0])
            ),
            // For testing: reads a public property of a PHP entity (PHPUnit asserts like
            // `$plugins[0]->version` have no method to call).
            '__shirabe_get_property' => static function ($args) {
                $obj = RpcRuntime::fromRpcValue($args[0]);
                if (!is_object($obj)) {
                    throw new \RuntimeException('__shirabe_get_property expects a handle argument');
                }
                return $obj->{$args[1]};
            },
            // For testing: invokes a non-public method of a PHP entity through ReflectionMethod.
            '__shirabe_reflection_invoke' => static function ($args) {
                [$obj, $method, $methodArgs] = $args;
                $reflMethod = new \ReflectionMethod($obj, $method);
                (\PHP_VERSION_ID < 80100) and $reflMethod->setAccessible(true);
                return $reflMethod->invokeArgs($obj, $methodArgs);
            },
            // For testing: reads a non-public property of a PHP entity through ReflectionProperty.
            '__shirabe_reflection_get_property' => static function ($args) {
                [$obj, $name] = $args;
                $reflProp = new \ReflectionProperty($obj, $name);
                (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
                return $reflProp->getValue($obj);
            },
            // For testing: writes a non-public property of a PHP entity through ReflectionProperty.
            '__shirabe_reflection_set_property' => static function ($args) {
                [$obj, $name, $value] = $args;
                $reflProp = new \ReflectionProperty($obj, $name);
                (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
                $reflProp->setValue($obj, $value);
                return null;
            },
        ];
    }

    /**
     * Port of Composer\XdebugHandler\XdebugHandler::setXdebugDetails(), which the
     * `__shirabe_xdebug_active` query and the `__shirabe_diagnose` payload both report.
     */
    private static function xdebugActive(): bool
    {
        if (!extension_loaded('xdebug')) {
            return false;
        }

        $version = phpversion('xdebug');
        $version = $version !== false ? $version : 'unknown';

        if (version_compare($version, '3.1', '>=')) {
            $modes = xdebug_info('mode');
            return (count($modes) === 0 ? 'off' : implode(',', $modes)) !== 'off';
        }

        $ini_mode = ini_get('xdebug.mode');
        if ($ini_mode === false) {
            return true;
        }

        $env_mode = (string) getenv('XDEBUG_MODE');
        if ($env_mode !== '') {
            $mode = $env_mode;
        } else {
            $mode = $ini_mode !== '' ? $ini_mode : 'off';
        }

        if (preg_match('/^,+$/', str_replace(' ', '', $mode)) === 1) {
            $mode = 'off';
        }

        return $mode !== 'off';
    }
}
