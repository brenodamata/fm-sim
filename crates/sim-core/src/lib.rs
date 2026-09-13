//! Simulation core.
//!
//! Pure: no I/O, no logging, no clock, no global state. Everything stochastic
//! traces back to an explicit seed (see [`determinism`]). This is what lets the
//! harness run thousands of headless seasons and lets any bug report be
//! reproduced from a seed and a snapshot.
//!
//! The engine deals in truth. It has no concept of what a manager knows — the
//! knowledge filter (ADR-010) lives in the application service above this crate,
//! which is the single point where true values are either contained or leaked.

pub mod data;
pub mod determinism;
pub mod domain;
pub mod gen;
pub mod tuning;

pub use data::Dataset;
pub use determinism::{Domain, Seed, SeedPath, WorldSeed, RULESET_VERSION};
pub use domain::world::World;
pub use gen::{generate, generate_from_dataset, WorldSpec};
pub use tuning::Tuning;
