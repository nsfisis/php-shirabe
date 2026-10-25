<?php

namespace ShirabeTest\Exception;

use Composer\Composer;
use Composer\Downloader\TransportException;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Composer\Util\Filesystem;
use Composer\Util\ProcessExecutor;

/**
 * Records what a plugin sees when a Composer service raises an exception at it: the class it was
 * thrown as, its message and code, whether it is still an instance of the parent classes the real
 * hierarchy gives it, and whether a `catch` naming that class matches. Composer plugins branch on
 * the exception class rather than on its message, so the whole surface is compared line by line
 * between implementations.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
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
        $filesystem = new Filesystem();
        $lines = ['event=' . $event->getName()];

        $lines[] = 'findShortestPath ' . $this->describe(static function () use ($filesystem): void {
            $filesystem->findShortestPath('relative', '/absolute');
        });
        $lines[] = 'findShortestPathCode ' . $this->describe(static function () use ($filesystem): void {
            $filesystem->findShortestPathCode('/absolute', 'relative');
        });

        // A different class through the same seam, so the trace shows the class travelling rather
        // than every failure arriving under one name.
        file_put_contents('not-a-directory', '');
        $lines[] = 'ensureDirectoryExists ' . $this->describe(static function () use ($filesystem): void {
            $filesystem->ensureDirectoryExists('not-a-directory');
        });

        // get_class() answers for the object; a catch clause answers for the class hierarchy the
        // child holds, which is what plugin code is actually written against.
        try {
            $filesystem->findShortestPath('relative', '/absolute');
            $caught = 'nothing-thrown';
        } catch (\InvalidArgumentException $e) {
            $caught = 'InvalidArgumentException';
        } catch (\Throwable $e) {
            $caught = 'unmatched:' . \get_class($e);
        }
        $lines[] = 'catch-clause=' . $caught;

        // A throw from plugin code unwinds through the service that called it and back out to the
        // plugin, keeping its own class, the parents of a class only the plugin declares, and the
        // state of a Composer class.
        $executor = new ProcessExecutor($event->getIO());
        $lines[] = 'callback-own-class ' . $this->describe(static function () use ($executor): void {
            $callback = static function (): void {
                throw new PluginException('from the plugin', 7);
            };
            $executor->execute('echo out', $callback);
        });
        $lines[] = 'callback-transport ' . $this->describe(static function () use ($executor): void {
            $callback = static function (): void {
                $e = new TransportException('not found', 404);
                $e->setStatusCode(404);
                $e->setResponse('missing');
                $e->setHeaders(['HTTP/1.1 404 Not Found']);
                throw $e;
            };
            $executor->execute('echo out', $callback);
        });

        file_put_contents('exception-trace.txt', implode("\n", $lines) . "\n");
    }

    private function describe(callable $call): string
    {
        try {
            $call();

            return 'class=none';
        } catch (\Throwable $e) {
            return 'class=' . json_encode(\get_class($e))
                . ' message=' . json_encode($e->getMessage())
                . ' code=' . json_encode($e->getCode())
                . ' logic=' . json_encode($e instanceof \LogicException)
                . ' runtime=' . json_encode($e instanceof \RuntimeException)
                . ($e instanceof TransportException
                    ? ' status=' . json_encode($e->getStatusCode())
                        . ' response=' . json_encode($e->getResponse())
                        . ' headers=' . json_encode($e->getHeaders())
                    : '');
        }
    }
}
