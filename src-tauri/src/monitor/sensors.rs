//! Choosing one CPU temperature out of the labelled readings the system gives.

const PACKAGE_LABELS: [&str; 3] = ["package", "tctl", "tdie"];

/// The package sensor's value, or the hottest reading when `hottest` is set or no package sensor exists.
pub fn cpu_temperature(readings: &[(String, f32)], hottest: bool) -> Option<f32> {
    let max = |it: &mut dyn Iterator<Item = f32>| it.reduce(f32::max);
    if !hottest {
        let package = max(&mut readings.iter().filter(|(l, _)| PACKAGE_LABELS.iter().any(|k| l.contains(k))).map(|(_, c)| *c));
        if package.is_some() {
            return package;
        }
    }
    max(&mut readings.iter().map(|(_, c)| *c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(label: &str, c: f32) -> (String, f32) {
        (label.to_string(), c)
    }

    #[test]
    fn package_by_default_hottest_on_request() {
        let readings = [r("cpu package", 60.0), r("core 1", 72.0), r("core 2", 65.0)];
        assert_eq!(cpu_temperature(&readings, false), Some(60.0));
        assert_eq!(cpu_temperature(&readings, true), Some(72.0));
    }

    #[test]
    fn falls_back_to_the_hottest_without_a_package_sensor() {
        let readings = [r("core 1", 50.0), r("core 2", 58.0)];
        assert_eq!(cpu_temperature(&readings, false), Some(58.0));
        assert_eq!(cpu_temperature(&[], false), None);
    }
}
