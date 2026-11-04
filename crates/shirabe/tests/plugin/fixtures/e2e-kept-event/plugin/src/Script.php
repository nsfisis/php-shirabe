<?php

namespace ShirabeTest\KeptEvent;

use Composer\Script\Event;

/**
 * A `scripts` handler that keeps the event it is called with, for the plugin to use later.
 */
class Script
{
    /** @var Event|null */
    public static $kept;

    public static function keep(Event $event): void
    {
        self::$kept = $event;
        Plugin::trace('script keep ' . $event->getName() . ' identical to the plugin-kept one=' . json_encode($event === Plugin::$scriptEvent));
    }
}
