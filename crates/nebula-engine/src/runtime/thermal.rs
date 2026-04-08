//! Thermal monitoring and protective shutdown for NEBULA nodes.

/// Thermal state of the device, ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThermalState {
    Normal,
    Light,
    Moderate,
    Severe,
    Critical,
    Emergency,
}

impl ThermalState {
    pub fn from_status_code(code: u8) -> Self {
        match code {
            0 => Self::Normal,
            1 => Self::Light,
            2 => Self::Moderate,
            3 => Self::Severe,
            4 => Self::Critical,
            _ => Self::Emergency,
        }
    }
}

/// Actions the engine can take in response to thermal conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalAction {
    None,
    ReduceHeartbeat,
    PausePlugins,
    VoluntaryDemotion,
    ProtectiveShutdown,
}

/// Tracks device thermal state and recommends workload adjustments.
#[derive(Debug)]
pub struct ThermalManager {
    current_state: ThermalState,
}

impl ThermalManager {
    pub fn new() -> Self {
        Self {
            current_state: ThermalState::Normal,
        }
    }
    pub fn current_state(&self) -> ThermalState {
        self.current_state
    }

    pub fn evaluate(
        &mut self,
        battery: u8,
        cpu_temp: Option<f32>,
        thermal_status: u8,
    ) -> ThermalAction {
        let reported = ThermalState::from_status_code(thermal_status);
        let temp_state = match cpu_temp {
            Some(t) if t >= 90.0 => ThermalState::Emergency,
            Some(t) if t >= 80.0 => ThermalState::Critical,
            Some(t) if t >= 70.0 => ThermalState::Severe,
            Some(t) if t >= 60.0 => ThermalState::Moderate,
            Some(t) if t >= 50.0 => ThermalState::Light,
            _ => ThermalState::Normal,
        };
        let mut effective = reported.max(temp_state);
        if battery <= 10 && effective >= ThermalState::Moderate {
            effective = effective.max(ThermalState::Critical);
        } else if battery <= 20 && effective >= ThermalState::Severe {
            effective = effective.max(ThermalState::Critical);
        }
        self.current_state = effective;
        match effective {
            ThermalState::Normal | ThermalState::Light => ThermalAction::None,
            ThermalState::Moderate => ThermalAction::ReduceHeartbeat,
            ThermalState::Severe => ThermalAction::PausePlugins,
            ThermalState::Critical => ThermalAction::VoluntaryDemotion,
            ThermalState::Emergency => ThermalAction::ProtectiveShutdown,
        }
    }
}

impl Default for ThermalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_status_code() {
        assert_eq!(ThermalState::from_status_code(0), ThermalState::Normal);
        assert_eq!(ThermalState::from_status_code(5), ThermalState::Emergency);
    }
    #[test]
    fn test_normal() {
        assert_eq!(
            ThermalManager::new().evaluate(100, Some(40.0), 0),
            ThermalAction::None
        );
    }
    #[test]
    fn test_moderate() {
        assert_eq!(
            ThermalManager::new().evaluate(80, None, 2),
            ThermalAction::ReduceHeartbeat
        );
    }
    #[test]
    fn test_severe() {
        assert_eq!(
            ThermalManager::new().evaluate(60, None, 3),
            ThermalAction::PausePlugins
        );
    }
    #[test]
    fn test_critical() {
        assert_eq!(
            ThermalManager::new().evaluate(50, None, 4),
            ThermalAction::VoluntaryDemotion
        );
    }
    #[test]
    fn test_emergency() {
        assert_eq!(
            ThermalManager::new().evaluate(30, None, 5),
            ThermalAction::ProtectiveShutdown
        );
    }
    #[test]
    fn test_high_cpu_temp() {
        let mut m = ThermalManager::new();
        assert_eq!(m.evaluate(80, Some(75.0), 0), ThermalAction::PausePlugins);
        assert_eq!(m.current_state(), ThermalState::Severe);
    }
    #[test]
    fn test_extreme_cpu() {
        assert_eq!(
            ThermalManager::new().evaluate(50, Some(95.0), 0),
            ThermalAction::ProtectiveShutdown
        );
    }
    #[test]
    fn test_low_bat_escalates() {
        let mut m = ThermalManager::new();
        assert_eq!(m.evaluate(8, None, 2), ThermalAction::VoluntaryDemotion);
        assert_eq!(m.current_state(), ThermalState::Critical);
    }
    #[test]
    fn test_ordering() {
        assert!(ThermalState::Normal < ThermalState::Emergency);
    }
}
