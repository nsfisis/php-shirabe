<?php

namespace ShirabeTest\GlobalHelper;

use Composer\Script\Event;

/**
 * Called by the project's post-autoload-dump script. Only the global plugin's class loader can
 * resolve it: the project does not require this package.
 */
class ScriptHandler
{
    public static function run(Event $event): void
    {
        Helper::trace('script ' . $event->getName());
    }
}
