// [[A1]] rustRete
//! `ruggine-bench <n> <runs>`: prints the native report as JSON.

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let n = args.next().map_or(1000, |a| a.parse().expect("n"));
    let runs = args.next().map_or(5, |a| a.parse().expect("runs"));
    let start = Instant::now();
    let now = || start.elapsed().as_secs_f64() * 1000.0;
    println!("{}", ruggine_bench::report(n, runs, &now));
}
