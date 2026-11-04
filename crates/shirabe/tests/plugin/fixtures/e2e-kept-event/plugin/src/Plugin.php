<?php

namespace ShirabeTest\KeptEvent;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\Installer\PackageEvent;
use Composer\Installer\PackageEvents;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

/**
 * Keeps the event objects its listeners receive and uses them after the listener has returned:
 * a later listener of the same dispatch stops propagation through the object an earlier one
 * stored, and the post-command listener reads and changes every event kept from earlier
 * dispatches, including the one a `scripts` handler stored. Each step appends a line to
 * kept-event-trace.txt.
 */
class Plugin implements PluginInterface, EventSubscriberInterface
{
    /** @var Event|null */
    public static $scriptEvent;

    /** @var PackageEvent[] */
    private $packageEvents = [];

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
        // Whether an install resolves or replays a lock file decides which of the two command
        // events fires, so both are subscribed.
        return [
            PackageEvents::POST_PACKAGE_INSTALL => [
                ['keepPackageEvent', 10],
                ['stopThroughKeptEvent', 5],
                ['afterStop', 0],
            ],
            ScriptEvents::POST_AUTOLOAD_DUMP => 'keepScriptEvent',
            ScriptEvents::POST_INSTALL_CMD => 'useKeptEvents',
            ScriptEvents::POST_UPDATE_CMD => 'useKeptEvents',
        ];
    }

    public function keepPackageEvent(PackageEvent $event): void
    {
        $this->packageEvents[] = $event;
        self::trace('keep ' . $event->getName() . ' ' . self::package($event));
    }

    public function stopThroughKeptEvent(PackageEvent $event): void
    {
        $kept = end($this->packageEvents);
        self::trace('second ' . self::package($event) . ' identical=' . json_encode($kept === $event));
        if (self::package($event) === 'shirabe-test/lib-a') {
            $kept->stopPropagation();
            self::trace('stopped through the kept event: ' . json_encode($event->isPropagationStopped()));
        }
    }

    public function afterStop(PackageEvent $event): void
    {
        self::trace('third ' . self::package($event));
    }

    public function keepScriptEvent(Event $event): void
    {
        self::$scriptEvent = $event;
        self::trace('keep ' . $event->getName());
    }

    public function useKeptEvents(Event $event): void
    {
        foreach ($this->packageEvents as $index => $kept) {
            self::trace(implode(' ', [
                'kept[' . $index . ']',
                'name=' . $kept->getName(),
                'package=' . self::package($kept),
                'stopped=' . json_encode($kept->isPropagationStopped()),
                'devMode=' . json_encode($kept->isDevMode()),
                'operations=' . count($kept->getOperations()),
                'root=' . $kept->getComposer()->getPackage()->getName(),
            ]));
        }
        $first = $this->packageEvents[0];
        $first->stopPropagation();
        self::trace('kept[0] stopped after stopPropagation=' . json_encode($first->isPropagationStopped()));
        self::trace('kept[0] identical to kept[1]=' . json_encode($first === $this->packageEvents[1]));

        $script = Script::$kept;
        self::trace(implode(' ', [
            'script kept',
            'name=' . $script->getName(),
            'flags=' . json_encode($script->getFlags()),
            'arguments=' . json_encode($script->getArguments()),
            'devMode=' . json_encode($script->isDevMode()),
            'stopped=' . json_encode($script->isPropagationStopped()),
            'identical to the plugin-kept one=' . json_encode($script === self::$scriptEvent),
            'identical to the current one=' . json_encode($script === $event),
            'same io=' . json_encode($script->getIO() === $event->getIO()),
        ]));
    }

    private static function package(PackageEvent $event): string
    {
        return $event->getOperation()->getPackage()->getPrettyName();
    }

    public static function trace(string $line): void
    {
        file_put_contents('kept-event-trace.txt', $line . "\n", FILE_APPEND);
    }
}
