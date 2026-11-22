<?php

namespace Shirabe;

/**
 * The protocol's own part of the dispatch table a `CallFunction` name reaches before the PHP
 * functions (see "Entry points" in docs/dev/php-worker.md). The embedder adds the rest through
 * `WorkerConfig::entry_point_classes`.
 */
final class EntryPoints
{
    /** @return array<string, callable(array): mixed> */
    public static function table(): array
    {
        return [
            '__shirabe_constant' => static fn($args) => defined($args[0]) ? constant($args[0]) : null,
            '__shirabe_eval' => static fn($args) => eval($args[0]),
            '__shirabe_require' => static function ($args) {
                require_once $args[0];
                // The required file may have registered further prepending autoloaders (a Composer
                // vendor/autoload.php prepends its ClassLoader); proxied FQCNs must stay resolvable to
                // the stub classes, so the stub autoloader is moved back to the front of the stack.
                RpcRuntime::ensureStubAutoloaderPriority();
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
}
