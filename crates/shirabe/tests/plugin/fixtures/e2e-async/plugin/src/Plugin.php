<?php

namespace ShirabeTest\Async;

use Composer\Composer;
use Composer\Downloader\TransportException;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Composer\Util\Filesystem;
use Composer\Util\HttpDownloader;
use Composer\Util\Loop;
use Composer\Util\ProcessExecutor;
use Composer\Util\SyncHelper;
use Symfony\Component\Process\Process;

/**
 * Drives the asynchronous surface and appends what it observes to async-trace.txt, so the points
 * at which a promise is pending, settles and runs its callbacks can be compared line by line
 * between implementations. Every child sleeps long enough that "still running right after it was
 * started" is the same answer on any machine, and nothing here reaches the network.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    /** @var IOInterface */
    private $io;
    /** @var AsyncInstaller */
    private $installer;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->io = $io;
        $this->installer = new AsyncInstaller($composer);
        $composer->getInstallationManager()->addInstaller($this->installer);
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
        $composer->getInstallationManager()->removeInstaller($this->installer);
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }

    public static function getSubscribedEvents()
    {
        // Whether an install resolves or replays a lock file decides which of the two fires, so
        // both are subscribed and the trace records the one that ran.
        return [
            ScriptEvents::POST_INSTALL_CMD => 'onPostCommand',
            ScriptEvents::POST_UPDATE_CMD => 'onPostCommand',
        ];
    }

    public function onPostCommand(Event $event): void
    {
        $lines = ['event=' . $event->getName()];
        $executor = new ProcessExecutor($this->io);

        $lines[] = 'executeAsync-before-enable=' . $this->describe(static function () use ($executor): void {
            $executor->executeAsync('echo never');
        });

        // A Loop enables the async surface of the services it is built over.
        $downloader = new HttpDownloader($this->io, $event->getComposer()->getConfig());
        $loop = new Loop($downloader, $executor);

        // The child is started inside executeAsync(), but its promise settles only inside a tick.
        $process = null;
        $promise = $executor->executeAsync('sleep 0.3; echo out; echo err 1>&2')
            ->then(static function ($result) use (&$process): void {
                $process = $result;
            });
        $lines[] = 'started settled=' . json_encode($process !== null)
            . ' countActiveJobs=' . $executor->countActiveJobs();
        $executor->wait();
        $lines[] = 'waited settled=' . json_encode($process !== null)
            . ' countActiveJobs=' . $executor->countActiveJobs();
        $lines[] = 'process instanceof=' . json_encode($process instanceof Process)
            . ' successful=' . json_encode($process->isSuccessful())
            . ' exitCode=' . json_encode($process->getExitCode())
            . ' exitCodeText=' . json_encode($process->getExitCodeText())
            . ' running=' . json_encode($process->isRunning())
            . ' terminated=' . json_encode($process->isTerminated())
            . ' status=' . json_encode($process->getStatus())
            . ' pid=' . json_encode($process->getPid());
        $lines[] = 'process output=' . json_encode($process->getOutput())
            . ' error=' . json_encode($process->getErrorOutput())
            . ' incremental=' . json_encode([$process->getIncrementalOutput(), $process->getIncrementalOutput()])
            . ' commandLine=' . json_encode($process->getCommandLine())
            . ' signaled=' . json_encode($process->hasBeenSignaled())
            . ' stopped=' . json_encode($process->hasBeenStopped())
            . ' timeout=' . json_encode($process->getTimeout());
        $process->clearOutput();
        $lines[] = 'process cleared=' . json_encode($process->getOutput())
            . ' stop=' . json_encode($process->stop());

        // A child that exits non-zero resolves its promise; only a failed start rejects it.
        $failed = null;
        $rejected = null;
        $executor->executeAsync('sleep 0.1; exit 3')->then(
            static function ($result) use (&$failed): void {
                $failed = $result;
            },
            static function ($error) use (&$rejected): void {
                $rejected = $error;
            }
        );
        $executor->wait();
        $lines[] = 'failing exitCode=' . json_encode($failed === null ? null : $failed->getExitCode())
            . ' successful=' . json_encode($failed === null ? null : $failed->isSuccessful())
            . ' rejected=' . json_encode($rejected === null ? null : \get_class($rejected));

        // Loop::wait() runs every job to completion and each callback inside the wait.
        $order = [];
        $first = $executor->executeAsync('sleep 0.4; echo slow')->then(static function ($result) use (&$order): void {
            $order[] = trim($result->getOutput());
        });
        $second = $executor->executeAsync(['echo', 'fast'])->then(static function ($result) use (&$order): void {
            $order[] = trim($result->getOutput());
        });
        $lines[] = 'loop before=' . json_encode($order);
        $loop->wait([$first, $second]);
        $lines[] = 'loop after=' . json_encode($order) . ' countActiveJobs=' . $executor->countActiveJobs();

        // A callback may start the next job and wait for it from inside the wait that ran it.
        $nested = [];
        $outer = $executor->executeAsync('sleep 0.1; echo outer')->then(
            static function ($result) use ($executor, $loop, &$nested): void {
                $nested[] = trim($result->getOutput());
                $loop->wait([
                    $executor->executeAsync('sleep 0.1; echo inner')->then(static function ($result) use (&$nested): void {
                        $nested[] = trim($result->getOutput());
                    }),
                ]);
                $nested[] = 'inner waited';
            }
        );
        $loop->wait([$outer]);
        $lines[] = 'nested=' . json_encode($nested);

        // Cancelling a started job stops the child and rejects the promise from the canceller.
        $cancelled = null;
        $running = $executor->executeAsync('sleep 30');
        $running->then(null, static function ($error) use (&$cancelled): void {
            $cancelled = $error;
        });
        $running->cancel();
        $lines[] = 'cancel rejected=' . json_encode($cancelled === null ? null : \get_class($cancelled) . ': ' . $cancelled->getMessage())
            . ' countActiveJobs=' . $executor->countActiveJobs();

        // A job the budget has not let through yet is dropped without a rejection.
        $executor->setMaxJobs(1);
        $ran = [];
        $queuedRejection = null;
        $occupying = $executor->executeAsync('sleep 0.2; echo occupying')->then(static function ($result) use (&$ran): void {
            $ran[] = trim($result->getOutput());
        });
        $queued = $executor->executeAsync('echo queued');
        $queued->then(
            static function ($result) use (&$ran): void {
                $ran[] = trim($result->getOutput());
            },
            static function ($error) use (&$queuedRejection): void {
                $queuedRejection = $error;
            }
        );
        $queued->cancel();
        $loop->wait([$occupying]);
        $lines[] = 'queued-cancel ran=' . json_encode($ran)
            . ' rejected=' . json_encode($queuedRejection === null ? null : \get_class($queuedRejection));
        $executor->resetMaxJobs();

        // The budget holds the second job back until the first one is done.
        $executor->setMaxJobs(1);
        $budget = [];
        $loop->wait([
            $executor->executeAsync('sleep 0.2; echo one')->then(static function ($result) use (&$budget): void {
                $budget[] = trim($result->getOutput());
            }),
            $executor->executeAsync('echo two')->then(static function ($result) use (&$budget): void {
                $budget[] = trim($result->getOutput());
            }),
        ]);
        $lines[] = 'budget order=' . json_encode($budget);
        $executor->resetMaxJobs();

        // A rejection surfaces from the wait once the whole group is done.
        $lines[] = 'loop rejection=' . $this->describe(static function () use ($loop, $executor): void {
            $loop->wait([
                $executor->executeAsync('sleep 0.1; echo fine')->then(static function (): void {
                    throw new \DomainException('from a callback');
                }),
            ]);
        });

        $awaited = null;
        SyncHelper::await($loop, $executor->executeAsync('sleep 0.1; echo awaited')->then(static function ($result) use (&$awaited): void {
            $awaited = trim($result->getOutput());
        }));
        $lines[] = 'SyncHelper::await=' . json_encode($awaited);

        // removeDirectoryAsync() chains on a job of the executor the Filesystem was built with.
        $filesystem = new Filesystem($executor);
        @mkdir('async-probe-dir/nested', 0777, true);
        file_put_contents('async-probe-dir/nested/file.txt', 'x');
        $removed = null;
        $removal = $filesystem->removeDirectoryAsync(getcwd() . '/async-probe-dir')->then(static function ($result) use (&$removed): void {
            $removed = $result;
        });
        $loop->wait([$removal]);
        $lines[] = 'removeDirectoryAsync result=' . json_encode($removed)
            . ' exists=' . json_encode(is_dir('async-probe-dir'));

        $missing = null;
        $filesystem->removeDirectoryAsync(getcwd() . '/async-probe-missing')->then(static function ($result) use (&$missing): void {
            $missing = $result;
        });
        $lines[] = 'removeDirectoryAsync missing=' . json_encode($missing);

        // The Loop of the run is one object, and it drives the services the run itself uses.
        $shared = $event->getComposer()->getLoop();
        $viaShared = null;
        $shared->wait([
            $shared->getProcessExecutor()->executeAsync('sleep 0.1; echo shared')->then(static function ($result) use (&$viaShared): void {
                $viaShared = trim($result->getOutput());
            }),
        ]);
        $lines[] = 'shared loop same=' . json_encode($shared === $event->getComposer()->getLoop())
            . ' output=' . json_encode($viaShared);

        // The test serves a directory over plain HTTP on the loopback interface, which is what
        // takes a request down the curl path and makes it a job the downloader has to be ticked
        // for. The address stays out of the trace: the port differs from run to run.
        $base = getenv('SHIRABE_E2E_ASYNC_HTTP');
        if (\is_string($base) && $base !== '') {
            $body = null;
            $downloader->add($base . '/slow.php')->then(static function ($response) use (&$body): void {
                $body = $response->getBody();
            });
            $lines[] = 'http started settled=' . json_encode($body !== null);
            $downloader->wait();
            $lines[] = 'http waited body=' . json_encode($body)
                . ' countActiveJobs=' . $downloader->countActiveJobs();

            $responses = [];
            $loop->wait([
                $downloader->add($base . '/payload.json')->then(static function ($response) use (&$responses): void {
                    $responses[] = 'add ' . $response->getStatusCode() . ' ' . $response->getBody();
                }),
                $downloader->addCopy($base . '/payload.json', getcwd() . '/async-http-copy.json')->then(static function ($response) use (&$responses): void {
                    $responses[] = 'addCopy ' . $response->getStatusCode();
                }),
            ]);
            sort($responses);
            $lines[] = 'http loop responses=' . json_encode($responses)
                . ' file=' . json_encode(@file_get_contents('async-http-copy.json'));

            $missing = null;
            try {
                $loop->wait([$downloader->add($base . '/missing.json')]);
            } catch (\Throwable $e) {
                $missing = $e;
            }
            $lines[] = 'http missing=' . json_encode($missing === null ? null : \get_class($missing))
                . ' status=' . json_encode($missing instanceof TransportException ? $missing->getStatusCode() : null);

            // A request the job budget let through counts as started, so cancelling it rejects.
            $cancelled = null;
            $slow = $downloader->add($base . '/slow.php?ms=2000');
            $slow->then(null, static function ($error) use (&$cancelled): void {
                $cancelled = $error;
            });
            $slow->cancel();
            $lines[] = 'http cancel rejected=' . json_encode($cancelled === null ? null : \get_class($cancelled))
                . ' canceled-message=' . json_encode($cancelled !== null && strpos($cancelled->getMessage(), 'Download of ') === 0)
                . ' countActiveJobs=' . $downloader->countActiveJobs();
        }

        file_put_contents('async-trace.txt', implode("\n", $lines) . "\n");
    }

    private function describe(callable $call): string
    {
        try {
            $call();

            return 'ok';
        } catch (\Throwable $e) {
            return \get_class($e) . ': ' . $e->getMessage();
        }
    }
}
