<?php

namespace Shirabe;

/**
 * Port of Composer\Platform\Runtime, whose runtime queries feed the `__shirabe_platform`
 * payload. Its parseHtmlExtensionInfo() has no counterpart here: the worker is always the CLI
 * SAPI, so getExtensionInfo() never takes the branch that reformats phpinfo()'s HTML output.
 */
final class PlatformRuntime
{
    public static function getExtensionInfo(string $extension): string
    {
        $reflector = new \ReflectionExtension($extension);

        ob_start();
        $reflector->info();

        return (string) ob_get_clean();
    }
}
