<?php

// The PHP half of the throwable record codec (`PhpThrow::to_rpc_value` / `PhpThrow::from_rpc_value`
// on the Rust side), used wherever an exception crosses: a Throw frame, a promise rejection, or a
// value an entry point answers with. A failure crosses with the class it was thrown as, so `catch
// (TransportException $e)` on either side catches what it would catch under Composer, plus the
// state that class declares beyond message and code and the place it was thrown at.

namespace Shirabe;

final class ThrowableRecord
{
    // ref: composer/src/Composer/Downloader/TransportException.php sha1:b7bd47db7402a92d600ea4ce8abd9c66d23f1b27
    // ref: composer/src/Composer/Package/Loader/InvalidPackageException.php sha1:be37904970764b3091d04e97838ad5969a8a12c3
    // ref: composer/src/Composer/Json/JsonValidationException.php sha1:12472643e8abf0e6f472b8f333040bf2d4501f24
    // ref: composer/vendor/symfony/console/Exception/CommandNotFoundException.php sha1:4cbfe3d0bd964e1153c09786e456a73af9234eb9
    /**
     * The state each class whose state the Rust side ports declares beyond message and code, by
     * the names it declares it under.
     */
    private const PROPERTIES = [
        'Composer\Downloader\TransportException' => ['headers', 'response', 'statusCode', 'responseInfo'],
        'Composer\Package\Loader\InvalidPackageException' => ['errors', 'warnings', 'data'],
        'Composer\Json\JsonValidationException' => ['errors'],
        'Symfony\Component\Console\Exception\CommandNotFoundException' => ['alternatives'],
    ];

    /**
     * The throwable record of an exception raised in this process: its fields are the class,
     * message, code, declared state, file, line and the superclasses, nearest first.
     *
     * @return array{string, string, int, array<string, mixed>, string, int, list<string>}
     */
    public static function fromThrowable(\Throwable $e): array
    {
        $properties = [];
        foreach (self::PROPERTIES as $class => $names) {
            if (!$e instanceof $class) {
                continue;
            }
            foreach ($names as $name) {
                $property = new \ReflectionProperty($class, $name);
                $property->setAccessible(true);
                $properties[$name] = $property->getValue($e);
            }
        }

        return [
            get_class($e),
            $e->getMessage(),
            (int) $e->getCode(),
            $properties,
            $e->getFile(),
            $e->getLine(),
            array_values(class_parents($e)),
        ];
    }

    /**
     * Rebuilds the exception a throwable record in the shape fromThrowable() writes stands for. The
     * superclasses only matter to the Rust side, which has no type for every class. A class the
     * child cannot construct keeps the RuntimeException shape, which is all the record
     * guarantees.
     *
     * @param array{string, string, int, array<string, mixed>, string, int, list<string>} $record
     */
    public static function revive(array $record): \Throwable
    {
        [$class, $message, $code, $properties, $file, $line] = $record;
        $exception = self::instantiate($class, $message, $code);
        foreach ($properties as $name => $value) {
            // A name the class does not declare is a Shirabe bug rather than a plugin one, and
            // ReflectionProperty reports it as such instead of dropping the state silently.
            $property = new \ReflectionProperty($exception, $name);
            $property->setAccessible(true);
            $property->setValue($exception, $value);
        }
        if ($file !== '') {
            self::setBaseProperty($exception, 'file', $file);
            self::setBaseProperty($exception, 'line', $line);
        }

        return $exception;
    }

    private static function instantiate(string $class, string $message, int $code): \Throwable
    {
        if ($class === '' || !class_exists($class) || !is_a($class, \Throwable::class, true)) {
            return new \RuntimeException($message, $code);
        }
        $reflection = new \ReflectionClass($class);
        $constructor = $reflection->getConstructor();
        if ($constructor !== null && self::acceptsMessageAndCode($constructor)) {
            return new $class($message, $code);
        }
        if ($reflection->isAbstract() || ($reflection->isInternal() && $reflection->isFinal())) {
            return new \RuntimeException($message, $code);
        }

        // A constructor of another shape, such as InvalidPackageException's, builds the message
        // from state the frame carries separately, so the instance is filled in field by field.
        $exception = $reflection->newInstanceWithoutConstructor();
        self::setBaseProperty($exception, 'message', $message);
        self::setBaseProperty($exception, 'code', $code);

        return $exception;
    }

    /**
     * Writes one of the properties \Exception and \Error declare for every throwable.
     *
     * @param mixed $value
     */
    private static function setBaseProperty(\Throwable $exception, string $name, $value): void
    {
        $property = new \ReflectionProperty($exception instanceof \Exception ? \Exception::class : \Error::class, $name);
        $property->setAccessible(true);
        $property->setValue($exception, $value);
    }

    /**
     * Whether the constructor has \Exception's shape as far as the frame fills it in: a string
     * message, an int code, and nothing else required.
     */
    private static function acceptsMessageAndCode(\ReflectionMethod $constructor): bool
    {
        $parameters = $constructor->getParameters();
        if (count($parameters) < 2) {
            return false;
        }
        foreach ($parameters as $position => $parameter) {
            $type = $parameter->getType();
            $name = $type instanceof \ReflectionNamedType ? $type->getName() : null;
            if ($position === 0 && $name !== 'string') {
                return false;
            }
            if ($position === 1 && $name !== 'int') {
                return false;
            }
            if ($position >= 2 && !$parameter->isOptional()) {
                return false;
            }
        }

        return true;
    }
}
