<?php

namespace ShirabeTest\FileDownload;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Package\PackageInterface;
use Composer\Plugin\PluginEvents;
use Composer\Plugin\PluginInterface;
use Composer\Plugin\PostFileDownloadEvent;
use Composer\Plugin\PreFileDownloadEvent;
use Composer\Util\HttpDownloader;

/**
 * Appends one line per file download event to file-download-trace.txt, so both the order the
 * events arrive in and everything the event exposes are comparable between implementations.
 *
 * The dist URL the repository serves does not resolve; the pre-event rewrites it to the one that
 * does, so the install only succeeds when the rewritten URL is the one downloaded.
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
        return [
            PluginEvents::PRE_FILE_DOWNLOAD => 'onPreFileDownload',
            PluginEvents::POST_FILE_DOWNLOAD => 'onPostFileDownload',
        ];
    }

    public function onPreFileDownload(PreFileDownloadEvent $event): void
    {
        $line = [
            $event->getName(),
            'type=' . $event->getType(),
            'url=' . $event->getProcessedUrl(),
            'cacheKey=' . json_encode($event->getCustomCacheKey()),
            'transportOptions=' . json_encode($event->getTransportOptions()),
            'httpDownloader=' . ($event->getHttpDownloader() instanceof HttpDownloader ? '1' : '0'),
        ];
        if ($event->getType() === 'package') {
            $line[] = 'context=' . $this->package($event->getContext());
            $event->setProcessedUrl(str_replace('/missing.zip', '/remote-lib.zip', $event->getProcessedUrl()));
            $event->setCustomCacheKey('custom-key');
            $line[] = 'rewritten=' . $event->getProcessedUrl();
            $line[] = 'customCacheKey=' . $event->getCustomCacheKey();
        } else {
            $event->setTransportOptions(['http' => ['header' => ['X-Shirabe-Probe: 1']]]);
            $line[] = 'newTransportOptions=' . json_encode($event->getTransportOptions());
        }
        $this->trace(implode(' ', $line));
    }

    public function onPostFileDownload(PostFileDownloadEvent $event): void
    {
        $line = [
            $event->getName(),
            'type=' . $event->getType(),
            'url=' . $event->getUrl(),
            'fileName=' . json_encode($event->getFileName() === null ? null : basename(dirname($event->getFileName())) . '/' . pathinfo($event->getFileName(), PATHINFO_EXTENSION)),
            'checksum=' . json_encode($event->getChecksum()),
        ];
        if ($event->getType() === 'package') {
            $line[] = 'context=' . $this->package($event->getContext());
        }
        $this->trace(implode(' ', $line));
    }

    private function package($context): string
    {
        return $context instanceof PackageInterface
            ? $context->getPrettyName() . '@' . $context->getPrettyVersion()
            : get_debug_type($context);
    }

    private function trace(string $line): void
    {
        file_put_contents('file-download-trace.txt', $line . "\n", FILE_APPEND);
    }
}
