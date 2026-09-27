//! The authored diagrams of the guide. Each module owns one semantic seed and its composition.

pub(crate) mod call_timeline;
pub(crate) mod crate_map;
pub(crate) mod processing_flow;

use crate::model::Diagram;

/// Every diagram, in the order the guide shows them.
pub(crate) const ALL: [&Diagram; 3] = [&crate_map::DIAGRAM, &processing_flow::DIAGRAM, &call_timeline::DIAGRAM];
