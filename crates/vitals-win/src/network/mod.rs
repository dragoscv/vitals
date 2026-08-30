//! Network interface enumeration and throughput sampling.

pub mod adapters;
pub mod rate;

pub use adapters::{AdapterInfo, enumerate_adapters};
pub use rate::{NetworkCounters, NetworkRates, compute_rates};
