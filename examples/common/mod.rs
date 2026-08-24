//! Shared presentation support for Aqua examples.

pub mod capture;

use bevy::{
    post_process::bloom::Bloom,
    prelude::*,
    window::{ExitCondition, PresentMode, WindowResolution},
};

/// Configures the normal interactive window or disables the primary window.
pub fn window_plugin(
    headless: bool,
    title: &'static str,
    size: UVec2,
    present_mode: PresentMode,
) -> WindowPlugin {
    WindowPlugin {
        primary_window: (!headless).then(|| Window {
            title: title.into(),
            resolution: WindowResolution::new(size.x, size.y),
            present_mode,
            ..default()
        }),
        exit_condition: if headless {
            ExitCondition::DontExit
        } else {
            ExitCondition::OnPrimaryClosed
        },
        ..default()
    }
}

/// Modest energy-conserving bloom for HDR water highlights.
pub fn beauty_bloom() -> Bloom {
    Bloom {
        intensity: 0.08,
        low_frequency_boost: 0.5,
        ..Bloom::NATURAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beauty_bloom_is_modest_and_energy_conserving() {
        let bloom = beauty_bloom();
        assert_eq!(bloom.intensity, 0.08);
        assert_eq!(bloom.low_frequency_boost, 0.5);
        assert!(matches!(
            bloom.composite_mode,
            bevy::post_process::bloom::BloomCompositeMode::EnergyConserving
        ));
        assert_eq!(bloom.prefilter.threshold, 0.0);
    }

    #[test]
    fn headless_window_has_no_primary_surface() {
        let plugin = window_plugin(true, "test", UVec2::new(1280, 720), PresentMode::AutoVsync);
        assert!(plugin.primary_window.is_none());
        assert!(matches!(plugin.exit_condition, ExitCondition::DontExit));
    }
}
