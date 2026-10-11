<?php declare(strict_types=1);

// Shirabe's own definition of Composer\Util\Loop for the plugin worker: a port of the real
// class, running here on top of the HttpDownloader and ProcessExecutor proxy stubs. The Rust side
// keeps a Loop of its own for the execution itself, and the two are siblings over one job
// registry (docs/dev/plugin-async.md). The one addition to the original is the forward in
// abortJobs(), which only the Loop serving Composer::getLoop() makes.

namespace Composer\Util;

use Symfony\Component\Console\Helper\ProgressBar;
use React\Promise\PromiseInterface;

/**
 * @author Jordi Boggiano <j.boggiano@seld.be>
 */
class Loop
{
    /** @var HttpDownloader */
    private $httpDownloader;
    /** @var ProcessExecutor|null */
    private $processExecutor;
    /** @var array<int, array<PromiseInterface<mixed>>> */
    private $currentPromises = [];
    /** @var int */
    private $waitIndex = 0;
    /** @var bool Whether this is the Loop of a Composer entity, as opposed to one a plugin built */
    private $__shirabeBoundToExecution = false;

    public function __construct(HttpDownloader $httpDownloader, ?ProcessExecutor $processExecutor = null)
    {
        $this->httpDownloader = $httpDownloader;
        $this->httpDownloader->enableAsync();

        $this->processExecutor = $processExecutor;
        if ($this->processExecutor) {
            $this->processExecutor->enableAsync();
        }
    }

    /**
     * Marks this Loop as the worker-side sibling of the Loop the Rust-side execution waits on.
     * Under Composer the two are one object sharing one $currentPromises table.
     */
    public function __shirabeBindToExecution(): void
    {
        $this->__shirabeBoundToExecution = true;
    }

    public function getHttpDownloader(): HttpDownloader
    {
        return $this->httpDownloader;
    }

    public function getProcessExecutor(): ?ProcessExecutor
    {
        return $this->processExecutor;
    }

    /**
     * @param array<PromiseInterface<mixed>> $promises
     */
    public function wait(array $promises, ?ProgressBar $progress = null): void
    {
        $uncaught = null;

        \React\Promise\all($promises)->then(
            static function (): void {
            },
            static function (\Throwable $e) use (&$uncaught): void {
                $uncaught = $e;
            }
        );

        // keep track of every group of promises that is waited on, so abortJobs can
        // cancel them all, even if wait() was called within a wait()
        $waitIndex = $this->waitIndex++;
        $this->currentPromises[$waitIndex] = $promises;

        if ($progress) {
            $totalJobs = 0;
            $totalJobs += $this->httpDownloader->countActiveJobs();
            if ($this->processExecutor) {
                $totalJobs += $this->processExecutor->countActiveJobs();
            }
            $progress->start($totalJobs);
        }

        $lastUpdate = 0;
        while (true) {
            $activeJobs = 0;

            $activeJobs += $this->httpDownloader->countActiveJobs();
            if ($this->processExecutor) {
                $activeJobs += $this->processExecutor->countActiveJobs();
            }

            if ($progress && microtime(true) - $lastUpdate > 0.1) {
                $lastUpdate = microtime(true);
                $progress->setProgress($progress->getMaxSteps() - $activeJobs);
            }

            if (!$activeJobs) {
                break;
            }
        }

        // as we skip progress updates if they are too quick, make sure we do one last one here at 100%
        if ($progress) {
            $progress->finish();
        }

        unset($this->currentPromises[$waitIndex]);
        if (null !== $uncaught) {
            throw $uncaught;
        }
    }

    public function abortJobs(): void
    {
        foreach ($this->currentPromises as $promiseGroup) {
            foreach ($promiseGroup as $promise) {
                // to support react/promise 2.x we wrap the promise in a resolve() call for safety
                \React\Promise\resolve($promise)->cancel();
            }
        }

        if ($this->__shirabeBoundToExecution) {
            \ShirabeRpcRuntime::callRust(0, '__shirabeAbortJobs', []);
        }
    }
}
