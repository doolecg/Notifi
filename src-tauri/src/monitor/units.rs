//! Number formatting for the tray readouts and alert messages. Units roll over at 1000
//! (decimal), so "999K" becomes "1.0M" once it would round to 1000.

const COMPACT: [&str; 4] = ["K", "M", "G", "T"];

/// Scales `n` by 1000 until it reads below 1000 once rounded to what we print.
fn scale(mut n: f64, steps: usize) -> (f64, usize) {
    let mut i = 0;
    while i + 1 < steps && rounded(n) >= 1000.0 {
        n /= 1000.0;
        i += 1;
    }
    (n, i)
}

fn rounded(n: f64) -> f64 {
    if n < 10.0 {
        (n * 10.0).round() / 10.0
    } else {
        n.round()
    }
}

fn number(n: f64) -> String {
    if rounded(n) < 10.0 {
        format!("{n:.1}")
    } else {
        format!("{n:.0}")
    }
}

/// At most four characters for the tray: "0.5K", "12M", "450K", "1.0G". Never below K.
pub fn compact_rate(bytes_per_sec: f64, bits: bool) -> String {
    let n = bytes_per_sec.max(0.0) * if bits { 8.0 } else { 1.0 } / 1000.0;
    let (n, i) = scale(n, COMPACT.len());
    format!("{}{}", number(n), COMPACT[i])
}

/// Size for the four-character tray slot: "512G", "1.5T".
pub fn compact_size(bytes: u64) -> String {
    let (n, i) = scale(bytes as f64 / 1000.0, COMPACT.len());
    format!("{}{}", number(n), COMPACT[i])
}

pub fn to_unit(celsius: f32, fahrenheit: bool) -> f32 {
    if fahrenheit {
        celsius * 9.0 / 5.0 + 32.0
    } else {
        celsius
    }
}

pub fn temperature(celsius: f32, fahrenheit: bool) -> String {
    format!("{:.0}°{}", to_unit(celsius, fahrenheit), if fahrenheit { 'F' } else { 'C' })
}

/// "2:05" for hours and minutes left.
pub fn hours_minutes(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 3600, (seconds % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_roll_over_at_the_boundary() {
        assert_eq!(compact_rate(999_400.0, false), "999K");
        assert_eq!(compact_rate(999_600.0, false), "1.0M");
        assert_eq!(compact_rate(9_960.0, false), "10K");
        assert_eq!(compact_rate(0.0, false), "0.0K");
    }

    #[test]
    fn compact_rates_fit_four_characters() {
        for n in [0.0, 512.0, 9_999.0, 99_999.0, 450_000.0, 999_999.0, 12e6, 999e6, 5e9, 9e12] {
            let s = compact_rate(n, false);
            assert!(s.chars().count() <= 4, "{n} -> {s}");
        }
        assert_eq!(compact_rate(512.0, false), "0.5K");
        assert_eq!(compact_rate(450_000.0, false), "450K");
        assert_eq!(compact_rate(999_999.0, false), "1.0M");
        assert_eq!(compact_rate(1_250_000.0, true), "10M");
    }

    #[test]
    fn sizes_and_temperatures() {
        assert_eq!(compact_size(1_500_000_000_000), "1.5T");
        assert_eq!(compact_size(256_000_000_000), "256G");
        assert_eq!(temperature(100.0, true), "212°F");
        assert_eq!(temperature(45.4, false), "45°C");
        assert_eq!(hours_minutes(7_500), "2:05");
    }
}
