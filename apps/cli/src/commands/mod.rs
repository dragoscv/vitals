//! One module per subcommand. Each takes the resolved [`Source`] and the
//! `--json` flag and writes to stdout; everything diagnostic goes to stderr
//! so `--json` stays pipeable.

pub mod act;
pub mod alerts;
pub mod eco;
pub mod history;
pub mod info;
pub mod mcp;
pub mod ps;
pub mod report;
pub mod sensors;
pub mod serve;
pub mod top;
pub mod widget;
