<?php

declare(strict_types=1);

namespace Shirabe\PluginStubGenerator;

/**
 * Emits `crates/shirabe/src/php_worker/rpc/forwarding.rs`: one Rust module per worker-side class
 * that forwards members to the Rust side, laid out by the class's namespace the way the porting
 * rules lay out files (`Composer\Package\Locker` is `package::locker`, `Shirabe\RustPluginStub`
 * is `shirabe::rust_plugin_stub`).
 *
 * Each module holds `PhpClass`, a marker type standing for the class; `Forwarding`, a trait with
 * one method per member the class itself declares and forwards, whose default answers that the
 * member is not available; `dispatch`, which routes a method name to the `Forwarding` of the class
 * declaring it, the class itself or an ancestor; and `CROSSING`, the class and its `dispatch` as
 * one value. The trait of a class whose parent forwards nothing also names the handle type of the
 * entities crossing as it or as a descendant (`type Handle`); every descendant takes that type.
 */
final class ForwardingModules
{
    private const KEYWORDS = [
        'as', 'async', 'await', 'break', 'const', 'continue', 'dyn', 'else', 'enum', 'extern',
        'false', 'fn', 'for', 'gen', 'if', 'impl', 'in', 'let', 'loop', 'match', 'mod', 'move', 'mut',
        'pub', 'ref', 'return', 'static', 'struct', 'trait', 'true', 'try', 'type', 'unsafe', 'use',
        'where', 'while', 'abstract', 'become', 'box', 'do', 'final', 'macro', 'override', 'priv',
        'typeof', 'unsized', 'virtual', 'yield',
    ];

    /** @var \Closure(string): string */
    private readonly \Closure $root;

    /**
     * @param array<string, array{guard: bool, parent: ?string, abstract: bool, methods: list<string>, statics: list<string>, constructs: ?bool}> $classes
     * @param list<string> $generated the forwarding classes to emit a module for
     * @param list<string> $guards the guard classes forwarding static methods
     * @param callable(string): string $root the class a forwarding class descends from whose
     *                                       parent forwards nothing
     */
    public function __construct(
        private readonly array $classes,
        private readonly array $generated,
        private readonly array $guards,
        callable $root,
    ) {
        $this->root = \Closure::fromCallable($root);
    }

    /** A PHP member or class name as a Rust identifier: snake case, raw if it is a keyword. */
    public static function rustName(string $name): string
    {
        $snake = strtolower(preg_replace(
            ['/([A-Z]+)([A-Z][a-z])/', '/([a-z0-9])([A-Z])/'],
            ['$1_$2', '$1_$2'],
            $name,
        ));

        return in_array($snake, self::KEYWORDS, true) ? 'r#' . $snake : $snake;
    }

    /** @return list<string> the module path of a class */
    public static function modulePath(string $fqcn): array
    {
        $segments = explode('\\', $fqcn);
        if ($segments[0] === 'Composer') {
            array_shift($segments);
        }

        return array_map(static fn (string $segment): string => self::rustName($segment), $segments);
    }

    /**
     * Matches the hand-written impls with the generated traits.
     *
     * @param array<string, list<array{path: string, fns: list<string>}>> $impls module path => impls
     * @return array<string, list<string>> FQCN => the members its impl answers, as Rust names
     */
    public function overrides(array $impls): array
    {
        $errors = [];
        $overrides = [];
        foreach ([...$this->generated, ...$this->guards] as $fqcn) {
            $module = implode('::', self::modulePath($fqcn));
            $found = $impls[$module] ?? [];
            if (count($found) !== 1) {
                $errors[] = "expected exactly one `impl forwarding::$module::Forwarding for forwarding::$module::PhpClass`, found "
                    . (count($found) === 0 ? 'none' : implode(', ', array_column($found, 'path')));
                continue;
            }
            $overrides[$fqcn] = array_map(
                static fn (string $fn): string => str_starts_with($fn, 'r#') ? substr($fn, 2) : $fn,
                $found[0]['fns'],
            );
        }
        if ($errors !== []) {
            throw new GenerationError($errors);
        }

        return array_map(
            static fn (array $fns): array => array_map(
                static fn (string $fn): string => in_array($fn, self::KEYWORDS, true) ? 'r#' . $fn : $fn,
                $fns,
            ),
            $overrides,
        );
    }

