#![warn(unreachable_pub)]

//! Crest-style concentric ocean ring geometry for Aqua.
//!
//! The ring count and tile resolution also define Aqua's cascade ABI.

pub mod rings;

/// Number of camera-centred cascade rings.
pub const LOD_COUNT: usize = 5;
/// Ocean tile grid resolution (vertices per edge minus one).
pub const TILE_RESOLUTION: u32 = 64;
