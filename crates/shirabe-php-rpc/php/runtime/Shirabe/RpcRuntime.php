<?php

namespace Shirabe;

final class RpcRuntime
{
    private const TAG_CALL_FUNCTION = 0x00;
    private const TAG_CALL_STATIC_METHOD = 0x01;
    private const TAG_NEW_OBJECT = 0x02;
    private const TAG_CALL_PHP_METHOD = 0x03;
    private const TAG_CALL_RUST_METHOD = 0x04;
    private const TAG_RETURN = 0x05;
    private const TAG_THROW = 0x06;
    private const TAG_RELEASE_RUST_HANDLE = 0x07;
    private const TAG_RELEASE_PHP_HANDLE = 0x08;

    private const MAX_FRAME_LEN = 268435456; // 256 MiB, mirrored on the Rust side.

    /** @var resource */
    public static $socket;
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
            [$httpDownloader, $processExecutor] = self::callRust($rhandle, '__shirabe_loop_services', []);
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
        if ($len < 9 || $len > self::MAX_FRAME_LEN) {
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
        self::writeFrame(self::TAG_RELEASE_RUST_HANDLE, 0, serialize([$rhandle]));
    }

    /**
     * Converts a value about to be serialized onto the wire: a proxy stub, a promise and any
     * other object without a materialized value become descriptors; scalars and arrays pass
     * through. Every object crosses as a descriptor or a materialized value, so a descriptor
     * record on the wire never stands for an object of plugin code, even one of a descriptor
     * class.
     */
    public static function toPluginValue($value)
    {
        if ($value instanceof RustStub) {
            $descriptor = $value->__shirabeRustHandleDescriptor();
            if ($descriptor !== null) {
                return $descriptor;
            }
            // A natively-constructed dual-mode instance falls through to the P table below.
        }
        if ($value instanceof \React\Promise\PromiseInterface) {
            return JobRegistry::descriptor($value);
        }
        if (is_object($value)) {
            // A materialized value needs no descriptor: it crosses as the object record
            // serialize() writes for it, which the Rust side decodes into its own value.
            $materialized = MaterializedValue::forWire($value);
            if ($materialized !== null) {
                return $materialized;
            }
            return PhpObjectRegistry::descriptor($value);
        }
        if (is_resource($value)) {
            throw new \RuntimeException('a PHP resource cannot cross the RPC boundary');
        }
        if (is_array($value)) {
            return array_map([self::class, 'toPluginValue'], $value);
        }
        return $value;
    }

    /**
     * Converts a decoded plugin value: descriptors become live objects. A materialized value
     * arrives as a real instance already, revived by unserialize().
     */
    public static function fromPluginValue($value)
    {
        if ($value instanceof Wire\RustHandle) {
            return RustObjectRegistry::stubFor($value->rhandle, $value->class);
        }
        if ($value instanceof Wire\PhpHandle) {
            return PhpObjectRegistry::get($value->phandle);
        }
        if ($value instanceof Wire\Job) {
            return JobRegistry::promiseFor($value->id);
        }
        if (is_array($value)) {
            return array_map([self::class, 'fromPluginValue'], $value);
        }
        return $value;
    }

    /**
     * Decodes a frame payload. The descriptor records and the materialized values are the only
     * classes a payload may name.
     */
    public static function unserializePayload(string $payload)
    {
        return unserialize($payload, ['allowed_classes' => array_merge(
            [Wire\RustHandle::class, Wire\PhpHandle::class, Wire\Job::class],
            MaterializedValue::CLASSES
        )]);
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
            self::TAG_CALL_RUST_METHOD,
            $corrId,
            serialize([$rhandle, $method, self::toPluginValue($args)])
        );
        while (true) {
            $frame = self::readFrame();
            if ($frame === null) {
                self::fail('connection lost while waiting for a Return from Rust');
            }
            [$tag, $inId, $payload] = $frame;
            if ($tag === self::TAG_RETURN || $tag === self::TAG_THROW) {
                if ($inId !== $corrId) {
                    self::fail("protocol violation: response for unexpected corr_id {$inId}");
                }
                $fields = self::unserializePayload($payload);
                if (!is_array($fields)) {
                    self::fail('protocol violation: unparseable response payload');
                }
                if ($tag === self::TAG_RETURN) {
                    $outParams = self::fromPluginValue($fields[1] ?? []);
                    return self::fromPluginValue($fields[0]);
                }
                throw MaterializedThrowable::revive(self::fromPluginValue($fields));
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
        $fields = self::unserializePayload($payload);
        if (!is_array($fields)) {
            self::fail('protocol violation: unparseable frame payload');
        }
        switch ($tag) {
            case self::TAG_CALL_FUNCTION:
                [$name, $args] = $fields;
                self::replyWith($corrId, static function () use ($name, $args) {
                    $args = RpcRuntime::fromPluginValue($args);
                    if (isset(RpcRuntime::$dispatch[$name])) {
                        return (RpcRuntime::$dispatch[$name])($args);
                    }
                    if (function_exists($name)) {
                        return $name(...$args);
                    }
                    throw new \RuntimeException("PHP function `{$name}` does not exist");
                });
                break;
            case self::TAG_CALL_STATIC_METHOD:
                [$class, $method, $args] = $fields;
                self::replyWith($corrId, static function () use ($class, $method, $args) {
                    $args = RpcRuntime::fromPluginValue($args);
                    if (!is_callable([$class, $method])) {
                        throw new \RuntimeException("{$class}::{$method} is not callable");
                    }
                    return $class::$method(...$args);
                });
                break;
            case self::TAG_NEW_OBJECT:
                [$class, $ctorArgs] = $fields;
                self::replyWith($corrId, static function () use ($class, $ctorArgs) {
                    $ctorArgs = RpcRuntime::fromPluginValue($ctorArgs);
                    if (!class_exists($class)) {
                        throw new \RuntimeException("PHP class `{$class}` does not exist");
                    }
                    return new $class(...$ctorArgs);
                });
                break;
            case self::TAG_CALL_PHP_METHOD:
                [$phandle, $method, $args] = $fields;
                self::replyWith($corrId, static function () use ($phandle, $method, $args) {
                    $obj = PhpObjectRegistry::get((int) $phandle);
                    $args = RpcRuntime::fromPluginValue($args);
                    if (!is_callable([$obj, $method])) {
                        throw new \RuntimeException(
                            get_class($obj) . "::{$method} is not callable"
                        );
                    }
                    return $obj->$method(...$args);
                });
                break;
            case self::TAG_RELEASE_PHP_HANDLE:
                [$phandle] = $fields;
                PhpObjectRegistry::release((int) $phandle);
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
                self::TAG_RETURN,
                $corrId,
                serialize([self::toPluginValue($result), []])
            );
        } catch (\Throwable $e) {
            self::writeFrame(
                self::TAG_THROW,
                $corrId,
                serialize(self::toPluginValue(MaterializedThrowable::describe($e)))
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
            $file = RpcRuntime::callRust(0, '__shirabe_find_file', [$class]);
            if (is_string($file) && $file !== '') {
                require $file;
            }
        });
    }
}
