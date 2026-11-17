<?php

namespace Shirabe;

/** Marker interface every Rust-proxy stub class implements. */
interface RustStub
{
    /**
     * Null when this instance holds no Rust entity id: a dual-mode class (see php/runtime/) that
     * was constructed natively in this process is passed to the Rust side as a PHP entity instead.
     *
     */
    public function __shirabeRustEntityHandle(): ?Rpc\RustEntityHandle;

    /** Binds this stub to an existing Rust entity, in place of running its constructor. */
    public function __shirabeBind(int $entityId): void;
}
