<?php

namespace Shirabe;

/**
 * The worker's half of the job registry (docs/dev/plugin-async.md). A job is an asynchronous
 * operation one side runs and the other side holds a promise for; only its id crosses the wire.
 * The Rust side allocates odd ids, this side even ones.
 */
final class JobRegistry
{
    /**
     * Rust-owned jobs that have not settled yet. The entry holds the Deferred strongly: it has
     * to outlive the plugin dropping its promise, until the Rust side reports the outcome.
     *
     * @var array<int, \React\Promise\Deferred>
     */
    private static array $deferreds = [];
    /**
     * PHP-owned jobs that have not settled yet. Holding the promise keeps its object id from
     * being reused while the id is interned.
     *
     * @var array<int, \React\Promise\PromiseInterface>
     */
    private static array $promises = [];
    /** @var array<int, int> spl_object_id => job id, so one promise keeps one id */
    private static array $idsByObjectId = [];
    /**
     * Settled promises built here on the Rust side's behalf, each waiting for the answer that
     * carries its id back.
     *
     * @var array<int, \React\Promise\PromiseInterface>
     */
    private static array $parked = [];
    /** @var array<int, int> spl_object_id => job id of a parked promise */
    private static array $parkedIdsByObjectId = [];
    private static int $nextId = 2;

    private static function requireReact(): void
    {
        if (!class_exists(\React\Promise\Deferred::class)) {
            throw new \RuntimeException(
                'react/promise is not loaded in the plugin process, so a PromiseInterface cannot be built'
            );
        }
    }

    /** The promise standing for a job id that arrived over the wire. */
    public static function promiseFor(int $id): \React\Promise\PromiseInterface
    {
        if ($id % 2 === 0) {
            if (isset(self::$parked[$id])) {
                $promise = self::$parked[$id];
                unset(self::$parked[$id], self::$parkedIdsByObjectId[spl_object_id($promise)]);
                return $promise;
            }
            if (!isset(self::$promises[$id])) {
                throw new \RuntimeException("unknown PHP-owned job {$id}");
            }
            return self::$promises[$id];
        }
        if (!isset(self::$deferreds[$id])) {
            self::requireReact();
            self::$deferreds[$id] = new \React\Promise\Deferred(static function () use ($id): void {
                // The Rust side drops the job, so no settle push follows either way. It answers
                // with the exception a started job is cancelled with, and with null for one that
                // had not started, which Composer aborts without rejecting.
                unset(self::$deferreds[$id]);
                $thrown = RpcRuntime::callRust(0, '__shirabe_cancel_job', [$id]);
                if ($thrown !== null) {
                    throw MaterializedThrowable::revive($thrown);
                }
            });
        }
        return self::$deferreds[$id]->promise();
    }

    private static function take(int $id): \React\Promise\Deferred
    {
        if (!isset(self::$deferreds[$id])) {
            throw new \RuntimeException("unknown Rust-owned job {$id}");
        }
        $deferred = self::$deferreds[$id];
        unset(self::$deferreds[$id]);
        return $deferred;
    }

    /** Runs the callbacks chained on the job's promise before it returns. */
    public static function resolve(int $id, $value): void
    {
        self::take($id)->resolve($value);
    }

    public static function reject(int $id, \Throwable $reason): void
    {
        self::take($id)->reject($reason);
    }

    /**
     * Parks a settled promise the Rust side asked for. Its descriptor names no job: the Rust
     * side passes it on as the answer of the call it is serving, and the promise is handed out
     * when that answer arrives.
     */
    public static function park(\React\Promise\PromiseInterface $promise): \React\Promise\PromiseInterface
    {
        $id = self::$nextId;
        self::$nextId += 2;
        self::$parked[$id] = $promise;
        self::$parkedIdsByObjectId[spl_object_id($promise)] = $id;
        return $promise;
    }

    /**
     * The descriptor of a promise leaving for the Rust side: the id it is parked under, or a
     * PHP-owned job. The recorder attached to a job reports the outcome when the promise
     * settles, which is right away for one that already has.
     */
    public static function descriptor(\React\Promise\PromiseInterface $promise): Wire\Job
    {
        $objectId = spl_object_id($promise);
        if (isset(self::$parkedIdsByObjectId[$objectId])) {
            return new Wire\Job(self::$parkedIdsByObjectId[$objectId]);
        }
        if (isset(self::$idsByObjectId[$objectId])) {
            return new Wire\Job(self::$idsByObjectId[$objectId]);
        }
        $id = self::$nextId;
        self::$nextId += 2;
        self::$idsByObjectId[$objectId] = $id;
        self::$promises[$id] = $promise;
        $forget = static function () use ($id, $objectId): void {
            unset(self::$promises[$id], self::$idsByObjectId[$objectId]);
        };
        $promise->then(
            static function ($value) use ($id, $forget): void {
                $forget();
                RpcRuntime::callRust(0, '__shirabe_resolve_job', [$id, $value]);
            },
            static function ($reason) use ($id, $forget): void {
                $forget();
                if (!$reason instanceof \Throwable) {
                    $reason = new \RuntimeException('the promise was rejected with ' . gettype($reason));
                }
                RpcRuntime::callRust(
                    0,
                    '__shirabe_reject_job',
                    [$id, MaterializedThrowable::describe($reason)]
                );
            }
        );
        return new Wire\Job($id);
    }
}
