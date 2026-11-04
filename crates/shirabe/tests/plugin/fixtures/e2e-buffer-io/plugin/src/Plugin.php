<?php

namespace ShirabeTest\BufferIO;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\BufferIO;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Composer\Util\ProcessExecutor;
use Symfony\Component\Console\Formatter\OutputFormatter;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * Captures output into BufferIO instances the plugin constructs itself and appends what each one
 * reports to buffer-io-trace.txt, so the captured text can be compared line by line between
 * implementations: the constructor arguments, the verbosity filter, the formatter tags, the
 * backspace handling of getOutput(), the user inputs, and a BufferIO handed to another Composer
 * utility as its IOInterface.
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
        $lines = ['event=' . $event->getName()];

        $io = new BufferIO();
        $io->write('plain');
        $io->write(['first', 'second']);
        $io->write('no newline', false);
        $io->writeError(' <info>tagged</info>');
        $io->write('verbose only', true, IOInterface::VERBOSE);
        $io->write("10%\x08\x08\x0820%");
        $lines[] = 'default output=' . json_encode($io->getOutput())
            . ' ' . self::flags($io);

        $verbose = new BufferIO('', OutputInterface::VERBOSITY_VERY_VERBOSE);
        $verbose->write('very verbose', true, IOInterface::VERY_VERBOSE);
        $verbose->write('debug only', true, IOInterface::DEBUG);
        $lines[] = 'very-verbose output=' . json_encode($verbose->getOutput())
            . ' ' . self::flags($verbose);

        $verbose->setUserInputs(['yes', 'no']);
        $lines[] = 'user-inputs ' . self::flags($verbose);

        // A BufferIO is the IOInterface a plugin hands to the utilities it builds when it wants
        // their output, so the debug line ProcessExecutor writes has to land in this buffer.
        $debug = new BufferIO('input', OutputInterface::VERBOSITY_DEBUG);
        $process = new ProcessExecutor($debug);
        $code = $process->execute('echo captured', $captured);
        $lines[] = 'process code=' . $code . ' output=' . json_encode($captured)
            . ' io=' . json_encode($debug->getOutput()) . ' ' . self::flags($debug);

        file_put_contents('buffer-io-trace.txt', implode("\n", $lines) . "\n");

        try {
            new BufferIO('', OutputInterface::VERBOSITY_NORMAL, new OutputFormatter(true));
            $formatter = 'constructed';
        } catch (\Exception $e) {
            $formatter = 'caught=' . get_class($e) . ' message=' . json_encode($e->getMessage());
        }
        file_put_contents('buffer-io-formatter.txt', $formatter . "\n");
    }

    private static function flags(IOInterface $io): string
    {
        return 'verbose=' . json_encode($io->isVerbose())
            . ' veryVerbose=' . json_encode($io->isVeryVerbose())
            . ' debug=' . json_encode($io->isDebug())
            . ' interactive=' . json_encode($io->isInteractive())
            . ' decorated=' . json_encode($io->isDecorated());
    }
}