    public function render(): string
    {
        $tree = [];
        $generated = [...$this->generated, ...$this->guards];
        sort($generated, SORT_STRING);
        foreach ($generated as $fqcn) {
            $node = &$tree;
            foreach (self::modulePath($fqcn) as $segment) {
                $node['children'][$segment] ??= [];
                $node = &$node['children'][$segment];
            }
            if (isset($node['class'])) {
                throw new GenerationError(["$fqcn and {$node['class']} map to the same Rust module"]);
            }
            $node['class'] = $fqcn;
            unset($node);
        }

        $code = "//! Generated by scripts/plugin-stub-generator/surface-manifest; do not edit by hand.\n"
            . "//! One module per worker-side class that forwards members to the Rust side. A class no\n"
            . "//! entity crosses as still has its `CROSSING`, which nothing refers to.\n"
            . "#![allow(dead_code)]\n\n"
            . "use crate::php_worker::rpc::entity::{Call, protocol_violation};\nuse crate::php_worker::rpc::forwarding;\n"
            . "use bstr::{BStr, ByteSlice};\n"
            . "use shirabe_php_rpc::{PhpThrow, PluginValue};\n\n"
            . $this->construct() . "\n" . $this->callStatic();
        foreach ($tree['children'] ?? [] as $segment => $child) {
            $code .= "\n" . $this->module($segment, $child);
        }

        return self::rustfmt($code);
    }

    /** @param array{children?: array<string, mixed>, class?: string} $node */
    private function module(string $segment, array $node): string
    {
        $code = "pub(crate) mod $segment {\n";
        if (isset($node['class'])) {
            $code .= $this->classItems($node['class']);
        }
        foreach ($node['children'] ?? [] as $child => $grandchild) {
            $code .= $this->module($child, $grandchild);
        }

        return $code . "}\n";
    }

    /** `__shirabe_construct`: builds the entity, registers it and answers its rhandle. */
    private function construct(): string
    {
        $arms = '';
        $generated = $this->generated;
        sort($generated, SORT_STRING);
        foreach ($generated as $fqcn) {
            if (!$this->classes[$fqcn]['abstract'] && $this->constructs($fqcn)) {
                $arms .= self::byteString($fqcn) . ' => ' . self::pathOf($fqcn) . "::construct(call),\n";
            }
        }

        return "/// `__shirabe_construct`: builds the entity, registers it and answers its rhandle. A stub's\n"
            . "/// constructor sends `static::class`, so a plugin's subclass of a proxied class reaches the\n"
            . "/// last arm.\n"
            . "pub(crate) fn construct(class: &BStr, call: &mut Call<'_>) -> Result<u64, PhpThrow> {\n"
            . "match class.as_bytes() {\n{$arms}_ => Err(call.unconstructible()),\n}\n}\n";
    }

    /** `__shirabe_call_static`. */
    private function callStatic(): string
    {
        $arms = '';
        $classes = [...$this->generated, ...$this->guards];
        sort($classes, SORT_STRING);
        foreach ($classes as $fqcn) {
            if ($this->classes[$fqcn]['statics'] !== []) {
                $arms .= self::byteString($fqcn) . ' => ' . self::pathOf($fqcn) . "::call_static(call),\n";
            }
        }

        return "/// `__shirabe_call_static`. A static forwarder sends `self::class`, so the class is always\n"
            . "/// one that declares the method.\n"
            . "pub(crate) fn call_static(class: &BStr, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\n"
            . "match class.as_bytes() {\n{$arms}_ => protocol_violation(call),\n}\n}\n";
    }

    private function guardItems(string $fqcn): string
    {
        $names = [];
        $items = [];
        foreach ($this->classes[$fqcn]['statics'] as $method) {
            $name = $this->claim($names, $fqcn, $method);
            $items[] = "fn $name(call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\nErr(call.unsupported())\n}\n";
        }

        return "use crate::php_worker::rpc::entity::{Call, protocol_violation};\nuse bstr::ByteSlice;\nuse shirabe_php_rpc::{PhpThrow, PluginValue};\n\n"
            . "/// `$fqcn`, whose static methods are forwarded.\n#[derive(Debug)]\npub(crate) struct PhpClass;\n\n"
            . "pub(crate) trait Forwarding {\n" . implode("\n", $items) . "}\n\n"
            . $this->staticRouter($fqcn);
    }

