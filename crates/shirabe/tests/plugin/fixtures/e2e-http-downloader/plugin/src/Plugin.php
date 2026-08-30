<?php

namespace ShirabeTest\HttpDownloader;

use Composer\Composer;
use Composer\Downloader\TransportException;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Composer\Util\HttpDownloader;

/**
 * Drives an HttpDownloader the plugin constructs itself and appends what every call reports to
 * http-downloader-trace.txt, so the surface can be compared line by line between implementations:
 * the identity of the object a plugin gets, the options it merges, and the static helpers.
 * Nothing here reaches the network — the trace has to be reproducible offline.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    /** @var IOInterface */
    private $io;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->io = $io;
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
        $downloader = new HttpDownloader($this->io, $event->getComposer()->getConfig());
        $lines = ['event=' . $event->getName()];

        $lines[] = 'class=' . json_encode(\get_class($downloader))
            . ' instanceof=' . json_encode($downloader instanceof HttpDownloader);

        // Only the key the plugin set is compared: the rest of the map is the TLS defaults, whose
        // CA paths depend on the machine rather than on the implementation.
        $downloader->setOptions(['http' => ['header' => ['X-Probe: 1']]]);
        $options = $downloader->getOptions();
        $lines[] = 'options header=' . json_encode($options['http']['header'] ?? null);
        $downloader->setOptions(['http' => ['header' => ['X-Probe: 2']]]);
        $lines[] = 'options merged=' . json_encode($downloader->getOptions()['http']['header'] ?? null);

        $lines[] = 'isCurlEnabled=' . json_encode(HttpDownloader::isCurlEnabled());

        // getExceptionHints() only inspects the exception it is handed; both arguments below stay
        // clear of the branch that probes connectivity.
        $lines[] = 'hints other=' . json_encode(HttpDownloader::getExceptionHints(new \RuntimeException('x')))
            . ' transport=' . json_encode(HttpDownloader::getExceptionHints(new TransportException('plain', 400)));

        // The version constraint cannot match, so this reaches Composer::getVersion() and the
        // version parser without writing anything to the terminal.
        $lines[] = 'outputWarnings=' . $this->describe(static function () use ($event): void {
            HttpDownloader::outputWarnings(
                $event->getIO(),
                'https://example.org/packages.json',
                ['warning' => 'unreachable', 'warning-versions' => '^0.0.1']
            );
        });

        file_put_contents('http-downloader-trace.txt', implode("\n", $lines) . "\n");
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
