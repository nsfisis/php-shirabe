<?php

// PHP glue worker. See docs/dev/php-rpc.md for the frame layout and message catalogue.

const SHIRABE_TAG_CALL_FUNCTION = 0x00;
const SHIRABE_TAG_CALL_STATIC_METHOD = 0x01;
const SHIRABE_TAG_NEW_OBJECT = 0x02;
const SHIRABE_TAG_CALL_PHP_METHOD = 0x03;
const SHIRABE_TAG_CALL_RUST_METHOD = 0x04;
const SHIRABE_TAG_RETURN = 0x05;
const SHIRABE_TAG_THROW = 0x06;
const SHIRABE_TAG_RELEASE_RUST_HANDLE = 0x07;
const SHIRABE_TAG_RELEASE_PHP_HANDLE = 0x08;
const SHIRABE_TAG_EPOCH_BUMP = 0x09;

const SHIRABE_MAX_FRAME_LEN = 268435456; // 256 MiB, mirrored on the Rust side.

/** Marker interface every Rust-proxy stub class implements. */
interface ShirabeRustStub
{
    /** @return array{__rhandle: int, __class: string, __epoch: int} */
    public function __shirabeRustHandleDescriptor(): array;
}

/** Interns proxy stubs so the same Rust handle always yields the same stub instance. */
final class ShirabeRustObjectRegistry
{
    /** @var array<int, WeakReference> */
    private static array $internTable = [];

    public static function stubFor(int $rhandle, string $class, int $epoch): object
    {
        if (isset(self::$internTable[$rhandle])) {
            $existing = self::$internTable[$rhandle]->get();
            if ($existing !== null) {
                return $existing;
            }
        }
        if (!class_exists($class)) {
            throw new RuntimeException(
                "no proxy stub class is available for {$class}"
            );
        }
        $stub = new $class($rhandle, $epoch);
        self::$internTable[$rhandle] = WeakReference::create($stub);
        return $stub;
    }

    /** Invoked when an EpochBump frame arrives. No-op if the stub already died. */
    public static function bumpEpoch(int $rhandle, int $epoch): void
    {
        $ref = self::$internTable[$rhandle] ?? null;
        $stub = $ref !== null ? $ref->get() : null;
        if ($stub !== null && method_exists($stub, '__invalidateCache')) {
            $stub->__invalidateCache($epoch);
        }
    }

    /** Invoked from stub destructors. */
    public static function release(int $rhandle): void
    {
        unset(self::$internTable[$rhandle]);
        ShirabeRpcRuntime::notifyReleaseRustHandle($rhandle);
    }
}

final class ShirabeRpcRuntime
{
    /** @var resource */
    public static $socket;
    public static ?string $stubsDir = null;
    /** @var array<string, callable(array): mixed> */
    public static array $dispatch = [];
    /** Even correlation ids; the Rust side allocates odd ones. */
    private static int $nextCorrId = 2;
    private static bool $scriptAutoloaderRegistered = false;
    private static bool $shuttingDown = false;

    public static function fail(string $message): void
    {
        // A malformed frame means the Rust side and this script disagree about the protocol,
        // which is a bug in Shirabe itself (both halves ship in the same commit). Dying makes
        // the Rust side observe EOF and report a fatal error.
        fwrite(STDERR, "shirabe php worker: {$message}\n");
        exit(1);
    }

    private static function readExact(int $len): ?string
    {
        $buf = '';
        while (strlen($buf) < $len) {
            $chunk = fread(self::$socket, $len - strlen($buf));
            if ($chunk === false || $chunk === '') {
                return null;
            }
            $buf .= $chunk;
        }
        return $buf;
    }

