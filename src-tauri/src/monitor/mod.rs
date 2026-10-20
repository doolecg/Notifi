//! System monitor parts used by the sampler in `stats.rs`: graphs, memory pressure, GPUs,
//! per-app use, tray readouts, alerts and the speed test.

pub mod alerts;
pub mod devices;
pub mod gpu;
pub mod history;
pub mod memory;
pub mod notify;
#[cfg(windows)]
pub mod pdh;
pub mod procs;
pub mod readouts;
pub mod sensors;
pub mod speedtest;
pub mod units;
