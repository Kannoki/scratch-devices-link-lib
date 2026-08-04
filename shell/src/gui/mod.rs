//! GUI module for Future Academy Link
//!
//! This module provides a cross-platform GUI using eframe/egui.

pub mod app;
pub mod components;
pub mod screens;
pub mod styles;

pub use app::{AppState, FutureAcademyApp, SharedState};