    /** @return ?array{0: int, 1: int, 2: string} [tag, corrId, payload], null on clean EOF */
    public static function readFrame(): ?array
    {
        $header = self::readExact(8);
        if ($header === null) {
            return null;
        }
        $len = unpack('P', $header)[1];
        if ($len < 9 || $len > SHIRABE_MAX_FRAME_LEN) {
            self::fail("invalid frame length {$len}");
        }
        $rest = self::readExact($len);
        if ($rest === null) {
            self::fail('connection lost mid-frame');
        }
        $tag = ord($rest[0]);
        $corrId = unpack('P', substr($rest, 1, 8))[1];
        return [$tag, $corrId, substr($rest, 9)];
    }

    public static function writeFrame(int $tag, int $corrId, string $payload): void
    {
        $frame = pack('P', 9 + strlen($payload)) . chr($tag) . pack('P', $corrId) . $payload;
        if (fwrite(self::$socket, $frame) === false) {
            // Cannot report an error over a broken channel; die and let Rust observe EOF.
            exit(1);
        }
    }

    public static function notifyReleaseRustHandle(int $rhandle): void
    {
        if (self::$shuttingDown) {
            // Destructors run after the socket may already be closed at shutdown; the whole
            // process is going away, so there is nothing left to release remotely.
            return;
        }
        self::writeFrame(SHIRABE_TAG_RELEASE_RUST_HANDLE, 0, serialize([$rhandle]));
    }

    /**
     * Converts a value about to be serialized onto the wire: proxy stubs become handle
     * descriptor arrays; plain scalars and arrays pass through.
     */
    public static function toWire($value)
    {
        if ($value instanceof ShirabeRustStub) {
            return $value->__shirabeRustHandleDescriptor();
        }
        if (is_object($value)) {
            // TODO(plugin): PHP-owned objects (the P table) are not implemented yet; only Rust
            // proxy stubs can cross the boundary until the plugin activation milestone.
            throw new RuntimeException(
                'returning a PHP object over RPC is not supported yet: ' . get_class($value)
            );
        }
        if (is_resource($value)) {
            throw new RuntimeException('a PHP resource cannot cross the RPC boundary');
        }
        if (is_array($value)) {
            return array_map([self::class, 'toWire'], $value);
        }
        return $value;
    }

    /** Converts a decoded wire value: handle descriptor arrays become live objects. */
    public static function fromWire($value)
    {
        if (!is_array($value)) {
            return $value;
        }
        if (isset($value['__rhandle'])) {
            return ShirabeRustObjectRegistry::stubFor(
                $value['__rhandle'],
                $value['__class'],
                $value['__epoch']
            );
        }
        if (isset($value['__phandle'])) {
            // TODO(plugin): the P table is not implemented yet.
            throw new RuntimeException('__phandle descriptors are not supported yet');
        }
        if (isset($value['__pclass']) && count($value) === 1) {
            return $value['__pclass'];
        }
        return array_map([self::class, 'fromWire'], $value);
    }

    /** Sends a CallRustMethod request and drives the cooperative loop until its Return. */
    public static function callRust(int $rhandle, string $method, array $args)
    {
        $corrId = self::$nextCorrId;
        self::$nextCorrId += 2;
        self::writeFrame(
            SHIRABE_TAG_CALL_RUST_METHOD,
            $corrId,
            serialize([$rhandle, $method, self::toWire($args), []])
        );
        while (true) {
            $frame = self::readFrame();
            if ($frame === null) {
                self::fail('connection lost while waiting for a Return from Rust');
            }
            [$tag, $inId, $payload] = $frame;
            if ($tag === SHIRABE_TAG_RETURN || $tag === SHIRABE_TAG_THROW) {
                if ($inId !== $corrId) {
                    self::fail("protocol violation: response for unexpected corr_id {$inId}");
                }
                $fields = unserialize($payload, ['allowed_classes' => false]);
                if (!is_array($fields)) {
                    self::fail('protocol violation: unparseable response payload');
                }
                if ($tag === SHIRABE_TAG_RETURN) {
                    return self::fromWire($fields[0]);
                }
                [$class, $message, $code] = $fields;
                // TODO(plugin): reconstruct the original exception class instead of collapsing
                // everything to RuntimeException.
                throw new RuntimeException($message, (int) $code);
            }
            self::dispatchRequest($tag, $inId, $payload);
        }
    }