    private function classItems(string $fqcn): string
    {
        if ($this->classes[$fqcn]['guard']) {
            return $this->guardItems($fqcn);
        }
        $class = $this->classes[$fqcn];
        $root = ($this->root)($fqcn);
        $isRoot = $root === $fqcn;
        $handle = $isRoot ? '<PhpClass as Forwarding>::Handle' : self::traitOf($root) . '::Handle';
        $traitHandle = $isRoot ? 'Self::Handle' : $handle;
        $literal = self::byteString($fqcn);

        $entity = ['Call', 'Crossing', 'protocol_violation'];
        if ($isRoot) {
            $entity[] = 'Entity';
        }
        $constructs = !$class['abstract'] && $this->constructs($fqcn);
        if ($constructs) {
            $entity[] = 'constructed';
        }
        sort($entity, SORT_STRING);
        $code = 'use crate::php_worker::rpc::entity::{' . implode(', ', $entity) . "};\n";
        if (!$isRoot) {
            $code .= "use crate::php_worker::rpc::forwarding;\n";
        }
        $code .= "use bstr::ByteSlice;\nuse shirabe_php_rpc::{PhpThrow, PluginValue};\n\n";

        $code .= "/// `$fqcn`.\n#[derive(Debug)]\npub(crate) struct PhpClass;\n\n";

        $names = [];
        $items = [];
        if ($isRoot) {
            $items[] = "/// The handle type of the entities crossing as this class or a subclass.\ntype Handle: Entity;\n";
        }
        foreach ($class['methods'] as $method) {
            $name = $this->claim($names, $fqcn, $method);
            $items[] = "fn $name(this: &$traitHandle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\n"
                . "let _ = this;\nErr(call.unsupported())\n}\n";
        }
        if (!$class['abstract'] && $this->constructs($fqcn)) {
            $name = $this->claim($names, $fqcn, '__construct');
            $items[] = "fn $name(call: &mut Call<'_>) -> Result<$traitHandle, PhpThrow> {\nErr(call.unconstructible())\n}\n";
        }
        foreach ($class['statics'] as $method) {
            $name = $this->claim($names, $fqcn, $method);
            $items[] = "fn $name(call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\nErr(call.unsupported())\n}\n";
        }
        $code .= "pub(crate) trait Forwarding {\n" . implode("\n", $items) . "}\n\n";

        $arms = '';
        $seen = [];
        for ($current = $fqcn; $current !== null && isset($this->classes[$current]) && !$this->classes[$current]['guard']; $current = $this->classes[$current]['parent']) {
            $trait = $current === $fqcn ? '<PhpClass as Forwarding>' : self::traitOf($current);
            foreach ($this->classes[$current]['methods'] as $method) {
                if (isset($seen[$method])) {
                    continue;
                }
                $seen[$method] = true;
                $arms .= self::byteString($method) . " => $trait::" . self::rustName($method) . "(this, call),\n";
            }
            if ($current === $root) {
                break;
            }
        }
        $code .= "pub(crate) fn dispatch(this: &$handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\n"
            . "match call.method().as_bytes() {\n{$arms}_ => protocol_violation(call),\n}\n}\n\n";
        $code .= "pub(crate) static CROSSING: Crossing<$handle> = Crossing {\nname: $literal,\ndispatch,\n};\n";
        if ($constructs) {
            $code .= "\npub(crate) fn construct(call: &mut Call<'_>) -> Result<u64, PhpThrow> {\n"
                . "let entity = <PhpClass as Forwarding>::__construct(call)?;\nconstructed(call.class(), entity)\n}\n";
        }
        if ($class['statics'] !== []) {
            $code .= "\n" . $this->staticRouter($fqcn);
        }

        return $code;
    }

    private function staticRouter(string $fqcn): string
    {
        $arms = '';
        foreach ($this->classes[$fqcn]['statics'] as $method) {
            $arms .= self::byteString($method) . ' => <PhpClass as Forwarding>::' . self::rustName($method) . "(call),\n";
        }

        return "pub(crate) fn call_static(call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {\n"
            . "match call.method().as_bytes() {\n{$arms}_ => protocol_violation(call),\n}\n}\n";
    }

    /** @param array<string, true> $names */
    private function claim(array &$names, string $fqcn, string $member): string
    {
        $name = self::rustName($member);
        if (isset($names[$name])) {
            throw new GenerationError(["$fqcn: two members map to the Rust name $name"]);
        }
        $names[$name] = true;

        return $name;
    }

    private function constructs(string $fqcn): bool
    {
        for ($class = $fqcn; $class !== null && isset($this->classes[$class]); $class = $this->classes[$class]['parent']) {
            if ($this->classes[$class]['constructs'] !== null) {
                return $this->classes[$class]['constructs'];
            }
        }

        return false;
    }

    private static function pathOf(string $fqcn): string
    {
        return 'forwarding::' . implode('::', self::modulePath($fqcn));
    }

    private static function traitOf(string $fqcn): string
    {
        $path = 'forwarding::' . implode('::', self::modulePath($fqcn));

        return "<$path::PhpClass as $path::Forwarding>";
    }

    private static function byteString(string $value): string
    {
        return 'b"' . addcslashes($value, "\\\"") . '"';
    }

    private static function rustfmt(string $code): string
    {
        $process = proc_open(
            ['rustfmt', '--edition', '2024'],
            [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
            $pipes,
        );
        if (!is_resource($process)) {
            throw new GenerationError(['cannot run rustfmt']);
        }
        fwrite($pipes[0], $code);
        fclose($pipes[0]);
        $formatted = stream_get_contents($pipes[1]);
        $errors = stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        if (proc_close($process) !== 0) {
            throw new GenerationError(["rustfmt failed on the generated forwarding modules:\n$errors"]);
        }

        return $formatted;
    }
}
