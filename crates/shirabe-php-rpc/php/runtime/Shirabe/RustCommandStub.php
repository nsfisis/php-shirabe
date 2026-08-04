<?php

// Worker-side stand-in for a Rust-implemented (built-in) command: it carries only the list
// metadata, and running it forwards the raw input line back to the Rust process, where the
// real implementation executes against its own application state (helper set included). This
// is how a plugin command that invokes a built-in command crosses back over the process
// boundary; each command always runs on the side that owns its implementation, so a helper
// lookup always resolves against the helper set of the world the command was written for.

namespace Shirabe;

use Composer\Command\BaseCommand;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class RustCommandStub extends BaseCommand
{
    /** @param array{name: string, description: string, aliases: list<string>, hidden: bool} $meta */
    public function __construct(array $meta)
    {
        parent::__construct($meta['name']);
        $this->setDescription($meta['description']);
        $this->setAliases($meta['aliases']);
        $this->setHidden($meta['hidden']);
        // The real input definition lives on the Rust side and the raw tokens are forwarded
        // untouched, so nothing may be validated (or consumed) here.
        $this->ignoreValidationErrors();
    }

    public function run(InputInterface $input, OutputInterface $output): int
    {
        return (int) \ShirabeRpcRuntime::callRust(0, '__shirabe_run_rust_command', [$this->getName(), (string) $input]);
    }
}