    /** The top-level standing loop: serve incoming requests until the Rust side goes away. */
    public static function serveForever(): void
    {
        while (($frame = self::readFrame()) !== null) {
            self::dispatchRequest(...$frame);
        }
        self::$shuttingDown = true;
    }

    public static function dispatchRequest(int $tag, int $corrId, string $payload): void
    {
        $fields = unserialize($payload, ['allowed_classes' => false]);
        if (!is_array($fields)) {
            self::fail('protocol violation: unparseable frame payload');
        }
        switch ($tag) {
            case SHIRABE_TAG_CALL_FUNCTION:
                [$name, $args] = $fields;
                self::replyWith($corrId, static function () use ($name, $args) {
                    $args = ShirabeRpcRuntime::fromWire($args);
                    if (isset(ShirabeRpcRuntime::$dispatch[$name])) {
                        return (ShirabeRpcRuntime::$dispatch[$name])($args);
                    }
                    if (function_exists($name)) {
                        return $name(...$args);
                    }
                    throw new RuntimeException("PHP function `{$name}` does not exist");
                });
                break;
            case SHIRABE_TAG_CALL_STATIC_METHOD:
                [$class, $method, $args] = $fields;
                self::replyWith($corrId, static function () use ($class, $method, $args) {
                    $args = ShirabeRpcRuntime::fromWire($args);
                    if (!is_callable([$class, $method])) {
                        throw new RuntimeException("{$class}::{$method} is not callable");
                    }
                    return $class::$method(...$args);
                });
                break;
            case SHIRABE_TAG_NEW_OBJECT:
                self::replyWith($corrId, static function () {
                    // TODO(plugin): requires the P table (plugin activation milestone M2).
                    throw new RuntimeException('NewObject is not supported yet');
                });
                break;
            case SHIRABE_TAG_CALL_PHP_METHOD:
                self::replyWith($corrId, static function () {
                    // TODO(plugin): requires the P table (plugin activation milestone M2).
                    throw new RuntimeException('CallPhpMethod is not supported yet');
                });
                break;
            case SHIRABE_TAG_RELEASE_PHP_HANDLE:
                // TODO(plugin): the P table is not implemented yet; nothing to release.
                break;
            case SHIRABE_TAG_EPOCH_BUMP:
                [$rhandle, $epoch] = $fields;
                ShirabeRustObjectRegistry::bumpEpoch((int) $rhandle, (int) $epoch);
                break;
            default:
                self::fail("protocol violation: unexpected frame tag {$tag}");
        }
    }

    /** Runs a handler and sends its result as Return, or the raised Throwable as Throw. */
    private static function replyWith(int $corrId, callable $handler): void
    {
        try {
            $result = $handler();
            self::writeFrame(
                SHIRABE_TAG_RETURN,
                $corrId,
                serialize([self::toWire($result), []])
            );
        } catch (Throwable $e) {
            self::writeFrame(
                SHIRABE_TAG_THROW,
                $corrId,
                serialize([get_class($e), $e->getMessage(), (int) $e->getCode()])
            );
        }
    }

    /**
     * Registers the script-class autoloader: classes referenced by composer.json scripts are
     * resolved by asking the Rust-side ClassLoader (built by EventDispatcher::makeAutoloader)
     * where the class file lives. Handle 0 is the runtime service endpoint on the Rust side.
     */
    public static function enableScriptAutoloader(): void
    {
        if (self::$scriptAutoloaderRegistered) {
            return;
        }
        self::$scriptAutoloaderRegistered = true;
        spl_autoload_register(static function (string $class): void {
            $file = ShirabeRpcRuntime::callRust(0, '__shirabe_find_file', [$class]);
            if (is_string($file) && $file !== '') {
                require $file;
            }
        });
    }
}

