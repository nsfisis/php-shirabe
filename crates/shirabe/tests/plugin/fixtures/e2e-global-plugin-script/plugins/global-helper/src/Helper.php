<?php

namespace ShirabeTest\GlobalHelper;

class Helper
{
    public static function trace(string $line): void
    {
        file_put_contents('global-plugin-script-trace.txt', $line . "\n", FILE_APPEND);
    }
}
