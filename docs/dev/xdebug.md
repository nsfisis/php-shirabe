# Xdebug

Composer restarts itself without Xdebug loaded, because Xdebug makes PHP
several times slower. Shirabe is written in Rust, where Xdebug does not exist,
but it invokes a PHP command for plugins, scripts and platform queries.
When Shirabe spawns a PHP worker, Xdebug is disabled as Composer does.

The PHP worker is spawned with the `-d xdebug.mode=off` flag and the
`XDEBUG_MODE=off` environment variable. The way to disable Xdebug in Shirabe is
different from Composer: Composer restarts its own process with a temporary
INI file, where Xdebug extension is disabled. The difference probably does not
matter, both for users and for plugin authors.

## Enable Xdebug in Shirabe's PHP worker

It is the same as Composer: setting `COMPOSER_ALLOW_XDEBUG` to 1 makes Shirabe
leave Xdebug enabled.

```
$ COMPOSER_ALLOW_XDEBUG=1 shirabe install
```

## Xdebug 2 support

Shirabe does not try to disable Xdebug version 2 because Xdebug 2 has no
`xdebug.mode` or `XDEBUG_MODE`, while Composer disables Xdebug 2 too. The
performance penalty seems to be small as Shirabe's CPU-heavy workloads are
written in Rust.
