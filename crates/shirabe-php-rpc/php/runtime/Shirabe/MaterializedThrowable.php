<?php

// The PHP half of the Throw frame's exception codec. A failure crosses with the class it was
// thrown as, so `catch (TransportException $e)` on either side catches what it would catch under
// Composer, plus the state that class declares beyond message and code and the place it was
// thrown at.

namespace Shirabe;

final class MaterializedThrowable
{
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
     * The fields of a Throw frame for an exception raised in this process: class, message,
     * code, declared state, file, line and the superclasses, nearest first.
     *
     * @return array{string, string, int, array<string, mixed>, string, int, list<string>}
     */
    public static function describe(\Throwable $e): array
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
     * Rebuilds the exception a Throw frame describes. A class the child cannot construct keeps
     * the RuntimeException shape, which is all the frame guarantees.
     *
     * @param array<string, mixed> $properties
     */
    public static function revive(string $class, string $message, int $code, array $properties, string $file = '', int $line = 0): \Throwable
    {
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
