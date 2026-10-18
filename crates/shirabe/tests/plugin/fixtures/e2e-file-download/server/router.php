<?php

// Router of the PHP built-in web server the file download E2E test starts: a Composer repository
// serving one package whose dist URL the recorder plugin has to rewrite before it resolves.

$base = 'http://' . $_SERVER['HTTP_HOST'];
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);

if ($path === '/packages.json') {
    header('Content-Type: application/json');
    echo json_encode(['packages' => [], 'metadata-url' => '/p2/%package%.json']);

    return true;
}

if ($path === '/p2/shirabe-test/remote-lib.json') {
    header('Content-Type: application/json');
    echo json_encode([
        'packages' => [
            'shirabe-test/remote-lib' => [
                [
                    'name' => 'shirabe-test/remote-lib',
                    'version' => '1.0.0',
                    'version_normalized' => '1.0.0.0',
                    'dist' => [
                        'type' => 'zip',
                        'url' => $base . '/dists/missing.zip',
                        'shasum' => sha1_file(__DIR__ . '/dists/remote-lib.zip'),
                    ],
                ],
            ],
        ],
    ]);

    return true;
}

if ($path === '/dists/remote-lib.zip') {
    header('Content-Type: application/zip');
    readfile(__DIR__ . '/dists/remote-lib.zip');

    return true;
}

http_response_code(404);

return true;
