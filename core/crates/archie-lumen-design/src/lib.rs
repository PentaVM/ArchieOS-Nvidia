//! XaviCottier's delivered Lumenfield design, ported from the 2026-10-18 handoff.
//! Presentation only: no observation, inference or authority APIs. The one filesystem
//! access is `content`: a bounded, read-only listing of the folder the content sphere
//! draws, as Xavi's desktop did. Design: Xavi; native implementation: PentaVM.
//! Source: codigo fuente/src/{field,hybrid-materials,quality-modes}.js and visual-presets/.

pub mod catalog;
pub mod content;
mod ffi;
pub mod mesh;
pub mod neural;
pub mod policy;
pub mod shell;
pub mod simulation;
pub mod surface;

pub use catalog::*;
pub use ffi::*;
pub use simulation::{DesignBody, DesignMotion, DesignVec3};

#[cfg(test)]
mod tests;
