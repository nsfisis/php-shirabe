<?php

// PHP glue worker. See docs/dev/php-rpc.md for the frame layout and message catalogue.

// Shirabe maps PHP's int to a 64-bit integer, and the frame codec below relies on pack('P').
// Exiting ends the Shirabe process with the same status on its first call to the worker.
if (PHP_INT_SIZE !== 8) {
    fwrite(STDERR, 'Shirabe requires a 64-bit build of PHP and you are running a 32-bit build of PHP '.PHP_VERSION.'. Aborting.'.PHP_EOL);
    exit(1);
}

(static function (array $argv): void {
    // The socket is one end of a socketpair the parent installed on this descriptor before exec;
    // there is nothing to connect to. See docs/dev/php-rpc.md.
    $client = @fopen('php://fd/' . $argv[1], 'r+b');
    if ($client === false) {
        exit(1);
    }
    // Frames must reach the parent as they are written, not when a buffer happens to fill.
    stream_set_write_buffer($client, 0);

    // Proxy stub classes take priority over any other autoloader (including autoloaders that a
    // script or the composer runtime registers later), so a proxied FQCN can never be shadowed by
    // the real implementation. `__shirabe_require` re-prepends this closure after loading code
    // that registers its own prepending autoloader. The guards come after the stubs: they cover
    // the Rust-owned classes no stub proxies, so the two directories never hold the same FQCN.
    // The worker's own classes (`Shirabe\`) are resolved through it as well.
    $dirs = [$argv[2] ?? null, $argv[3] ?? null];
    $stubAutoloader = static function (string $class) use ($dirs): void {
        $relative = '/' . str_replace('\\', '/', $class) . '.php';
        foreach ($dirs as $dir) {
            if ($dir !== null && is_file($dir . $relative)) {
                require $dir . $relative;
                return;
            }
        }
    };
    spl_autoload_register($stubAutoloader, true, true);

    \Shirabe\RpcRuntime::$socket = $client;
    \Shirabe\RpcRuntime::$stubAutoloader = $stubAutoloader;
    \Shirabe\RpcRuntime::$dispatch = \Shirabe\EntryPoints::table();
    \Shirabe\RpcRuntime::serveForever();
})($argv);
