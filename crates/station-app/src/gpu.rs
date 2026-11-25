use crate::cli::GpuPreference;
use bevy::render::settings::PowerPreference;

pub fn wgpu_power_preference(pref: GpuPreference) -> PowerPreference {
    match pref {
        GpuPreference::Auto => PowerPreference::None,
        GpuPreference::Integrated => PowerPreference::LowPower,
        GpuPreference::Discrete => PowerPreference::HighPerformance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_preference_maps_to_wgpu_power_preference() {
        assert_eq!(wgpu_power_preference(GpuPreference::Auto), PowerPreference::None);
        assert_eq!(
            wgpu_power_preference(GpuPreference::Integrated),
            PowerPreference::LowPower
        );
        assert_eq!(
            wgpu_power_preference(GpuPreference::Discrete),
            PowerPreference::HighPerformance
        );
    }
}
