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

        // A file:// url keeps the probe offline and off the curl path, so the trace is the same
        // on a machine with no network. Paths stay out of the trace: the two runs work in
        // different temporary directories.
        $payload = getcwd() . '/probe-payload.json';
        file_put_contents($payload, '{"probe":true,"n":42}');

        $response = null;
        $lines[] = 'get=' . $this->describe(static function () use ($downloader, $payload, &$response): void {
            $response = $downloader->get('file://' . $payload);
        });
        $lines[] = 'response class=' . json_encode($response === null ? null : \get_class($response))
            . ' body=' . json_encode($response === null ? null : $response->getBody())
            . ' headers=' . json_encode($response === null ? null : $response->getHeaders());
        $lines[] = 'getHeader=' . json_encode($response === null ? null : $response->getHeader('Content-Type'));

        $target = getcwd() . '/probe-copy.json';
        $lines[] = 'copy=' . $this->describe(static function () use ($downloader, $payload, $target): void {
            $downloader->copy('file://' . $payload, $target);
        }) . ' file=' . json_encode(@file_get_contents($target));

        // A downloader that is not part of a Loop refuses async requests, so the probe records
        // both sides of the gate.
        $lines[] = 'add-before-enable=' . $this->describe(static function () use ($downloader, $payload): void {
            $downloader->add('file://' . $payload);
        });
        $downloader->enableAsync();

        $added = null;
        $lines[] = 'add=' . $this->describe(static function () use ($downloader, $payload, &$added): void {
            $downloader->add('file://' . $payload)->then(static function ($result) use (&$added): void {
                $added = $result;
            });
        }) . ' body=' . json_encode($added === null ? null : $added->getBody());

        // A path that cannot exist keeps the rejection reason free of the temporary directory.
        $rejection = null;
        $lines[] = 'add-missing=' . $this->describe(static function () use ($downloader, &$rejection): void {
            $downloader->add('file:///shirabe-probe-missing.json')->then(null, static function ($error) use (&$rejection): void {
                $rejection = $error;
            });
        }) . ' rejected=' . json_encode($rejection === null ? null : \get_class($rejection));

        $lines[] = 'addCopy=' . $this->describe(static function () use ($downloader, $payload): void {
            $downloader->addCopy('file://' . $payload, getcwd() . '/probe-async-copy.json')->then(null, static function ($error): void {
                throw $error;
            });
        }) . ' file=' . json_encode(@file_get_contents(getcwd() . '/probe-async-copy.json'));

        $lines[] = 'countActiveJobs=' . json_encode($downloader->countActiveJobs())
            . ' wait=' . $this->describe(static function () use ($downloader): void {
                $downloader->wait();
            });

        // collect() unsets the response's own properties, so the object is spent afterwards and
        // nothing may read it again.
        $lines[] = 'collect=' . $this->describe(static function () use ($response): void {
            $response->collect();
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
