<?php

// PHP glue worker. See docs/dev/php-rpc.md.

$client = @stream_socket_client('unix://' . $argv[1], $errno, $errstr);
if ($client === false) {
    exit(1);
}
// Port of Composer\XdebugHandler\XdebugHandler::setXdebugDetails(), which the diagnose payload
// reports as `xdebug_active`.
$xdebug_active = static function (): bool {
    if (!extension_loaded('xdebug')) {
        return false;
    }

    $version = phpversion('xdebug');
    $version = $version !== false ? $version : 'unknown';

    if (version_compare($version, '3.1', '>=')) {
        $modes = xdebug_info('mode');
        return (count($modes) === 0 ? 'off' : implode(',', $modes)) !== 'off';
    }

    $ini_mode = ini_get('xdebug.mode');
    if ($ini_mode === false) {
        return true;
    }

    $env_mode = (string) getenv('XDEBUG_MODE');
    if ($env_mode !== '') {
        $mode = $env_mode;
    } else {
        $mode = $ini_mode !== '' ? $ini_mode : 'off';
    }

    if (preg_match('/^,+$/', str_replace(' ', '', $mode)) === 1) {
        $mode = 'off';
    }

    return $mode !== 'off';
};
$dispatch = [
    'defined'  => static fn($name) => defined($name),
    'constant' => static fn($name) => defined($name) ? constant($name) : null,
    'inet_pton' => static fn($arg) => @inet_pton($arg),
    'curl_version' => static fn($arg) => function_exists('curl_version') ? (curl_version()['version'] ?? null) : null,
    'phpversion' => static fn($name) => phpversion($name),
    'get_loaded_extensions' => static fn($arg) => get_loaded_extensions(),
    'get_all_ini_files' => static function ($arg) {
        $paths = [(string) php_ini_loaded_file()];
        $scanned = php_ini_scanned_files();
        if ($scanned !== false) {
            $paths = array_merge($paths, array_map('trim', explode(',', $scanned)));
        }
        return $paths;
    },
    'extension_info' => static function ($name) {
        if (!extension_loaded($name)) {
            return '';
        }
        $re = new ReflectionExtension($name);
        ob_start();
        $re->info();
        return (string) ob_get_clean();
    },
    'diagnose' => static function ($arg) use ($xdebug_active) {
        $extensions = [];
        foreach ([
            'apcu',
            'curl',
            'filter',
            'hash',
            'iconv',
            'ionCube Loader',
            'mbstring',
            'openssl',
            'Phar',
            'uopz',
            'zip',
            'zlib',
        ] as $extension) {
            $extensions[$extension] = extension_loaded($extension);
        }

        $functions = [];
        foreach (['disk_free_space', 'json_decode', 'proc_open'] as $function) {
            $functions[$function] = function_exists($function);
        }

        $ini = [];
        foreach ([
            'allow_url_fopen',
            'apc.enable_cli',
            'uopz.disable',
            'uopz.exit',
            'xdebug.profiler_enabled',
        ] as $setting) {
            $value = ini_get($setting);
            $ini[$setting] = $value === false ? null : $value;
        }

        ob_start();
        phpinfo(INFO_GENERAL);
        $phpinfo = (string) ob_get_clean();

        return [
            'php_version' => PHP_VERSION,
            'php_version_id' => PHP_VERSION_ID,
            'php_binary' => defined('PHP_BINARY') ? PHP_BINARY : null,
            'openssl_version_text' => defined('OPENSSL_VERSION_TEXT') ? OPENSSL_VERSION_TEXT : null,
            'openssl_version_number' => defined('OPENSSL_VERSION_NUMBER') ? OPENSSL_VERSION_NUMBER : 0,
            'has_hhvm_version' => defined('HHVM_VERSION'),
            'has_php_windows_version_build' => defined('PHP_WINDOWS_VERSION_BUILD'),
            'xdebug_active' => $xdebug_active(),
            'ioncube_loader_iversion' => extension_loaded('ionCube Loader') ? ioncube_loader_iversion() : 0,
            'ioncube_loader_version' => extension_loaded('ionCube Loader') ? ioncube_loader_version() : '',
            'phpinfo_general' => $phpinfo,
            'extensions' => $extensions,
            'functions' => $functions,
            'ini' => $ini,
        ];
    },
];
$read_exact = static function ($conn, int $len): ?string {
    $buf = '';
    while (strlen($buf) < $len) {
        $chunk = fread($conn, $len - strlen($buf));
        if ($chunk === false || $chunk === '') {
            return null;
        }
        $buf .= $chunk;
    }
    return $buf;
};
while (true) {
    $header = $read_exact($client, 8);
    if ($header === null) {
        break;
    }
    $len = unpack('P', $header)[1];
    $name = $len === 0 ? '' : $read_exact($client, $len);
    if ($name === null) {
        break;
    }
    $sep = strpos($name, "\0");
    $arg = null;
    if ($sep !== false) {
        $arg = substr($name, $sep + 1);
        $name = substr($name, 0, $sep);
    }
    $result = isset($dispatch[$name]) ? ($dispatch[$name])($arg) : null;
    $payload = serialize($result);
    fwrite($client, pack('P', strlen($payload)) . $payload);
}
