use std::{
    env,
    future::Future,
    hint::black_box,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tiny_wasm_runtime::WasmRuntimeAsyncEngine;

const DEFAULT_SPAWN_COUNT: usize = 50_000;
const DEFAULT_MULTI_TASK_COUNT: usize = 2_000;
const DEFAULT_YIELDS_PER_TASK: usize = 250;
const DEFAULT_SINGLE_TASK_YIELDS: usize = 500_000;

#[derive(Clone, Copy, Debug)]
struct BenchmarkConfig {
    spawn_count: usize,
    multi_task_count: usize,
    yields_per_task: usize,
    single_task_yields: usize,
}

impl BenchmarkConfig {
    fn from_args() -> Option<Self> {
        let args = env::args().skip(1).collect::<Vec<_>>();
        if args.iter().any(|arg| arg == "--help" || arg == "-h") {
            print_usage();
            return None;
        }

        Some(Self {
            spawn_count: parse_arg(&args, 0, DEFAULT_SPAWN_COUNT),
            multi_task_count: parse_arg(&args, 1, DEFAULT_MULTI_TASK_COUNT),
            yields_per_task: parse_arg(&args, 2, DEFAULT_YIELDS_PER_TASK),
            single_task_yields: parse_arg(&args, 3, DEFAULT_SINGLE_TASK_YIELDS),
        })
    }
}

// Small self-waking future to stress the scheduler without involving timers or I/O.
struct YieldBurst {
    remaining: usize,
    state: u64,
}

impl YieldBurst {
    fn new(remaining: usize, seed: u64) -> Self {
        Self {
            remaining,
            state: seed,
        }
    }

    fn advance(&mut self) {
        self.state = self
            .state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
    }
}

impl Future for YieldBurst {
    type Output = u64;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.remaining == 0 {
            return Poll::Ready(this.state);
        }

        this.advance();
        this.remaining -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

fn parse_arg(args: &[String], index: usize, default: usize) -> usize {
    args.get(index)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --release --example high_frequency_benchmark -- [spawn_count] [multi_task_count] [yields_per_task] [single_task_yields]");
    println!();
    println!("Defaults:");
    println!("  spawn_count       = {DEFAULT_SPAWN_COUNT}");
    println!("  multi_task_count  = {DEFAULT_MULTI_TASK_COUNT}");
    println!("  yields_per_task   = {DEFAULT_YIELDS_PER_TASK}");
    println!("  single_task_yields= {DEFAULT_SINGLE_TASK_YIELDS}");
}

fn print_result(label: &str, unit: &str, operations: usize, elapsed: Duration, checksum: u64) {
    let seconds = elapsed.as_secs_f64();
    let rate = if seconds > 0.0 {
        operations as f64 / seconds
    } else {
        f64::INFINITY
    };

    println!(
        "{label:<22} {elapsed_ms:>10.3} ms  {rate:>14.0} {unit}/s  checksum={checksum}",
        elapsed_ms = seconds * 1_000.0
    );
}

fn bench_spawn_churn(spawn_count: usize) -> (Duration, u64) {
    let start = Instant::now();
    let checksum = WasmRuntimeAsyncEngine::block_on(async move {
        let mut handles = Vec::with_capacity(spawn_count);
        for index in 0..spawn_count {
            handles.push(WasmRuntimeAsyncEngine::spawn(async move {
                (index as u64).wrapping_mul(3).wrapping_add(1)
            }));
        }

        let mut total = 0_u64;
        for handle in handles {
            total = total.wrapping_add(handle.await);
        }
        total
    });
    (start.elapsed(), black_box(checksum))
}

fn bench_single_task_yields(single_task_yields: usize) -> (Duration, u64) {
    let start = Instant::now();
    let checksum =
        WasmRuntimeAsyncEngine::block_on(
            async move { YieldBurst::new(single_task_yields, 7).await },
        );
    (start.elapsed(), black_box(checksum))
}

fn bench_multi_task_yields(task_count: usize, yields_per_task: usize) -> (Duration, u64) {
    let start = Instant::now();
    let checksum = WasmRuntimeAsyncEngine::block_on(async move {
        let mut handles = Vec::with_capacity(task_count);
        for index in 0..task_count {
            handles.push(WasmRuntimeAsyncEngine::spawn(async move {
                YieldBurst::new(yields_per_task, index as u64 + 11).await
            }));
        }

        let mut total = 0_u64;
        for handle in handles {
            total = total.wrapping_add(handle.await);
        }
        total
    });
    (start.elapsed(), black_box(checksum))
}

fn warm_up() {
    let _ = WasmRuntimeAsyncEngine::block_on(async {
        let mut handles = Vec::with_capacity(64);
        for index in 0..64 {
            handles.push(WasmRuntimeAsyncEngine::spawn(async move {
                YieldBurst::new(16, index as u64).await
            }));
        }

        let mut total = 0_u64;
        for handle in handles {
            total = total.wrapping_add(handle.await);
        }
        total
    });
}

fn main() {
    let Some(config) = BenchmarkConfig::from_args() else {
        return;
    };

    println!("tiny-wasm-runtime high-frequency benchmark");
    println!(
        "spawn_count={}, multi_task_count={}, yields_per_task={}, single_task_yields={}",
        config.spawn_count,
        config.multi_task_count,
        config.yields_per_task,
        config.single_task_yields
    );

    warm_up();

    let (spawn_elapsed, spawn_checksum) = bench_spawn_churn(config.spawn_count);
    print_result(
        "spawn churn",
        "tasks",
        config.spawn_count,
        spawn_elapsed,
        spawn_checksum,
    );

    let (single_elapsed, single_checksum) = bench_single_task_yields(config.single_task_yields);
    print_result(
        "single-task yields",
        "yields",
        config.single_task_yields,
        single_elapsed,
        single_checksum,
    );

    let multi_operations = config.multi_task_count * config.yields_per_task;
    let (multi_elapsed, multi_checksum) =
        bench_multi_task_yields(config.multi_task_count, config.yields_per_task);
    print_result(
        "multi-task yields",
        "yields",
        multi_operations,
        multi_elapsed,
        multi_checksum,
    );
}
