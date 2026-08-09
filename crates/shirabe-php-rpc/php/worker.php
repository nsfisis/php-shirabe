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
    /**
     * Null when this instance holds no Rust handle: a dual-mode class (see php/runtime/) that
     * was constructed natively in this process crosses the wire as a P-table entity instead.
     *
     * @return ?array{__rhandle: int, __class: string, __epoch: int}
     */
    public function __shirabeRustHandleDescriptor(): ?array;

    /** Binds this stub to an existing Rust entity, in place of running its constructor. */
    public function __shirabeBind(int $rhandle, int $epoch): void;
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
        // The stub's constructor belongs to plugin code building a *new* entity; an entity that
        // already exists is bound directly, so proxying never runs it.
        $stub = (new ReflectionClass($class))->newInstanceWithoutConstructor();
        $stub->__shirabeBind($rhandle, $epoch);
        self::$internTable[$rhandle] = WeakReference::create($stub);
        return $stub;
    }

    /**
     * Interns a stub the registry did not build: a `__clone` forwarder rebinds the copy PHP
     * made to a freshly cloned entity, and that pairing has to be visible to later crossings
     * of the same handle.
     */
    public static function adopt(int $rhandle, object $stub): void
    {
        self::$internTable[$rhandle] = WeakReference::create($stub);
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

/**
 * The P table: PHP-owned entities exposed to Rust, keyed by phandle. Entries are strong
 * references — an entity stays alive until the Rust side sends ReleasePhpHandle.
 */
final class ShirabePhpObjectRegistry
{
    /** @var array<int, object> */
    private static array $objects = [];
    /** @var array<int, int> spl_object_id => phandle, so one entity keeps one handle */
    private static array $handlesByObjectId = [];
    private static int $nextPhandle = 1;

    public static function register(object $obj): int
    {
        $objectId = spl_object_id($obj);
        $existing = self::$handlesByObjectId[$objectId] ?? null;
        if ($existing !== null && isset(self::$objects[$existing])) {
            return $existing;
        }
        $phandle = self::$nextPhandle++;
        self::$objects[$phandle] = $obj;
        self::$handlesByObjectId[$objectId] = $phandle;
        return $phandle;
    }

    public static function get(int $phandle): object
    {
        if (!isset(self::$objects[$phandle])) {
            throw new RuntimeException("unknown PHP handle {$phandle}");
        }
        return self::$objects[$phandle];
    }

    public static function release(int $phandle): void
    {
        $obj = self::$objects[$phandle] ?? null;
        unset(self::$objects[$phandle]);
        if ($obj !== null) {
            unset(self::$handlesByObjectId[spl_object_id($obj)]);
        }
    }

    /** @return array{__phandle: int, __class: string, __implements: list<string>} */
    public static function descriptor(object $obj): array
    {
        return [
            '__phandle' => self::register($obj),
            '__class' => get_class($obj),
            '__implements' => array_values(class_implements($obj)),
        ];
    }
}

final class ShirabeRpcRuntime
{
    /** @var resource */
    public static $socket;
    public static ?string $stubsDir = null;
    /** @var ?callable(string): void */
    public static $stubAutoloader = null;
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
            $descriptor = $value->__shirabeRustHandleDescriptor();
            if ($descriptor !== null) {
                return $descriptor;
            }
            // A natively-constructed dual-mode instance falls through to the P table below.
        }
        if (is_object($value)) {
            // A materialized value needs no descriptor: it crosses as the object record
            // serialize() writes for it, which the Rust side decodes into its own value.
            $materialized = \Shirabe\MaterializedValue::forWire($value);
            if ($materialized !== null) {
                return $materialized;
            }
            return ShirabePhpObjectRegistry::descriptor($value);
        }
        if (is_resource($value)) {
            throw new RuntimeException('a PHP resource cannot cross the RPC boundary');
        }
        if (is_array($value)) {
            return array_map([self::class, 'toWire'], $value);
        }
        return $value;
    }

    /**
     * Converts a decoded wire value: handle descriptor arrays become live objects. A
     * materialized value arrives as a real instance already, revived by unserialize().
     */
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
            return ShirabePhpObjectRegistry::get((int) $value['__phandle']);
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
                $fields = unserialize($payload, ['allowed_classes' => \Shirabe\MaterializedValue::CLASSES]);
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
        $fields = unserialize($payload, ['allowed_classes' => \Shirabe\MaterializedValue::CLASSES]);
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
                [$class, $ctorArgs] = $fields;
                self::replyWith($corrId, static function () use ($class, $ctorArgs) {
                    $ctorArgs = ShirabeRpcRuntime::fromWire($ctorArgs);
                    if (!class_exists($class)) {
                        throw new RuntimeException("PHP class `{$class}` does not exist");
                    }
                    return new $class(...$ctorArgs);
                });
                break;
            case SHIRABE_TAG_CALL_PHP_METHOD:
                [$phandle, $method, $args] = $fields;
                self::replyWith($corrId, static function () use ($phandle, $method, $args) {
                    $obj = ShirabePhpObjectRegistry::get((int) $phandle);
                    $args = ShirabeRpcRuntime::fromWire($args);
                    if (!is_callable([$obj, $method])) {
                        throw new RuntimeException(
                            get_class($obj) . "::{$method} is not callable"
                        );
                    }
                    return $obj->$method(...$args);
                });
                break;
            case SHIRABE_TAG_RELEASE_PHP_HANDLE:
                [$phandle] = $fields;
                ShirabePhpObjectRegistry::release((int) $phandle);
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

    /** Re-prepends the stub autoloader so it precedes any autoloader registered since. */
    public static function ensureStubAutoloaderPriority(): void
    {
        if (self::$stubAutoloader === null) {
            return;
        }
        spl_autoload_unregister(self::$stubAutoloader);
        spl_autoload_register(self::$stubAutoloader, true, true);
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

// The socket is one end of a socketpair the parent installed on this descriptor before exec;
// there is nothing to connect to. See docs/dev/php-rpc.md.
$client = @fopen('php://fd/' . $argv[1], 'r+b');
if ($client === false) {
    exit(1);
}
// Frames must reach the parent as they are written, not when a buffer happens to fill.
stream_set_write_buffer($client, 0);
ShirabeRpcRuntime::$socket = $client;
ShirabeRpcRuntime::$stubsDir = $argv[2] ?? null;

// Proxy stub classes take priority over any other autoloader (including autoloaders that a
// script or the composer runtime registers later), so a proxied FQCN can never be shadowed by
// the real implementation. `__shirabe_require` re-prepends this closure after loading code
// that registers its own prepending autoloader.
ShirabeRpcRuntime::$stubAutoloader = static function (string $class): void {
    if (ShirabeRpcRuntime::$stubsDir === null) {
        return;
    }
    $file = ShirabeRpcRuntime::$stubsDir . '/' . str_replace('\\', '/', $class) . '.php';
    if (is_file($file)) {
        require $file;
    }
};
spl_autoload_register(ShirabeRpcRuntime::$stubAutoloader, true, true);

/**
 * Port of Composer\Platform\Runtime, whose runtime queries feed the `platform` payload. Its
 * parseHtmlExtensionInfo() has no counterpart here: the worker is always the CLI SAPI, so
 * getExtensionInfo() never takes the branch that reformats phpinfo()'s HTML output.
 */
final class ShirabePlatformRuntime
{
    /** The constants the `platform` payload reports, named ltrim($class.'::'.$constant, ':'). */
    public const CONSTANTS = [
        'PHP_VERSION',
        'PHP_DEBUG',
        'PHP_ZTS',
        'PHP_INT_SIZE',
        'AF_INET6',
        'GD_VERSION',
        'GMP_VERSION',
        'ICONV_VERSION',
        'INTL_ICU_VERSION',
        'LIBXML_DOTTED_VERSION',
        'MB_ONIGURUMA_VERSION',
        'OPENSSL_VERSION_TEXT',
        'PCRE_VERSION',
        'PGSQL_LIBPQ_VERSION',
        'RD_KAFKA_VERSION',
        'SODIUM_LIBRARY_VERSION',
        'LIBXSLT_DOTTED_VERSION',
        'ZipArchive::LIBZIP_VERSION',
        'ZLIB_VERSION',
    ];

    /** The extensions whose info() output the `platform` payload reports when they are loaded. */
    public const EXTENSION_INFO = [
        'amqp',
        'bz2',
        'curl',
        'date',
        'fileinfo',
        'gd',
        'intl',
        'ldap',
        'mbstring',
        'memcached',
        'mongodb',
        'mysqlnd',
        'pcre',
        'pdo_mysql',
        'pdo_pgsql',
        'pdo_sqlite',
        'pgsql',
        'pq',
        'sqlite3',
        'ssh2',
        'xsl',
        'yaml',
        'zlib',
    ];

    /** The classes the `platform` payload reports the existence of. */
    public const CLASSES = ['ResourceBundle', 'IntlChar'];

    public static function getExtensionInfo(string $extension): string
    {
        $reflector = new ReflectionExtension($extension);

        ob_start();
        $reflector->info();

        return (string) ob_get_clean();
    }
}

// Port of Composer\XdebugHandler\XdebugHandler::setXdebugDetails(), which the `xdebug_active`
// query and the diagnose payload both report.
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
    'xdebug_active' => static fn($args) => $xdebug_active(),
    'get_all_ini_files' => static function ($args) {
        $paths = [(string) php_ini_loaded_file()];
        $scanned = php_ini_scanned_files();
        if ($scanned !== false) {
            $paths = array_merge($paths, array_map('trim', explode(',', $scanned)));
        }
        return $paths;
    },
    'platform' => static function ($args) {
        $extensions = get_loaded_extensions();

        $extension_versions = [];
        foreach ($extensions as $extension) {
            $version = phpversion($extension);
            $extension_versions[$extension] = $version !== false ? $version : '0';
        }

        $extension_info = [];
        foreach (ShirabePlatformRuntime::EXTENSION_INFO as $extension) {
            if (in_array($extension, $extensions, true)) {
                $extension_info[$extension] = ShirabePlatformRuntime::getExtensionInfo($extension);
            }
        }

        // Only the defined constants carry a value; `constant_names` tells the Rust side which
        // names were looked up, so a name it reads but this list omits is an error rather than a
        // silently undefined constant.
        $constants = [];
        foreach (ShirabePlatformRuntime::CONSTANTS as $constant) {
            if (defined($constant)) {
                $constants[$constant] = constant($constant);
            }
        }

        $classes = [];
        foreach (ShirabePlatformRuntime::CLASSES as $class) {
            $classes[$class] = class_exists($class, false);
        }

        // The values below stand in for the PHP objects and calls PlatformRepository reaches
        // through Composer\Platform\Runtime::invoke()/construct(), reduced to the entries it reads.
        $resource_bundle = null;
        if ($classes['ResourceBundle']) {
            $bundle = ResourceBundle::create('root', 'ICUDATA', false);
            if ($bundle !== null) {
                $resource_bundle = ['Version' => $bundle->get('Version')];
            }
        }

        return [
            'extensions' => $extensions,
            'extension_versions' => $extension_versions,
            'extension_info' => $extension_info,
            'constant_names' => ShirabePlatformRuntime::CONSTANTS,
            'constants' => $constants,
            'classes' => $classes,
            'curl_version' => extension_loaded('curl') ? curl_version() : null,
            'inet_pton_ipv6' => @inet_pton('::'),
            'resource_bundle' => $resource_bundle,
            'intl_char_unicode_version' => $classes['IntlChar'] ? IntlChar::getUnicodeVersion() : null,
            'imagick' => extension_loaded('imagick') ? (new Imagick())->getVersion() : null,
        ];
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
    '__shirabe_oracle_roundtrip' => static fn($args) => serialize(
        unserialize($args[0], ['allowed_classes' => \Shirabe\MaterializedValue::CLASSES])
    ),
    '__shirabe_require' => static function ($args) {
        require_once $args[0];
        // The required file may have registered further prepending autoloaders (a Composer
        // vendor/autoload.php prepends its ClassLoader); proxied FQCNs must stay resolvable to
        // the stub classes, so the stub autoloader is moved back to the front of the stack.
        ShirabeRpcRuntime::ensureStubAutoloaderPriority();
        return true;
    },
    '__shirabe_enable_script_autoloader' => static function ($args) {
        ShirabeRpcRuntime::enableScriptAutoloader();
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
    // TODO(plugin): seeding the initial state when the plugin runtime boots (the Factory-time
    // safelyLoadInstalledVersions of the project's installed.php) is not wired yet; until the
    // first write of a run, a plugin observes an unseeded InstalledVersions.
    '__shirabe_installed_versions_reload' => static function ($args) {
        [$versions, $repoDir] = $args;
        if (!class_exists('Composer\\InstalledVersions')) {
            return true;
        }
        \Composer\InstalledVersions::reload($versions);
        try {
            $reflProp = new ReflectionProperty(\Composer\InstalledVersions::class, 'selfDir');
            (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
            $reflProp->setValue(null, strtr($repoDir, '\\', '/'));

            $reflProp = new ReflectionProperty(\Composer\InstalledVersions::class, 'installedIsLocalDir');
            (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
            $reflProp->setValue(null, true);
        } catch (ReflectionException $e) {
            if (preg_match('{Property .*? does not exist}i', $e->getMessage()) !== 1) {
                throw $e;
            }
            // noop, an outdated InstalledVersions class simply lacks the properties
        }
        return true;
    },
    // An already-fulfilled promise for a Rust-side call whose PHP signature declares
    // PromiseInterface. The Rust future ran to completion before this is called, so there is
    // nothing left to defer; see .ken/plugin-arch/design.md §10.1.6.
    '__shirabe_resolved_promise' => static function ($args) {
        if (!function_exists('React\\Promise\\resolve')) {
            throw new RuntimeException(
                'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
            );
        }
        return \React\Promise\resolve($args[0]);
    },
    // Drains a promise a plugin returned to the Rust side. React settles promises
    // synchronously, so an already-settled one runs these handlers during then(); one that is
    // still pending is an explicit error rather than a silently dropped continuation.
    '__shirabe_settle_promise' => static function ($args) {
        $promise = $args[0];
        if (!$promise instanceof \React\Promise\PromiseInterface) {
            throw new RuntimeException('__shirabe_settle_promise expects a promise handle');
        }
        $settled = false;
        $value = null;
        $rejected = false;
        $reason = null;
        $promise->then(
            static function ($result) use (&$settled, &$value) {
                $settled = true;
                $value = $result;
            },
            static function ($error) use (&$settled, &$rejected, &$reason) {
                $settled = true;
                $rejected = true;
                $reason = $error;
            }
        );
        if (!$settled) {
            throw new RuntimeException(
                'the promise returned to Shirabe is still pending; deferred resolution across the'
                . ' RPC boundary is not implemented yet'
            );
        }
        if ($rejected) {
            throw $reason instanceof Throwable
                ? $reason
                : new RuntimeException('the promise returned to Shirabe was rejected with ' . gettype($reason));
        }
        return $value;
    },
    // For testing only: reads a public property of a P-table entity (PHPUnit asserts like
    // `$plugins[0]->version` have no method to call).
    '__shirabe_get_property' => static function ($args) {
        $obj = ShirabeRpcRuntime::fromWire($args[0]);
        if (!is_object($obj)) {
            throw new RuntimeException('__shirabe_get_property expects a handle argument');
        }
        return $obj->{$args[1]};
    },
    // Builds the worker-side Composer\Console\Application (the runtime/ definition, not the
    // real class) from the Rust handoff; the caller keeps the returned handle and runs
    // plugin-provided commands through __shirabe_run_console_application.
    '__shirabe_console_application_boot' => static function ($args) {
        return \Composer\Console\Application::__shirabeBoot($args[0]);
    },
    // Runs one command line (the stringified input of the Rust-side run) through a booted
    // worker-side application; output goes to the inherited stdio, the exit code returns
    // over the wire, and a command failure propagates as an RPC throw (catchExceptions is
    // off on the booted application).
    '__shirabe_run_console_application' => static function ($args) {
        [$app, $inputString] = $args;
        if (!$app instanceof \Composer\Console\Application) {
            throw new RuntimeException('__shirabe_run_console_application expects an application handle');
        }
        return $app->run(new \Symfony\Component\Console\Input\StringInput($inputString));
    },
    // Reads a command's input definition (plus help text and extra usages) as plain data, so
    // the Rust side can mirror it for `help`/`list` rendering without executing anything.
    '__shirabe_read_command_definition' => static function ($args) {
        $command = $args[0];
        if (!$command instanceof \Symfony\Component\Console\Command\Command) {
            throw new RuntimeException('__shirabe_read_command_definition expects a command handle');
        }
        $definition = $command->getDefinition();
        $arguments = [];
        foreach ($definition->getArguments() as $argument) {
            $arguments[] = [
                'name' => $argument->getName(),
                'required' => $argument->isRequired(),
                'isArray' => $argument->isArray(),
                'description' => $argument->getDescription(),
                'default' => $argument->getDefault(),
            ];
        }
        $options = [];
        foreach ($definition->getOptions() as $option) {
            $options[] = [
                'name' => $option->getName(),
                'shortcut' => $option->getShortcut(),
                'acceptValue' => $option->acceptValue(),
                'isValueRequired' => $option->isValueRequired(),
                'isArray' => $option->isArray(),
                'isNegatable' => $option->isNegatable(),
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
];

ShirabeRpcRuntime::serveForever();
