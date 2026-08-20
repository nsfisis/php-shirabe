<?php

namespace Shirabe\E2e;

use Composer\Script\Event;

class ScriptEventRecorder
{
    public static function postAutoloadDump(Event $event): void
    {
        $composer = $event->getComposer();
        $lines = [
            'event=' . $event->getName(),
            'dev=' . ($event->isDevMode() ? '1' : '0'),
            'root=' . $composer->getPackage()->getName(),
            'vendor-dir=' . basename($composer->getConfig()->get('vendor-dir')),
            'bin-dir=' . basename($composer->getConfig()->get('bin-dir')),
        ];
        $event->getIO()->write('script-event: ' . implode(' ', $lines));
        file_put_contents(__DIR__ . '/../script-event-trace.txt', implode("\n", $lines) . "\n");
    }
}
