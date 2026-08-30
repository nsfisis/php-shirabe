<?php

// The PHP half of the Throw frame's exception codec. A Rust-side failure carries the class it
// was thrown as, so `catch (TransportException $e)` in plugin code catches what it would catch
// under Composer, plus the state that class declares beyond message and code.

namespace Shirabe;

final class MaterializedThrowable
{
    /**
     * Rebuilds the exception a Throw frame describes. A class the child cannot construct from a
     * message and a code keeps the RuntimeException shape, which is all the frame guarantees.
     *
     * @param array<string, mixed> $properties
     */
    public static function revive(string $class, string $message, int $code, array $properties): \Throwable
    {
        $exception = self::instantiate($class, $message, $code);
        foreach ($properties as $name => $value) {
            // A name the class does not declare is a Shirabe bug rather than a plugin one, and
            // ReflectionProperty reports it as such instead of dropping the state silently.
            $property = new \ReflectionProperty($exception, $name);
            $property->setAccessible(true);
            $property->setValue($exception, $value);
        }

        return $exception;
    }

    private static function instantiate(string $class, string $message, int $code): \Throwable
    {
        if ($class === '' || !class_exists($class) || !is_a($class, \Throwable::class, true)) {
            return new \RuntimeException($message, $code);
        }
        $constructor = (new \ReflectionClass($class))->getConstructor();
        if ($constructor === null || !self::acceptsMessageAndCode($constructor)) {
            return new \RuntimeException($message, $code);
        }

        return new $class($message, $code);
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
