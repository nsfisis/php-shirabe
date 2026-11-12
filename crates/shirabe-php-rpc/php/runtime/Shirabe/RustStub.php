<?php

namespace Shirabe;

/** Marker interface every Rust-proxy stub class implements. */
interface RustStub
{
    /**
     * Null when this instance holds no Rust handle: a dual-mode class (see php/runtime/) that
     * was constructed natively in this process crosses the wire as a P-table entity instead.
     *
     */
    public function __shirabeRustHandleDescriptor(): ?Wire\RustHandle;

    /** Binds this stub to an existing Rust entity, in place of running its constructor. */
    public function __shirabeBind(int $rhandle): void;
}
