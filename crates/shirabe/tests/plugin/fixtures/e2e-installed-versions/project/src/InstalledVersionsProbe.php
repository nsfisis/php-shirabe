<?php

namespace Shirabe\E2e;

use Composer\InstalledVersions;
use Composer\Script\Event;

class InstalledVersionsProbe
{
    public static function probe(Event $event): void
    {
        $lines = [
            'installed=' . (InstalledVersions::isInstalled('shirabe/seeded') ? '1' : '0'),
            'pretty-version=' . InstalledVersions::getPrettyVersion('shirabe/seeded'),
            'install-path=' . substr(InstalledVersions::getInstallPath('shirabe/seeded'), strlen(dirname(__DIR__))),
        ];
        file_put_contents(__DIR__ . '/../installed-versions-trace.txt', implode("\n", $lines) . "\n");
    }
}
