<?php

// The Capable flavour of RustPluginStub: `$plugin instanceof Capable` decides whether Composer
// asks a plugin for capabilities, so the stub class is picked to match what the Rust-side
// plugin implements.

namespace Shirabe;

use Composer\Plugin\Capable;

class RustCapablePluginStub extends RustPluginStub implements Capable
{
    public function getCapabilities()
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getCapabilities', []);
    }
}
