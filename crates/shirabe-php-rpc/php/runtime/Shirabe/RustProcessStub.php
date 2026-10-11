<?php

// The Process an executeAsync() promise of the proxied Composer\Util\ProcessExecutor resolves
// with. The child was spawned by the Rust side, so there is no proc_open() resource in this
// process for the real class to work on; this subclass keeps `instanceof Process` true and
// answers from the facts the Rust-side Process reports instead.
//
// A promise of executeAsync() settles only once the child has exited, so an instance always
// stands for a terminated process. The surface that inspects one is served; the surface that
// drives one is an explicit error. See docs/dev/plugin-async.md.
//
// The signatures are the ones of the symfony/process the Composer runtime bundle ships.

namespace Shirabe;

use Symfony\Component\Process\Process;

class RustProcessStub extends Process implements \ShirabeRustStub
{
    /** @var int */
    protected $__rhandle;
    /** @var int */
    protected $__epoch;
    /** @var ?array<string, mixed> What the Rust side reports about the process, read once */
    private $__shirabeFacts;
    /** @var string */
    private $__shirabeOutput = '';
    /** @var string */
    private $__shirabeErrorOutput = '';
    /** @var int */
    private $__shirabeOutputOffset = 0;
    /** @var int */
    private $__shirabeErrorOutputOffset = 0;

    /**
     * Binds the stub to the Rust-side process. An instance is never constructed: the real
     * constructor would describe a process this side is about to start.
     */
    public function __shirabeBind(int $rhandle, int $epoch): void
    {
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

    public function __clone()
    {
        // The real __clone resets the copy into a process that can be started again, which is
        // the surface this class does not serve.
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    /**
     * A terminated process does not change any more, so one round trip reads everything.
     *
     * @return array<string, mixed>
     */
    private function __shirabeFacts(): array
    {
        if ($this->__shirabeFacts === null) {
            $this->__shirabeFacts = \ShirabeRpcRuntime::callRust($this->__rhandle, '__shirabeFacts', []);
            $this->__shirabeOutput = $this->__shirabeFacts['output'];
            $this->__shirabeErrorOutput = $this->__shirabeFacts['errorOutput'];
        }

        return $this->__shirabeFacts;
    }

    /** @return mixed */
    private function __shirabeFact(string $name)
    {
        return $this->__shirabeFacts()[$name];
    }

    private function __shirabeUnsupported(string $method): \RuntimeException
    {
        return new \RuntimeException(
            "Process::{$method}() is not available on a process the Rust side started: it has "
            . 'already terminated, and nothing in the plugin runtime can drive it.'
        );
    }

    public function run(?callable $callback = null, array $env = []): int
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function mustRun(?callable $callback = null, array $env = []): self
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function start(?callable $callback = null, array $env = [])
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function restart(?callable $callback = null, array $env = []): self
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function wait(?callable $callback = null)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function waitUntil(callable $callback): bool
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function getPid()
    {
        return null;
    }

    public function signal(int $signal)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function disableOutput()
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function enableOutput()
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function isOutputDisabled()
    {
        return false;
    }

    public function getOutput()
    {
        $this->__shirabeFacts();

        return $this->__shirabeOutput;
    }

    public function getIncrementalOutput()
    {
        $this->__shirabeFacts();
        $latest = (string) substr($this->__shirabeOutput, $this->__shirabeOutputOffset);
        $this->__shirabeOutputOffset = \strlen($this->__shirabeOutput);

        return $latest;
    }

    public function getIterator(int $flags = 0)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function clearOutput()
    {
        $this->__shirabeFacts();
        $this->__shirabeOutput = '';
        $this->__shirabeOutputOffset = 0;

        return $this;
    }

    public function getErrorOutput()
    {
        $this->__shirabeFacts();

        return $this->__shirabeErrorOutput;
    }

    public function getIncrementalErrorOutput()
    {
        $this->__shirabeFacts();
        $latest = (string) substr($this->__shirabeErrorOutput, $this->__shirabeErrorOutputOffset);
        $this->__shirabeErrorOutputOffset = \strlen($this->__shirabeErrorOutput);

        return $latest;
    }

    public function clearErrorOutput()
    {
        $this->__shirabeFacts();
        $this->__shirabeErrorOutput = '';
        $this->__shirabeErrorOutputOffset = 0;

        return $this;
    }

    public function getExitCode()
    {
        return $this->__shirabeFact('exitCode');
    }

    public function getExitCodeText()
    {
        return $this->__shirabeFact('exitCodeText');
    }

    public function isSuccessful()
    {
        return 0 === $this->__shirabeFact('exitCode');
    }

    public function hasBeenSignaled()
    {
        return $this->__shirabeFact('signaled');
    }

    public function getTermSignal()
    {
        return $this->__shirabeFact('termSignal');
    }

    public function hasBeenStopped()
    {
        return $this->__shirabeFact('stopped');
    }

    public function getStopSignal()
    {
        return $this->__shirabeFact('stopSignal');
    }

    public function isRunning()
    {
        return false;
    }

    public function isStarted()
    {
        return true;
    }

    public function isTerminated()
    {
        return true;
    }

    public function getStatus()
    {
        return self::STATUS_TERMINATED;
    }

    public function stop(float $timeout = 10, ?int $signal = null)
    {
        return $this->__shirabeFact('exitCode');
    }

    public function addOutput(string $line)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function addErrorOutput(string $line)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function getLastOutputTime(): ?float
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function getCommandLine()
    {
        return $this->__shirabeFact('commandLine');
    }

    public function getTimeout()
    {
        return $this->__shirabeFact('timeout');
    }

    public function getIdleTimeout()
    {
        return null;
    }

    public function setTimeout(?float $timeout)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function setIdleTimeout(?float $timeout)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function setTty(bool $tty)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function isTty()
    {
        return false;
    }

    public function setPty(bool $bool)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function isPty()
    {
        return false;
    }

    public function getWorkingDirectory()
    {
        return $this->__shirabeFact('workingDirectory');
    }

    public function setWorkingDirectory(string $cwd)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function getEnv()
    {
        return $this->__shirabeFact('env');
    }

    public function setEnv(array $env)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function getInput()
    {
        return null;
    }

    public function setInput($input)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }

    public function checkTimeout()
    {
    }

    public function getStartTime(): float
    {
        return $this->__shirabeFact('startTime');
    }

    public function setOptions(array $options)
    {
        throw $this->__shirabeUnsupported(__FUNCTION__);
    }
}