$client = @stream_socket_client('unix://' . $argv[1], $errno, $errstr);
if ($client === false) {
    exit(1);
}
ShirabeRpcRuntime::$socket = $client;
ShirabeRpcRuntime::$stubsDir = $argv[2] ?? null;

// Proxy stub classes take priority over any other autoloader (including autoloaders that a
// script or the composer runtime registers later), so a proxied FQCN can never be shadowed by
// the real implementation.
spl_autoload_register(static function (string $class): void {
    if (ShirabeRpcRuntime::$stubsDir === null) {
        return;
    }
    $file = ShirabeRpcRuntime::$stubsDir . '/' . str_replace('\\', '/', $class) . '.php';
    if (is_file($file)) {
        require $file;
    }
}, true, true);

// Port of Composer\XdebugHandler\XdebugHandler::setXdebugDetails(), which the diagnose payload
// reports as `xdebug_active`.
$xdebug_active = static function (): bool {
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
};

ShirabeRpcRuntime::$dispatch = [
    'constant' => static fn($args) => defined($args[0]) ? constant($args[0]) : null,
    'inet_pton' => static fn($args) => @inet_pton($args[0]),
    'curl_version' => static fn($args) => function_exists('curl_version') ? (curl_version()['version'] ?? null) : null,
    'get_loaded_extensions' => static fn($args) => get_loaded_extensions(),
    'get_all_ini_files' => static function ($args) {
        $paths = [(string) php_ini_loaded_file()];
        $scanned = php_ini_scanned_files();
        if ($scanned !== false) {
            $paths = array_merge($paths, array_map('trim', explode(',', $scanned)));
        }
        return $paths;
    },
    'extension_info' => static function ($args) {
        if (!extension_loaded($args[0])) {
            return '';
        }
        $re = new ReflectionExtension($args[0]);
        ob_start();
        $re->info();
        return (string) ob_get_clean();
    },
    'diagnose' => static function ($args) use ($xdebug_active) {
        $extensions = [];
        foreach ([
            'apcu',
            'curl',
            'filter',
            'hash',
            'iconv',
            'ionCube Loader',
            'mbstring',
            'openssl',
            'Phar',
            'uopz',
            'zip',
            'zlib',
        ] as $extension) {
            $extensions[$extension] = extension_loaded($extension);
        }

        $functions = [];
        foreach (['disk_free_space', 'json_decode', 'proc_open'] as $function) {
            $functions[$function] = function_exists($function);
        }

        $ini = [];
        foreach ([
            'allow_url_fopen',
            'apc.enable_cli',
            'uopz.disable',
            'uopz.exit',
            'xdebug.profiler_enabled',
        ] as $setting) {
            $value = ini_get($setting);
            $ini[$setting] = $value === false ? null : $value;
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
            'xdebug_active' => $xdebug_active(),
            'ioncube_loader_iversion' => extension_loaded('ionCube Loader') ? ioncube_loader_iversion() : 0,
            'ioncube_loader_version' => extension_loaded('ionCube Loader') ? ioncube_loader_version() : '',
            'phpinfo_general' => $phpinfo,
            'curl' => $curl,
            'extensions' => $extensions,
            'functions' => $functions,
            'ini' => $ini,
        ];
    },
    // Shirabe-internal helpers, not PHP builtins:
    '__shirabe_eval' => static fn($args) => eval($args[0]),
    // Round-trips raw serialize() bytes through the PHP core codec, for the codec oracle tests.
    '__shirabe_oracle_roundtrip' => static fn($args) => serialize(unserialize($args[0], ['allowed_classes' => false])),
    '__shirabe_require' => static function ($args) {
        require_once $args[0];
        return true;
    },
    '__shirabe_enable_script_autoloader' => static function ($args) {
        ShirabeRpcRuntime::enableScriptAutoloader();
        return true;
    },
];

ShirabeRpcRuntime::serveForever();
