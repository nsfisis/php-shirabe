<?php

// Hand-written proxy stub for Composer\Composer, kept in the shape the future stub generator
// will output. Class constants and static methods are materialized (they read no instance
// state); instance methods forward to the Rust-side entity.

namespace Composer;

use Composer\Autoload\AutoloadGenerator;
use Composer\Package\Archiver\ArchiveManager;
use Composer\Package\Locker;
use Composer\Downloader\DownloadManager;
use Composer\Pcre\Preg;
use Composer\Plugin\PluginManager;

class Composer extends PartialComposer
{
    public const VERSION = '2.9.7';
    public const BRANCH_ALIAS_VERSION = '';
    public const RELEASE_DATE = '2026-04-14 13:31:52';
    public const SOURCE_VERSION = '';
    public const RUNTIME_API_VERSION = '2.2.2';

    public static function getVersion(): string
    {
        // no replacement done, this must be a source checkout
        if (self::VERSION === '@package_version'.'@') {
            return self::SOURCE_VERSION;
        }

        // we have a branch alias and version is a commit id, this must be a snapshot build
        if (self::BRANCH_ALIAS_VERSION !== '' && Preg::isMatch('{^[a-f0-9]{40}$}', self::VERSION)) {
            return self::BRANCH_ALIAS_VERSION.'+'.self::VERSION;
        }

        return self::VERSION;
    }

    public function setLocker(Locker $locker): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setLocker', [$locker]);
    }

    public function getLocker(): Locker
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getLocker', []);
    }

    public function setDownloadManager(DownloadManager $manager): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setDownloadManager', [$manager]);
    }

    public function getDownloadManager(): DownloadManager
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getDownloadManager', []);
    }

    public function setArchiveManager(ArchiveManager $manager): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setArchiveManager', [$manager]);
    }

    public function getArchiveManager(): ArchiveManager
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getArchiveManager', []);
    }

    public function setPluginManager(PluginManager $manager): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setPluginManager', [$manager]);
    }

    public function getPluginManager(): PluginManager
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getPluginManager', []);
    }

    public function setAutoloadGenerator(AutoloadGenerator $autoloadGenerator): void
    {
        \ShirabeRpcRuntime::callRust($this->__rhandle, 'setAutoloadGenerator', [$autoloadGenerator]);
    }

    public function getAutoloadGenerator(): AutoloadGenerator
    {
        return \ShirabeRpcRuntime::callRust($this->__rhandle, 'getAutoloadGenerator', []);
    }
}
