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

const SHIRABE_MAX_FRAME_LEN = 268435456; // 256 MiB, mirrored on the Rust side.

/** Marker interface every Rust-proxy stub class implements. */
interface ShirabeRustStub
{
    /**
     * Null when this instance holds no Rust handle: a dual-mode class (see php/runtime/) that
     * was constructed natively in this process crosses the wire as a P-table entity instead.
     *
     * @return ?array{__rhandle: int, __class: string}
     */
    public function __shirabeRustHandleDescriptor(): ?array;

    /** Binds this stub to an existing Rust entity, in place of running its constructor. */
    public function __shirabeBind(int $rhandle): void;
}

/**
 * Raised by the guard classes (php/guards/): classes whose entity lives on the Rust side but that
 * this worker has no proxy for. Their declaration shadows the real Composer one, so running the
 * real implementation here — on an instance the Rust side never sees — fails loudly instead.
 */
final class ShirabeUnsupportedClass
{
    public static function fail(string $class, string $member): void
    {
        throw new RuntimeException(
            "{$class}::{$member}() is not available in the plugin runtime: the Rust side owns "
            . 'this class and there is no proxy for it here, so the real implementation would '
            . 'run on an instance the Rust side never sees.'
        );
    }
}

/** Interns proxy stubs so the same Rust handle always yields the same stub instance. */
final class ShirabeRustObjectRegistry
{
    /** @var array<int, WeakReference> */
    private static array $internTable = [];

