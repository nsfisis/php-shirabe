<?php

// PHP glue worker. See docs/dev/php-rpc.md.

$client = @stream_socket_client('unix://' . $argv[1], $errno, $errstr);
if ($client === false) {
    exit(1);
}
$dispatch = [
    'defined'  => static fn($name) => defined($name),
    'constant' => static fn($name) => defined($name) ? constant($name) : null,
    'inet_pton' => static fn($arg) => @inet_pton($arg),
    'curl_version' => static fn($arg) => function_exists('curl_version') ? (curl_version()['version'] ?? null) : null,
    'phpversion' => static fn($name) => phpversion($name),
    'get_loaded_extensions' => static fn($arg) => implode(',', get_loaded_extensions()),
    'extension_info' => static function ($name) {
        if (!extension_loaded($name)) {
            return '';
        }
        $re = new ReflectionExtension($name);
        ob_start();
        $re->info();
        return (string) ob_get_clean();
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
