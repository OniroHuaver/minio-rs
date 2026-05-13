//! Grid micro-benchmarks.
//!
//! Measure single-request throughput and latency using the simulated loopback pair.
//! All benchmarks are `#[ignore]` by default (run with `cargo test -- --ignored`).

use std::sync::Arc;
use std::time::Instant;

use crate::grid::connection::auth_validate_token;
use crate::grid::connection::Connection;
use crate::grid::handler::{single_handler_fn, HandlerRegistry};
use crate::grid::ConnectionState;

const HANDLER_ECHO: u8 = 100;

async fn bench_pair() -> (Connection, Connection) {
    let handlers = Arc::new(HandlerRegistry::new());
    handlers.singles.write().await.insert(
        HANDLER_ECHO,
        single_handler_fn(|payload: Vec<u8>| async move { Ok(payload) }),
    );

    let conn_a = Connection::new(
        [1u8; 16],
        "a:9000".into(),
        "b:9000".into(),
        true,
        "t".into(),
        auth_validate_token("t".into()),
        handlers.clone(),
    );
    let conn_b = Connection::new(
        [2u8; 16],
        "b:9000".into(),
        "a:9000".into(),
        false,
        "t".into(),
        auth_validate_token("t".into()),
        handlers,
    );
    conn_a.set_state(ConnectionState::Connected);
    conn_b.set_state(ConnectionState::Connected);

    tokio::spawn({
        let ca = conn_a.clone();
        let cb = conn_b.clone();
        let mut ra = conn_a.take_out_rx().expect("out_rx");
        let mut rb = conn_b.take_out_rx().expect("out_rx");
        async move {
            loop {
                tokio::select! {
                    Some(msg) = ra.recv() => { cb.dispatch(msg).await; }
                    Some(msg) = rb.recv() => { ca.dispatch(msg).await; }
                    else => break,
                }
            }
        }
    });
    tokio::task::yield_now().await;
    (conn_a, conn_b)
}

/// Single-request echo latency and throughput.
///
/// 512B payload, measures round-trip time.
#[tokio::test]
#[ignore]
async fn bench_grid_requests() {
    const ITERATIONS: usize = 10_000;
    const PAYLOAD_SIZE: usize = 512;

    let (conn, _) = bench_pair().await;
    let payload = vec![0xAAu8; PAYLOAD_SIZE];

    // Warmup.
    for _ in 0..100 {
        conn.request(HANDLER_ECHO, Some(payload.clone()), None)
            .await
            .unwrap();
    }

    // Timed run.
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        conn.request(HANDLER_ECHO, Some(payload.clone()), None)
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();

    let rps = ITERATIONS as f64 / elapsed.as_secs_f64();
    let us_per_op = elapsed.as_micros() as f64 / ITERATIONS as f64;

    println!(
        "\nbench_grid_requests ({}B payload):\n  {} requests in {:?}\n  {:.0} req/s\n  {:.1} µs/op",
        PAYLOAD_SIZE, ITERATIONS, elapsed, rps, us_per_op
    );
}

/// Throughput under concurrent load (20 concurrent senders).
#[tokio::test]
#[ignore]
async fn bench_grid_concurrent() {
    const TOTAL_REQUESTS: usize = 5_000;
    const CONCURRENCY: usize = 20;
    const PAYLOAD_SIZE: usize = 512;

    let (conn, _) = bench_pair().await;
    let conn = Arc::new(conn);
    let payload = vec![0xBBu8; PAYLOAD_SIZE];

    // Warmup.
    for _ in 0..100 {
        conn.request(HANDLER_ECHO, Some(payload.clone()), None)
            .await
            .unwrap();
    }

    let per_task = TOTAL_REQUESTS / CONCURRENCY;
    let start = Instant::now();

    let mut handles = vec![];
    for _ in 0..CONCURRENCY {
        let c = conn.clone();
        let p = payload.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..per_task {
                c.request(HANDLER_ECHO, Some(p.clone()), None)
                    .await
                    .unwrap();
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let elapsed = start.elapsed();
    let rps = TOTAL_REQUESTS as f64 / elapsed.as_secs_f64();

    println!(
        "\nbench_grid_concurrent ({}B, {} tasks):\n  {} requests in {:?}\n  {:.0} req/s",
        PAYLOAD_SIZE, CONCURRENCY, TOTAL_REQUESTS, elapsed, rps
    );
}

/// Large payload throughput (1 MiB).
#[tokio::test]
#[ignore]
async fn bench_grid_large_payload() {
    const ITERATIONS: usize = 100;
    const PAYLOAD_SIZE: usize = 1024 * 1024; // 1 MiB

    let (conn, _) = bench_pair().await;
    let payload = vec![0xCCu8; PAYLOAD_SIZE];

    // Warmup.
    for _ in 0..5 {
        conn.request(HANDLER_ECHO, Some(payload.clone()), None)
            .await
            .unwrap();
    }

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        conn.request(HANDLER_ECHO, Some(payload.clone()), None)
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();

    let mib_per_sec = (ITERATIONS * PAYLOAD_SIZE) as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64();

    println!(
        "\nbench_grid_large_payload (1 MiB):\n  {} requests in {:?}\n  {:.1} MiB/s",
        ITERATIONS, elapsed, mib_per_sec
    );
}

/// Stream benchmarks — require streaming support (Phase 2 future).
#[test]
#[ignore]
fn bench_grid_stream_responses() {
    // TODO: implement when streaming available.
    // Logic: NewStream -> receive 10 responses, measure throughput.
}

#[test]
#[ignore]
fn bench_grid_stream_requests() {
    // TODO: implement when streaming available.
    // Logic: NewStream -> send 10 requests -> verify latency.
}

#[test]
#[ignore]
fn bench_grid_stream_twoway() {
    // TODO: implement when streaming available.
    // Logic: NewStream -> send 10 + receive 10, measure bidirectional throughput.
}
