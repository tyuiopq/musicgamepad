//! Калибровка геймпада: у старых и дешёвых стиков центр «уплывает», а курки и стики
//! не добирают до края. Поправка своя у каждого устройства и применяется до раскладки.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::Axis;

/// Что намерили у одной оси (в сырых значениях −1..1, курки 0..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisCal {
    /// Где ось стоит в покое (у курков — отпущенный курок).
    pub center: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {
    pub axes: HashMap<Axis, AxisCal>,
}

/// Меньше этого хода оси не верим замеру (ось не трогали) и не растягиваем.
const MIN_TRAVEL: f32 = 0.2;

fn is_trigger(axis: Axis) -> bool {
    matches!(axis, Axis::LeftTrigger | Axis::RightTrigger)
}

impl Calibration {
    pub fn apply(&self, axis: Axis, v: f32) -> f32 {
        let Some(c) = self.axes.get(&axis) else { return v };
        if is_trigger(axis) {
            let span = c.max - c.center;
            if span < MIN_TRAVEL {
                return v;
            }
            return ((v - c.center) / span).clamp(0.0, 1.0);
        }
        let d = v - c.center;
        let span = if d >= 0.0 { c.max - c.center } else { c.center - c.min };
        let span = if span < MIN_TRAVEL { 1.0 } else { span };
        (d / span).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recenters_and_stretches() {
        let mut cal = Calibration::default();
        cal.axes.insert(Axis::LeftX, AxisCal { center: 0.08, min: -0.9, max: 0.88 });
        cal.axes.insert(Axis::RightTrigger, AxisCal { center: 0.05, min: 0.05, max: 0.85 });
        assert_eq!(cal.apply(Axis::LeftX, 0.08), 0.0, "уплывший центр — снова ноль");
        assert!((cal.apply(Axis::LeftX, 0.88) - 1.0).abs() < 1e-6, "недобор до края растянут");
        assert!((cal.apply(Axis::LeftX, -0.9) + 1.0).abs() < 1e-6);
        assert_eq!(cal.apply(Axis::RightTrigger, 0.05), 0.0);
        assert_eq!(cal.apply(Axis::RightTrigger, 0.9), 1.0);
        assert_eq!(cal.apply(Axis::RightY, 0.3), 0.3, "без замера ось не трогаем");
    }

    #[test]
    fn ignores_untouched_axes() {
        let mut cal = Calibration::default();
        cal.axes.insert(Axis::LeftY, AxisCal { center: 0.0, min: -0.02, max: 0.03 });
        assert_eq!(cal.apply(Axis::LeftY, 0.5), 0.5);
    }
}
