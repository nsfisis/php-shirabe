<?php

declare(strict_types=1);

namespace Shirabe\Lint\Linters;

use Shirabe\Lint\Linter;
use Shirabe\Lint\Support\FileFinder;
use Shirabe\Lint\Support\Paths;

/**
 * Checks the types that follow PHP's object model: they are shared as `Rc<T>` / `Rc<dyn T>` with
 * `&self` methods, and keep their mutable properties in `shirabe_php_shim::PhpProp`.
 *
 * - The struct of such a type does not hold a property in a raw `Cell` / `RefCell`.
 * - No file wraps such a type (or its trait) in a `RefCell`.
 */
final class PhpObjectModel implements Linter
{
    /**
     * The structs that follow the model, keyed by the root-relative path of the file defining them.
     *
     * @var array<string, list<string>>
     */
    private const STRUCTS = [
        'crates/shirabe/src/composer.rs' => ['PartialComposer', 'Composer'],
        'crates/shirabe/src/config.rs' => ['Config'],
        'crates/shirabe/src/downloader/download_manager.rs' => ['DownloadManager'],
        'crates/shirabe/src/event_dispatcher/event_dispatcher.rs' => ['EventDispatcher'],
        'crates/shirabe/src/installer/installation_manager.rs' => ['InstallationManager'],
        'crates/shirabe/src/io/buffer_io.rs' => ['BufferIO'],
        'crates/shirabe/src/io/console_io.rs' => ['ConsoleIO'],
        'crates/shirabe/src/io/null_io.rs' => ['NullIO'],
        'crates/shirabe/src/package/locker.rs' => ['Locker'],
        'crates/shirabe/src/repository/repository_manager.rs' => ['RepositoryManager'],
    ];

    /** The types and traits that are never wrapped in a `RefCell`. */
    private const HANDLE_TYPES = [
        'Composer',
        'Config',
        'DownloadManager',
        'DownloadManagerInterface',
        'EventDispatcherInterface',
        'InstallationManager',
        'InstallationManagerInterface',
        'IOInterface',
        'Locker',
        'LockerInterface',
        'PartialComposer',
        'PartialOrFullComposer',
        'RepositoryManager',
        'RepositoryManagerInterface',
    ];

    public function name(): string
    {
        return 'php_object_model';
    }

    public function failureIntro(): string
    {
        return "Found a type following PHP's object model used with a raw `Cell` / `RefCell`.\n"
            . 'Share it as `Rc<T>` / `Rc<dyn T>` and keep its mutable properties in `PhpProp`:';
    }

    public function check(string $rootDir, array $excludes): array
    {
        $errors = [];

        foreach (self::STRUCTS as $relative => $structs) {
            if (in_array($relative, $excludes, true)) {
                continue;
            }
            $path = "{$rootDir}/{$relative}";
            if (!is_file($path)) {
                $errors[] = "{$relative}: file listed in " . self::class . ' does not exist';
                continue;
            }
            foreach ($structs as $struct) {
                array_push($errors, ...$this->checkStruct($path, $relative, $struct));
            }
        }

        $handles = implode('|', array_map(static fn (string $t): string => preg_quote($t, '/'), self::HANDLE_TYPES));
        $pattern = '/\bRefCell<\s*(?:dyn\s+)?(?:[A-Za-z_][A-Za-z0-9_]*::)*(' . $handles . ')\s*>/';

        foreach (FileFinder::rustFiles($rootDir) as $path) {
            $relative = Paths::relativeTo($rootDir, $path);
            if (in_array($relative, $excludes, true)) {
                continue;
            }
            foreach (file($path) as $idx => $raw) {
                $code = explode('//', $raw, 2)[0];
                if (preg_match_all($pattern, $code, $m)) {
                    foreach ($m[1] as $type) {
                        $errors[] = "{$relative}:" . ($idx + 1) . ": `{$type}` wrapped in a `RefCell`";
                    }
                }
            }
        }

        return $errors;
    }

    /** @return list<string> */
    private function checkStruct(string $path, string $relative, string $struct): array
    {
        $lines = file($path);
        $start = null;
        foreach ($lines as $idx => $raw) {
            if (preg_match('/^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+' . preg_quote($struct, '/') . '\b.*\{\s*$/', $raw)) {
                $start = $idx;
                break;
            }
        }
        if ($start === null) {
            return ["{$relative}: struct `{$struct}` listed in " . self::class . ' is not defined with a field block'];
        }

        $errors = [];
        for ($idx = $start + 1; $idx < count($lines); $idx++) {
            $code = explode('//', $lines[$idx], 2)[0];
            if (preg_match('/^\s*\}/', $code)) {
                break;
            }
            if (preg_match('/^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*(?:std::cell::)?(Ref)?Cell</', $code, $m)) {
                $errors[] = "{$relative}:" . ($idx + 1) . ": property `{$struct}::{$m[1]}` held in a raw `" . ($m[2] ?? '') . 'Cell` (use `PhpProp`)';
            }
        }

        return $errors;
    }
}
