//! Downloadification — automatic Downloads folder organizer.
//!
//! The crate is organised as a small library so that the command line
//! (`downloadification`) and the graphical control panel
//! (`downloadification-gui`) share exactly the same logic.

#![forbid(unsafe_code)]

pub mod categories;
pub mod config;
pub mod logger;
pub mod organizer;
pub mod paths;
pub mod service;
pub mod watcher;

pub use config::Config;
pub use organizer::{OrganizeReport, Organizer};
