//! # vitals-store
//!
//! Local time-series persistence.
//!
//! **Off by default.** A tool that silently records everything your machine
//! does, forever, is not one people should have to trust blindly. Retention
//! is explicit, the file is local, and nothing leaves the machine.
//!
//! Storage strategy is tiered, because raw 1 Hz samples for every process
//! would be gigabytes per week:
//!
//! | Age        | Resolution | Scope                     |
//! |------------|-----------|---------------------------|
//! | < 1 hour   | 1 s       | full, every process       |
//! | < 24 hours | 1 min     | rolled up, top N by usage |
//! | < 30 days  | 5 min     | machine-wide totals only  |
//! | older      | 1 hour    | machine-wide totals only  |
//!
//! Rollups are computed on write, not on read, so opening a month-long chart
//! is a single indexed scan rather than an aggregation over millions of rows.

#![allow(clippy::missing_const_for_fn)]

pub mod retention;

pub use retention::{Resolution, RetentionPolicy};
