//! Budgeted GPU spray driven by Aqua's existing surface signals.

use bevy::prelude::*;
use bevy_hanabi::HanabiPlugin;

mod effect;
mod runtime;

pub(crate) const MAX_PROBES: usize = 128;
pub(crate) const MAX_EMITTERS: usize = 24;

/// Runtime spray quality. [`Self::Off`] incurs no wave-query or particle work.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SprayQuality {
    #[default]
    Off,
    Low,
    High,
}

/// Bounded spray controls. Insert before [`AquaSprayPlugin`] to override.
#[derive(Resource, Debug, Clone)]
pub struct SpraySettings {
    pub quality: SprayQuality,
    /// Minimum breaking-crest compression source that can emit spray.
    pub crest_threshold: f32,
}

impl Default for SpraySettings {
    fn default() -> Self {
        Self {
            quality: SprayQuality::Off,
            crest_threshold: 0.06,
        }
    }
}

/// Adds bounded Hanabi spray sourced from Aqua wave probes and bed depth.
#[derive(Debug, Default, Clone, Copy)]
pub struct AquaSprayPlugin;

impl Plugin for AquaSprayPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<HanabiPlugin>() {
            app.add_plugins(HanabiPlugin);
        }
        app.init_resource::<SpraySettings>()
            .init_resource::<Budget>()
            .add_systems(Startup, effect::setup)
            .add_systems(
                Update,
                (
                    runtime::configure_quality,
                    runtime::place_probes,
                    runtime::emit_spray,
                )
                    .chain(),
            );
    }
}

#[derive(Component, Debug)]
pub(crate) struct Probe {
    pub(crate) index: usize,
    pub(crate) cooldown: f32,
}

#[derive(Component, Debug)]
pub(crate) struct Emitter;

#[derive(Resource, Debug)]
pub(crate) struct Budget {
    pub(crate) quality: SprayQuality,
    pub(crate) tokens: f32,
    pub(crate) emitter_cursor: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            quality: SprayQuality::Off,
            tokens: 0.0,
            emitter_cursor: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) probes: usize,
    pub(crate) distance: f32,
    pub(crate) particles_per_second: f32,
    pub(crate) burst: u32,
    pub(crate) coverage: f32,
    pub(crate) columns: usize,
    pub(crate) spacing: f32,
}

pub(crate) fn limits(quality: SprayQuality) -> Limits {
    match quality {
        SprayQuality::Off => Limits {
            probes: 0,
            distance: 0.0,
            particles_per_second: 0.0,
            burst: 0,
            coverage: 0.0,
            columns: 1,
            spacing: 1.0,
        },
        SprayQuality::Low => Limits {
            probes: 48,
            distance: 35.0,
            particles_per_second: 32.0,
            burst: 8,
            coverage: 0.005,
            columns: 8,
            spacing: 4.0,
        },
        SprayQuality::High => Limits {
            probes: MAX_PROBES,
            distance: 80.0,
            particles_per_second: 160.0,
            burst: 20,
            coverage: 0.02,
            columns: 16,
            spacing: 5.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_limits_stay_bounded() {
        let off = limits(SprayQuality::Off);
        let low = limits(SprayQuality::Low);
        let high = limits(SprayQuality::High);
        assert_eq!(off.probes, 0);
        assert!(low.probes < high.probes);
        assert!(high.probes <= MAX_PROBES);
        assert!(high.burst <= 20);
        assert!(high.particles_per_second <= 160.0);
        assert!(high.coverage <= 0.02);
    }
}