    public static function stubFor(int $rhandle, string $class): object
    {
        if (isset(self::$internTable[$rhandle])) {
            $existing = self::$internTable[$rhandle]->get();
            if ($existing !== null) {
                return $existing;
            }
        }
        // A guard class exists for every proxied FQCN without a stub, so class_exists() alone no
        // longer tells the two apart; only a stub can be bound to a Rust handle.
        if (!class_exists($class) || !is_a($class, ShirabeRustStub::class, true)) {
            throw new RuntimeException(
                "no proxy stub class is available for {$class}"
            );
        }
        // The stub's constructor belongs to plugin code building a *new* entity; an entity that
        // already exists is bound directly, so proxying never runs it.
        $stub = (new ReflectionClass($class))->newInstanceWithoutConstructor();
        $stub->__shirabeBind($rhandle);
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

/**
 * The worker's half of the job registry (docs/dev/plugin-async.md). A job is an asynchronous
 * operation one side runs and the other side holds a promise for; only its id crosses the wire.
 * The Rust side allocates odd ids, this side even ones.
 */
final class ShirabeJobRegistry
{
    /**
     * Rust-owned jobs that have not settled yet. The entry holds the Deferred strongly: it has
     * to outlive the plugin dropping its promise, until the Rust side reports the outcome.
     *
     * @var array<int, \React\Promise\Deferred>
     */
    private static array $deferreds = [];
    /**
     * PHP-owned jobs that have not settled yet. Holding the promise keeps its object id from
     * being reused while the id is interned.
     *
     * @var array<int, \React\Promise\PromiseInterface>
     */
    private static array $promises = [];
    /** @var array<int, int> spl_object_id => job id, so one promise keeps one id */
    private static array $idsByObjectId = [];
    /**
     * Settled promises built here on the Rust side's behalf, each waiting for the answer that
     * carries its id back.
     *
     * @var array<int, \React\Promise\PromiseInterface>
     */
    private static array $parked = [];
    private static int $nextId = 2;

    private static function requireReact(): void
    {
        if (!class_exists(\React\Promise\Deferred::class)) {
            throw new RuntimeException(
                'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
            );
        }
    }

    /** The promise standing for a job id that arrived over the wire. */
    public static function promiseFor(int $id): \React\Promise\PromiseInterface
    {
        if ($id % 2 === 0) {
            if (isset(self::$parked[$id])) {
                $promise = self::$parked[$id];
                unset(self::$parked[$id]);
                return $promise;
            }
            if (!isset(self::$promises[$id])) {
                throw new RuntimeException("unknown PHP-owned job {$id}");
            }
            return self::$promises[$id];
        }
        if (!isset(self::$deferreds[$id])) {
            self::requireReact();
            self::$deferreds[$id] = new \React\Promise\Deferred(static function () use ($id): void {
                // The Rust side drops the job, so no settle push follows either way. It answers
                // with the exception a started job is cancelled with, and with null for one that
                // had not started, which Composer aborts without rejecting.
                unset(self::$deferreds[$id]);
                $thrown = ShirabeRpcRuntime::callRust(0, '__shirabeCancelJob', [$id]);
                if ($thrown !== null) {
                    throw \Shirabe\MaterializedThrowable::revive($thrown);
                }
            });
        }
        return self::$deferreds[$id]->promise();
    }

    private static function take(int $id): \React\Promise\Deferred
    {
        if (!isset(self::$deferreds[$id])) {
            throw new RuntimeException("unknown Rust-owned job {$id}");
        }
        $deferred = self::$deferreds[$id];
        unset(self::$deferreds[$id]);
        return $deferred;
    }

    /** Runs the callbacks chained on the job's promise before it returns. */
    public static function resolve(int $id, $value): void
    {
        self::take($id)->resolve($value);
    }

    public static function reject(int $id, \Throwable $reason): void
    {
        self::take($id)->reject($reason);
    }

    /**
     * The descriptor of a settled promise the Rust side asked for. It names no job: the Rust
     * side passes it on as the answer of the call it is serving, and the promise is handed out
     * when that answer arrives.
     *
     * @return array{__job: int}
     */
    public static function park(\React\Promise\PromiseInterface $promise): array
    {
        $id = self::$nextId;
        self::$nextId += 2;
        self::$parked[$id] = $promise;
        return ['__job' => $id];
    }

    /**
     * The descriptor of a promise leaving for the Rust side. The recorder attached here reports
     * the outcome when the promise settles, which is right away for one that already has.
     *
     * @return array{__job: int}
     */
    public static function descriptor(\React\Promise\PromiseInterface $promise): array
    {
        $objectId = spl_object_id($promise);
        if (isset(self::$idsByObjectId[$objectId])) {
            return ['__job' => self::$idsByObjectId[$objectId]];
        }
        $id = self::$nextId;
        self::$nextId += 2;
        self::$idsByObjectId[$objectId] = $id;
        self::$promises[$id] = $promise;
        $forget = static function () use ($id, $objectId): void {
            unset(self::$promises[$id], self::$idsByObjectId[$objectId]);
        };
        $promise->then(
            static function ($value) use ($id, $forget): void {
                $forget();
                ShirabeRpcRuntime::callRust(0, '__shirabeResolveJob', [$id, $value]);
            },
            static function ($reason) use ($id, $forget): void {
                $forget();
                if (!$reason instanceof Throwable) {
                    $reason = new RuntimeException('the promise was rejected with ' . gettype($reason));
                }
                ShirabeRpcRuntime::callRust(
                    0,
                    '__shirabeRejectJob',
                    [$id, \Shirabe\MaterializedThrowable::describe($reason)]
                );
            }
        );
        return ['__job' => $id];
    }
}

final class ShirabeRpcRuntime
{
    /** @var resource */
    public static $socket;
    public static ?string $stubsDir = null;
    public static ?string $guardsDir = null;
    /** @var ?callable(string): void */
    public static $stubAutoloader = null;
    /** @var array<string, callable(array): mixed> */
    public static array $dispatch = [];
    /** Even correlation ids; the Rust side allocates odd ones. */
    private static int $nextCorrId = 2;
    private static bool $scriptAutoloaderRegistered = false;
    private static bool $shuttingDown = false;
    /**
     * The worker-side Loop of each Composer entity, with the stub it was built for. Holding the
     * stub keeps the entity's handle, and with it this entry, alive for the rest of the run.
     *
     * @var array<int, array{0: object, 1: \Composer\Util\Loop}>
     */
    private static array $loops = [];

    /**
     * Serves `Composer::getLoop()`: the Loop is a two-world class, so the one a plugin gets is
     * built here over the proxies of the two services the Rust-side Loop drives.
     */
    public static function loopFor(object $composer, int $rhandle): \Composer\Util\Loop
    {
        if (!isset(self::$loops[$rhandle])) {
            [$httpDownloader, $processExecutor] = self::callRust($rhandle, '__shirabeLoopServices', []);
            $loop = new \Composer\Util\Loop($httpDownloader, $processExecutor);
            $loop->__shirabeBindToExecution();
            self::$loops[$rhandle] = [$composer, $loop];
        }
        return self::$loops[$rhandle][1];
    }

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
        if ($value instanceof \React\Promise\PromiseInterface) {
            return ShirabeJobRegistry::descriptor($value);
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
     *
     * TODO(type-model): a descriptor travels in-band as a plain array, so its exact key set is
     * the only thing separating it from plugin data of the same shape. The __rhandle and __phandle
     * checks below match on the reserved key alone, so an array a plugin built with a __rhandle or
     * __phandle key of its own is read as a handle rather than kept as data. The Rust half
     * (`decode_handle` in `src/value.rs`) matches the whole key set, and diverges the other way.
     * __job is a single-key descriptor and matches on that.
     */
    public static function fromWire($value)
    {
        if (!is_array($value)) {
            return $value;
        }
        if (isset($value['__rhandle'])) {
            return ShirabeRustObjectRegistry::stubFor(
                $value['__rhandle'],
                $value['__class']
            );
        }
        if (isset($value['__phandle'])) {
            return ShirabePhpObjectRegistry::get((int) $value['__phandle']);
        }
        if (isset($value['__job']) && count($value) === 1) {
            return ShirabeJobRegistry::promiseFor((int) $value['__job']);
        }
        return array_map([self::class, 'fromWire'], $value);
    }

    /**
     * Sends a CallRustMethod request and drives the cooperative loop until its Return.
     *
     * For a method with by-ref parameters, the Rust side answers with the value each of them
     * holds afterwards, keyed by position, and the caller (a generated stub) assigns those back
     * to its own by-ref parameters.
     *
     * @param array<int, mixed>|null $outParams
     */
    public static function callRust(
        int $rhandle,
        string $method,
        array $args,
        ?array &$outParams = null
    ) {
        $corrId = self::$nextCorrId;
        self::$nextCorrId += 2;
        self::writeFrame(
            SHIRABE_TAG_CALL_RUST_METHOD,
            $corrId,
            serialize([$rhandle, $method, self::toWire($args)])
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
                    $outParams = self::fromWire($fields[1] ?? []);
                    return self::fromWire($fields[0]);
                }
                throw \Shirabe\MaterializedThrowable::revive(self::fromWire($fields));
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
                serialize(self::toWire(\Shirabe\MaterializedThrowable::describe($e)))
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
ShirabeRpcRuntime::$guardsDir = $argv[3] ?? null;

// Proxy stub classes take priority over any other autoloader (including autoloaders that a
// script or the composer runtime registers later), so a proxied FQCN can never be shadowed by
// the real implementation. `__shirabe_require` re-prepends this closure after loading code
// that registers its own prepending autoloader. The guards come after the stubs: they cover the
// Rust-owned classes no stub proxies, so the two directories never hold the same FQCN.
ShirabeRpcRuntime::$stubAutoloader = static function (string $class): void {
    $relative = '/' . str_replace('\\', '/', $class) . '.php';
    foreach ([ShirabeRpcRuntime::$stubsDir, ShirabeRpcRuntime::$guardsDir] as $dir) {
        if ($dir !== null && is_file($dir . $relative)) {
            require $dir . $relative;
            return;
        }
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
    // The names reported on arrive with the query: constants as ltrim($class.'::'.$constant, ':'),
    // classes, and the extensions whose info() output is reported when they are loaded.
    'platform' => static function ($args) {
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
                $extension_info[$extension] = ShirabePlatformRuntime::getExtensionInfo($extension);
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
            $bundle = ResourceBundle::create('root', 'ICUDATA', false);
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
            'intl_char_unicode_version' => class_exists('IntlChar', false) ? IntlChar::getUnicodeVersion() : null,
            'imagick_version_string' => extension_loaded('imagick') ? (new Imagick())->getVersion()['versionString'] : null,
        ];
    },
    // The extensions, functions and ini settings reported on arrive with the query.
    'diagnose' => static function ($args) use ($xdebug_active) {
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
            'xdebug_active' => $xdebug_active(),
            'ioncube_loader_iversion' => extension_loaded('ionCube Loader') ? ioncube_loader_iversion() : 0,
            'ioncube_loader_version' => extension_loaded('ionCube Loader') ? ioncube_loader_version() : '',
            'phpinfo_general' => $phpinfo,
            'curl' => $curl,
            'extensions' => $extensions,
            'functions' => $functions,
            'ini_settings' => $ini_settings,
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
    // Tries to open the embedded runtime bundle and maps the phar to $alias.
    // Returns whether the extraction succeeds or not.
    '__shirabe_open_runtime_bundle' => static function ($args) {
        [$executable, $alias, $sentinel, $bundleId] = $args;
        if (!extension_loaded('phar') || !in_array('phar', stream_get_wrappers(), true)) {
            return false;
        }
        try {
            Phar::loadPhar($executable, $alias);
            $read = @file_get_contents("phar://{$alias}/{$sentinel}");
        } catch (Throwable $e) {
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
    // PromiseInterface and that has the value before a job exists.
    '__shirabe_resolved_promise' => static function ($args) {
        if (!function_exists('React\\Promise\\resolve')) {
            throw new RuntimeException(
                'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
            );
        }
        return ShirabeJobRegistry::park(\React\Promise\resolve($args[0]));
    },
    // The failure half: a job that failed before it could be registered surfaces to the caller
    // as a rejection it handles, the way it does under Composer, rather than as a throw out of
    // the call that started it.
    '__shirabe_rejected_promise' => static function ($args) {
        if (!function_exists('React\\Promise\\reject')) {
            throw new RuntimeException(
                'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
            );
        }
        return ShirabeJobRegistry::park(\React\Promise\reject(
            \Shirabe\MaterializedThrowable::revive($args[0])
        ));
    },
    // The settle pushes of a Rust-owned job, sent from inside the tick that saw it finish. The
    // callbacks plugin code chained on the job's promise run before these return.
    '__shirabe_resolve_job' => static function ($args) {
        ShirabeJobRegistry::resolve((int) $args[0], $args[1]);
        return null;
    },
    '__shirabe_reject_job' => static function ($args) {
        [$id, $description] = $args;
        ShirabeJobRegistry::reject((int) $id, \Shirabe\MaterializedThrowable::revive($description));
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
                    throw new RuntimeException("__shirabe_sync_env got an unknown storage `{$storage}`");
            }
        }
        return true;
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
    // For testing only: the ReflectionMethod / ReflectionProperty access PHPUnit tests use to
    // reach the non-public members of a P-table entity.
    '__shirabe_reflection_invoke' => static function ($args) {
        [$obj, $method, $methodArgs] = $args;
        $reflMethod = new ReflectionMethod($obj, $method);
        (\PHP_VERSION_ID < 80100) and $reflMethod->setAccessible(true);
        return $reflMethod->invokeArgs($obj, $methodArgs);
    },
    '__shirabe_reflection_get_property' => static function ($args) {
        [$obj, $name] = $args;
        $reflProp = new ReflectionProperty($obj, $name);
        (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
        return $reflProp->getValue($obj);
    },
    '__shirabe_reflection_set_property' => static function ($args) {
        [$obj, $name, $value] = $args;
        $reflProp = new ReflectionProperty($obj, $name);
        (\PHP_VERSION_ID < 80100) and $reflProp->setAccessible(true);
        $reflProp->setValue($obj, $value);
        return null;
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
];

ShirabeRpcRuntime::serveForever();
