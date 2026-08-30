<?php

namespace ShirabeTest\ProcessExecutor;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;
use Composer\Util\Filesystem;
use Composer\Util\ProcessExecutor;

/**
 * Drives a ProcessExecutor the plugin constructs itself and appends what every call reports to
 * process-executor-trace.txt, so the whole surface can be compared line by line between
 * implementations: the timeout shared with the rest of the run, both command forms, the three
 * ways the second argument is treated, the error output, and a Filesystem built on the executor.
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
        $process = new ProcessExecutor($this->io);
        $lines = ['event=' . $event->getName()];

        // The timeout is process-wide state Composer seeds from the config, so both worlds have
        // to report the value this project asked for and to observe each other's writes.
        $original = ProcessExecutor::getTimeout();
        $lines[] = 'timeout=' . $original;
        ProcessExecutor::setTimeout(7);
        $lines[] = 'timeout-after-set=' . ProcessExecutor::getTimeout();
        ProcessExecutor::setTimeout($original);

        $code = $process->execute('echo captured', $captured);
        $lines[] = 'capture code=' . $code . ' output=' . json_encode($captured)
            . ' error=' . json_encode($process->getErrorOutput());

        $code = $process->execute(['echo', 'from', 'a', 'list'], $listed);
        $lines[] = 'list code=' . $code . ' output=' . json_encode($listed);

        $code = $process->execute('echo out; echo err 1>&2; exit 3', $failed);
        $lines[] = 'failing code=' . $code . ' output=' . json_encode($failed)
            . ' error=' . json_encode($process->getErrorOutput());

        // Without a second argument the child's output is forwarded rather than captured, which
        // is a different branch of the same method; the redirection keeps it out of the terminal
        // so the trace stays the only thing under comparison.
        $code = $process->execute('echo forwarded > forwarded.txt');
        $lines[] = 'forwarded code=' . $code
            . ' file=' . json_encode(trim((string) @file_get_contents('forwarded.txt')));

        // A callable second argument drives the child's output itself and is never assigned to.
        $seen = [];
        $callback = static function (string $type, string $buffer) use (&$seen): void {
            $seen[] = $type . ':' . trim($buffer);
        };
        $code = $process->execute('echo through-a-callback', $callback);
        $lines[] = 'callback code=' . $code . ' seen=' . json_encode($seen)
            . ' argument=' . json_encode(\is_callable($callback));

        $code = $process->execute('pwd', $cwdOutput, 'vendor');
        $lines[] = 'cwd code=' . $code . ' basename=' . json_encode(basename(trim((string) $cwdOutput)));

        $code = $process->execute('echo x; echo y', $multiline);
        $lines[] = 'splitLines code=' . $code
            . ' lines=' . json_encode($process->splitLines($multiline))
            . ' empty=' . json_encode($process->splitLines(null));

        $lines[] = 'escape=' . json_encode(ProcessExecutor::escape("a b'c"));
        // TODO(php-semantics): a command matching GIT_CMDS_NEED_GIT_DIR has no agreed value to
        // compare. array_intersect() keeps its first argument's keys and `===` compares an
        // array's keys too, so Composer answers false for those patterns as well; the shim's
        // array_intersect drops the keys and Shirabe answers true.
        $lines[] = 'requiresGitDirEnv status='
            . json_encode($process->requiresGitDirEnv('git status'));

        $process->setMaxJobs(4);
        $process->resetMaxJobs();
        $lines[] = 'maxJobs=ok';

        // The executor is a constructor argument of other Composer utilities, so a plugin-built
        // one has to be accepted wherever the real class is.
        $filesystem = new Filesystem($process);
        $lines[] = 'filesystem normalizePath=' . json_encode($filesystem->normalizePath('/a/b/../c'))
            . ' isLocalPath=' . json_encode(Filesystem::isLocalPath('/a/b'))
            . ' getPlatformPath=' . json_encode(Filesystem::getPlatformPath('file:///a/b'));

        file_put_contents('process-executor-trace.txt', implode("\n", $lines) . "\n");
    }
}
