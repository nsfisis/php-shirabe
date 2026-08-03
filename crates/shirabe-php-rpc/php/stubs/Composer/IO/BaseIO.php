<?php

// Hand-written proxy stub for Composer\IO\BaseIO, kept in the shape the future stub generator
// will output: every public method of the real class (its own and the IOInterface /
// LoggerInterface surface) forwards to the Rust-side entity. Loading this stub pulls the real
// IOInterface contract from the Composer PHP runtime, which must already be required.

namespace Composer\IO;

use Composer\Config;

abstract class BaseIO implements IOInterface, \ShirabeRustStub
{
    /** @var int */
    protected $__rhandle;
    /** @var int */
    protected $__epoch;

    public function __construct(int $rhandle = 0, int $epoch = 0)
    {
        if (func_num_args() < 2) {
            // Constructing the class from plugin code (a common idiom for e.g. `new BufferIO()`)
            // is an open question of the plugin design; only proxy instantiation passes a
            // Rust handle. Fail with a diagnosable message instead of an ArgumentCountError.
            throw new \RuntimeException(
                'Shirabe does not support constructing ' . static::class . ' inside the plugin process yet'
            );
        }
        $this->__rhandle = $rhandle;
        $this->__epoch = $epoch;
    }

    public function __destruct()
    {
        \ShirabeRustObjectRegistry::release($this->__rhandle);
    }

    public function __shirabeRustHandleDescriptor(): array
    {
        return [
            '__rhandle' => $this->__rhandle,
            '__class' => static::class,
            '__epoch' => $this->__epoch,
        ];
    }

    public function isInteractive()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isInteractive', []);
    }

    public function isVerbose()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isVerbose', []);
    }

    public function isVeryVerbose()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isVeryVerbose', []);
    }

    public function isDebug()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isDebug', []);
    }

    public function isDecorated()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'isDecorated', []);
    }

    public function write($messages, bool $newline = true, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'write', [$messages, $newline, $verbosity]);
    }

    public function writeError($messages, bool $newline = true, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'writeError', [$messages, $newline, $verbosity]);
    }

    public function writeRaw($messages, bool $newline = true, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'writeRaw', [$messages, $newline, $verbosity]);
    }

    public function writeErrorRaw($messages, bool $newline = true, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'writeErrorRaw', [$messages, $newline, $verbosity]);
    }

    public function overwrite($messages, bool $newline = true, ?int $size = null, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'overwrite', [$messages, $newline, $size, $verbosity]);
    }

    public function overwriteError($messages, bool $newline = true, ?int $size = null, int $verbosity = self::NORMAL)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'overwriteError', [$messages, $newline, $size, $verbosity]);
    }

    public function ask(string $question, $default = null)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'ask', [$question, $default]);
    }

    public function askConfirmation(string $question, bool $default = true)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'askConfirmation', [$question, $default]);
    }

    public function askAndValidate(string $question, callable $validator, ?int $attempts = null, $default = null)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'askAndValidate', [$question, $validator, $attempts, $default]);
    }

    public function askAndHideAnswer(string $question)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'askAndHideAnswer', [$question]);
    }

    public function select(string $question, array $choices, $default, $attempts = false, string $errorMessage = 'Value "%s" is invalid', bool $multiselect = false)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'select', [$question, $choices, $default, $attempts, $errorMessage, $multiselect]);
    }

    public function getAuthentications()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getAuthentications', []);
    }

    public function resetAuthentications()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'resetAuthentications', []);
    }

    public function hasAuthentication(string $repositoryName)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'hasAuthentication', [$repositoryName]);
    }

    public function getAuthentication(string $repositoryName)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getAuthentication', [$repositoryName]);
    }

    public function setAuthentication(string $repositoryName, string $username, ?string $password = null)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'setAuthentication', [$repositoryName, $username, $password]);
    }

    public function loadConfiguration(Config $config)
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'loadConfiguration', [$config]);
    }

    public function emergency($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'emergency', [$message, $context]);
    }

    public function alert($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'alert', [$message, $context]);
    }

    public function critical($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'critical', [$message, $context]);
    }

    public function error($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'error', [$message, $context]);
    }

    public function warning($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'warning', [$message, $context]);
    }

    public function notice($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'notice', [$message, $context]);
    }

    public function info($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'info', [$message, $context]);
    }

    public function debug($message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'debug', [$message, $context]);
    }

    public function log($level, $message, array $context = []): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'log', [$level, $message, $context]);
    }
}
