//! The decisions the sound features make, kept free of the audio backend so they can be tested.

use notifi_platform::DeviceKind;

pub fn perceived_to_gain(perceived: f32) -> f32 {
    let p = perceived.max(0.0);
    if p <= 1.0 { p * p } else { p }
}

pub fn gain_to_perceived(gain: f32) -> f32 {
    let g = gain.max(0.0);
    if g <= 1.0 { g.sqrt() } else { g }
}

pub fn is_headphones(kind: DeviceKind) -> bool {
    matches!(kind, DeviceKind::Headphones | DeviceKind::Headset)
}

/// Whether unplugging `removed` should lower the volume of whatever is now the default output.
/// Nothing happens when the new default is headphones too, or the removed device was not headphones.
pub fn lower_on_disconnect(removed: DeviceKind, new_default: Option<DeviceKind>) -> bool {
    is_headphones(removed) && !new_default.is_some_and(is_headphones)
}

const VIRTUAL_MARKERS: &[&str] = &["virtual", "voicemeeter", "vb-audio", "cable ", "steam streaming", "remote audio", "loopback"];

pub fn is_virtual(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIRTUAL_MARKERS.iter().any(|m| lower.contains(m))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub name: String,
}

/// The highest-ranked connected device. Ranking entries match a device name or id, ignoring case;
/// an empty ranking never picks anything.
pub fn choose_by_priority(ranking: &[String], devices: &[Candidate], ignore_virtual: bool) -> Option<String> {
    ranking.iter().find_map(|entry| {
        let want = entry.trim().to_lowercase();
        if want.is_empty() {
            return None;
        }
        devices
            .iter()
            .filter(|d| !(ignore_virtual && is_virtual(&d.name)))
            .find(|d| d.name.to_lowercase() == want || d.id.to_lowercase() == want || d.name.to_lowercase().contains(&want))
            .map(|d| d.id.clone())
    })
}

/// The device after `current` in `ids`, wrapping round; the first one when `current` is not in the list.
pub fn next_in_cycle(ids: &[String], current: Option<&str>) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let at = current.and_then(|c| ids.iter().position(|i| i == c));
    Some(ids[at.map_or(0, |i| (i + 1) % ids.len())].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, name: &str) -> Candidate {
        Candidate { id: id.into(), name: name.into() }
    }

    #[test]
    fn loudness_curve_round_trips_and_is_continuous() {
        assert_eq!(perceived_to_gain(1.0), 1.0);
        assert!((perceived_to_gain(0.5) - 0.25).abs() < 1e-6);
        assert_eq!(perceived_to_gain(1.5), 1.5);
        for g in [0.0, 0.04, 0.25, 0.9, 1.0, 1.7, 2.0] {
            assert!((perceived_to_gain(gain_to_perceived(g)) - g).abs() < 1e-5, "{g}");
        }
    }

    #[test]
    fn disconnect_rule() {
        assert!(lower_on_disconnect(DeviceKind::Headphones, Some(DeviceKind::Speakers)));
        assert!(lower_on_disconnect(DeviceKind::Headset, None));
        assert!(!lower_on_disconnect(DeviceKind::Headphones, Some(DeviceKind::Headset)));
        assert!(!lower_on_disconnect(DeviceKind::Speakers, Some(DeviceKind::Display)));
    }

    #[test]
    fn priority_picks_first_connected_ranked_device() {
        let ranking = vec!["Headset".to_string(), "Speakers".to_string(), "TV".to_string()];
        let connected = [dev("a", "Speakers (Realtek)"), dev("b", "TV")];
        assert_eq!(choose_by_priority(&ranking, &connected, true), Some("a".into()));
        let with_headset = [dev("a", "Speakers (Realtek)"), dev("c", "Headset (USB)")];
        assert_eq!(choose_by_priority(&ranking, &with_headset, true), Some("c".into()));
        assert_eq!(choose_by_priority(&[], &connected, true), None);
        assert_eq!(choose_by_priority(&["Nothing".to_string()], &connected, true), None);
    }

    #[test]
    fn priority_skips_virtual_devices() {
        let ranking = vec!["VoiceMeeter Input".to_string(), "Speakers".to_string()];
        let devices = [dev("v", "VoiceMeeter Input"), dev("s", "Speakers")];
        assert_eq!(choose_by_priority(&ranking, &devices, true), Some("s".into()));
        assert_eq!(choose_by_priority(&ranking, &devices, false), Some("v".into()));
    }

    #[test]
    fn cycle_wraps() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(next_in_cycle(&ids, Some("a")), Some("b".into()));
        assert_eq!(next_in_cycle(&ids, Some("c")), Some("a".into()));
        assert_eq!(next_in_cycle(&ids, Some("zzz")), Some("a".into()));
        assert_eq!(next_in_cycle(&[], Some("a")), None);
    }
}
