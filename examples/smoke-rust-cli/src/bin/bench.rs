use std::time::Instant;

use smoke_rust_cli::{demo_catalog, suggest_commands};

const SAMPLES: usize = 15;

fn percentile_95(samples: &mut [f64]) -> f64 {
    samples.sort_by(|left, right| left.partial_cmp(right).unwrap());
    let index = ((samples.len() - 1) as f64 * 0.95).round() as usize;
    samples[index]
}

fn measure_once() -> f64 {
    let catalog = demo_catalog(320);
    let queries = [
        "cache memoization performance",
        "bench latency metrics",
        "workspace diagnostics",
        "log triage errors",
        "reproducible benchmark setup",
        "performance cache latency",
    ];

    let started = Instant::now();
    for _ in 0..6 {
        for query in queries {
            let _ = suggest_commands(&catalog, query, 5);
        }
    }
    started.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let mut samples = (0..SAMPLES).map(|_| measure_once()).collect::<Vec<_>>();
    println!("METRIC latency_p95={:.3}", percentile_95(&mut samples));
}
