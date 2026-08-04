<?php

// Shirabe's own definition of Composer\Console\Application for the plugin worker. The real
// class file is never autoloaded in this process: plugin-provided commands run here under a
// genuine Symfony console application, and this class supplies the Composer-specific surface
// (getIO()/getComposer()/...) backed by the Rust process over RPC. Defining the same FQCN
// instead of subclassing the real class keeps both `instanceof` and strict `get_class()`
// comparisons identical to upstream.
//
// Unlike the files under stubs/, the classes under runtime/ are hand-written: they are
// two-world implementations with behavior of their own, not mechanical proxies, so the stub
// generator does not manage them.

namespace Composer\Console;

use Composer\Composer;
use Composer\IO\IOInterface;
use Symfony\Component\Console\Application as BaseApplication;
use Symfony\Component\Console\Input\InputDefinition;
use Symfony\Component\Console\Input\InputOption;

class Application extends BaseApplication
{
    private static bool $shirabeConstructing = false;

    /** @var Composer|null */
    private $shirabeComposer = null;
    /** @var IOInterface */
    private $shirabeIo;
    /** @var string|null */
    private $shirabeInitialWorkingDirectory = null;
    private bool $shirabeDisablePluginsByDefault = false;
    private bool $shirabeDisableScriptsByDefault = false;

    /**
     * Builds the worker-side application from the Rust-side handoff. $config keys:
     * `composer` (Composer proxy stub or null), `io` (IO proxy stub),
     * `initialWorkingDirectory` (?string), `disablePluginsByDefault` (bool),
     * `disableScriptsByDefault` (bool), `rustCommands` (metadata rows for
     * \Shirabe\RustCommandStub), `pluginCommands` (live command entities from the P table).
     */
    public static function __shirabeBoot(array $config): self
    {
        self::$shirabeConstructing = true;
        try {
            $app = new self();
        } finally {
            self::$shirabeConstructing = false;
        }
        $app->setAutoExit(false);
        // A command failure must reach the Rust side as an RPC throw, not get rendered here.
        $app->setCatchExceptions(false);
        $app->shirabeComposer = $config['composer'];
        $app->shirabeIo = $config['io'];
        $app->shirabeInitialWorkingDirectory = $config['initialWorkingDirectory'];
        $app->shirabeDisablePluginsByDefault = $config['disablePluginsByDefault'];
        $app->shirabeDisableScriptsByDefault = $config['disableScriptsByDefault'];
        foreach ($config['rustCommands'] as $meta) {
            $app->add(new \Shirabe\RustCommandStub($meta));
        }
        foreach ($config['pluginCommands'] as $command) {
            $app->add($command);
        }

        return $app;
    }

    public function __construct(string $name = 'Composer', string $version = '')
    {
        if (!self::$shirabeConstructing) {
            // TODO(plugin): a plugin constructing its own Composer\Console\Application would
            // run commands outside the Rust-side orchestration; whether and how to support
            // that is undecided, so fail loudly instead of handing out a half-wired instance.
            throw new \RuntimeException(
                'Shirabe does not support constructing Composer\Console\Application inside the plugin process yet'
            );
        }
        parent::__construct($name, $version !== '' ? $version : Composer::getVersion());
    }

    public function getIO(): IOInterface
    {
        return $this->shirabeIo;
    }

    public function getComposer(bool $required = true, ?bool $disablePlugins = null, ?bool $disableScripts = null): ?Composer
    {
        if (null === $this->shirabeComposer && $required) {
            // TODO(plugin): creating a Composer instance from scratch here would need
            // Factory::create over RPC; the handoff currently always carries the instance the
            // Rust side already built, so this only fires after resetComposer-style flows.
            throw new \RuntimeException(
                'Shirabe cannot create a new Composer instance inside the plugin process yet'
            );
        }

        return $this->shirabeComposer;
    }

    public function resetComposer(): void
    {
        // TODO(plugin): the Composer instance is owned by the Rust process; dropping only the
        // worker-side reference would desynchronize the two worlds, so this needs a
        // reset-and-refetch round trip that does not exist yet.
        throw new \RuntimeException(
            'Shirabe does not support resetComposer() inside the plugin process yet'
        );
    }

    /**
     * @return string|null
     */
    public function getInitialWorkingDirectory()
    {
        return $this->shirabeInitialWorkingDirectory;
    }

    public function getDisablePluginsByDefault(): bool
    {
        return $this->shirabeDisablePluginsByDefault;
    }

    public function getDisableScriptsByDefault(): bool
    {
        return $this->shirabeDisableScriptsByDefault;
    }

    // Same as the real class: without Composer's global options in the definition, binding a
    // forwarded command line that carries e.g. --working-dir would fail here even though the
    // Rust side already consumed the option.
    protected function getDefaultInputDefinition(): InputDefinition
    {
        $definition = parent::getDefaultInputDefinition();
        $definition->addOption(new InputOption('--profile', null, InputOption::VALUE_NONE, 'Display timing and memory usage information'));
        $definition->addOption(new InputOption('--no-plugins', null, InputOption::VALUE_NONE, 'Whether to disable plugins.'));
        $definition->addOption(new InputOption('--no-scripts', null, InputOption::VALUE_NONE, 'Skips the execution of all scripts defined in composer.json file.'));
        $definition->addOption(new InputOption('--working-dir', '-d', InputOption::VALUE_REQUIRED, 'If specified, use the given directory as working directory.'));
        $definition->addOption(new InputOption('--no-cache', null, InputOption::VALUE_NONE, 'Prevent use of the cache'));

        return $definition;
    }
}
