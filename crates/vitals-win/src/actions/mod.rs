//! Actions that change the system, as opposed to observing it.
//!
//! Every destructive action is split into a **plan** and an **execute**
//! phase. The plan states what will happen and how dangerous it is, so the UI
//! can say something specific and true before committing. A single generic
//! "are you sure?" is what trains people to click through warnings — which is
//! precisely how someone ends up bugchecking their machine from a process
//! list.

pub mod process;
pub mod safety;

pub use process::{
    ActionPlan, Priority, efficiency_mode, plan_suspend, plan_terminate, resume, set_affinity,
    set_efficiency_mode, set_priority, suspend, terminate,
};
pub use safety::{ProcessFacts, Risk, assess_suspension, assess_termination, consequence_key};
