//! One module per subcommand. Each takes the resolved [`Source`] and the
//! `--json` flag and writes to stdout; everything diagnostic goes to stderr
//! so `--json` stays pipeable.

pub mod eco;
pub mod info;
pub mod ps;
pub mod report;
pub mod serve;
pub mod top;
