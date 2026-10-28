//! Benchmarks for `ProcessExecutor::execute_async`.

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use shirabe::util::process_executor::ProcessExecutor;

/// Pinned so the measurement does not depend on `COMPOSER_MAX_PARALLEL_PROCESSES`, which
/// `ProcessExecutor::new` otherwise reads to size the semaphore.
const MAX_JOBS: i64 = 10;

/// The cheapest child that still goes through the whole spawn path.
const COMMAND: &[&str; 1] = &["true"];

/// Concurrency within a case comes from joining futures inside a single task on a current-thread
/// runtime.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build the benchmark runtime")
}

fn executor() -> ProcessExecutor {
    let mut process_executor = ProcessExecutor::new(None);
    process_executor.enable_async();
    process_executor.set_max_jobs(MAX_JOBS);
    process_executor
}

fn bench_single(c: &mut Criterion) {
    let runtime = runtime();
    let mut group = c.benchmark_group("execute_async/single");

    group.bench_function("argv", |b| {
        b.to_async(&runtime).iter_batched(
            executor,
            |mut process_executor| async move {
                let process = process_executor.execute_async(COMMAND, None);
                process.await.expect("the child failed to run")
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("shell", |b| {
        b.to_async(&runtime).iter_batched(
            executor,
            |mut process_executor| async move {
                let process = process_executor.execute_async("true", None);
                process.await.expect("the child failed to run")
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_concurrent(c: &mut Criterion) {
    let runtime = runtime();
    let mut group = c.benchmark_group("execute_async/concurrent");

    for jobs in [1_u64, MAX_JOBS as u64, MAX_JOBS as u64 * 2] {
        group.throughput(Throughput::Elements(jobs));
        group.bench_with_input(BenchmarkId::from_parameter(jobs), &jobs, |b, &jobs| {
            b.to_async(&runtime).iter_batched(
                executor,
                |mut process_executor| async move {
                    let processes: Vec<_> = (0..jobs)
                        .map(|_| process_executor.execute_async(COMMAND, None))
                        .collect();
                    futures::future::join_all(processes)
                        .await
                        .into_iter()
                        .map(|process| process.expect("the child failed to run"))
                        .collect::<Vec<_>>()
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_single, bench_concurrent);
criterion_main!(benches);
