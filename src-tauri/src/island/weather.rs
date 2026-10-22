//! Current weather for the resting pill, from Open-Meteo (https://open-meteo.com): free, no account and no key.
//! The only thing sent is the city the user typed in settings; the place it finds is remembered until the city changes.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri_plugin_http::reqwest;

const GEOCODE: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";

/// A place found from a city name.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// What the pill shows.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub city: String,
    /// Whole degrees Celsius.
    pub temp: i32,
    /// A glyph name from the Island's icon set.
    pub icon: &'static str,
    /// "Light rain", "Clear" and so on.
    pub label: &'static str,
}

#[derive(Deserialize)]
struct Geocode {
    #[serde(default)]
    results: Vec<GeoHit>,
}

#[derive(Deserialize)]
struct GeoHit {
    name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct Forecast {
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    weather_code: u8,
    #[serde(default)]
    is_day: u8,
}

pub fn parse_place(json: &str) -> Option<Place> {
    let hit = serde_json::from_str::<Geocode>(json).ok()?.results.into_iter().next()?;
    (hit.latitude.is_finite() && hit.longitude.is_finite() && (-90.0..=90.0).contains(&hit.latitude) && (-180.0..=180.0).contains(&hit.longitude))
        .then_some(Place { name: hit.name, lat: hit.latitude, lon: hit.longitude })
}

/// WMO weather code to an icon and a short name.
pub fn describe(code: u8, day: bool) -> (&'static str, &'static str) {
    match code {
        0 => (if day { "sun" } else { "moon" }, "Clear"),
        1 | 2 => (if day { "sun" } else { "moon" }, "Mostly clear"),
        3 => ("cloud", "Cloudy"),
        45 | 48 => ("cloud", "Fog"),
        51..=57 => ("rain", "Drizzle"),
        61..=65 | 80..=82 => ("rain", "Rain"),
        66 | 67 => ("rain", "Freezing rain"),
        71..=77 | 85 | 86 => ("snow", "Snow"),
        95..=99 => ("storm", "Thunderstorm"),
        _ => ("cloud", "Cloudy"),
    }
}

pub fn parse_forecast(city: &str, json: &str) -> Option<Weather> {
    let c = serde_json::from_str::<Forecast>(json).ok()?.current;
    if !c.temperature_2m.is_finite() {
        return None;
    }
    let (icon, label) = describe(c.weather_code, c.is_day != 0);
    Some(Weather { city: city.to_string(), temp: c.temperature_2m.round() as i32, icon, label })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().user_agent(concat!("Notifi/", env!("CARGO_PKG_VERSION"))).timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())
}

async fn get(client: &reqwest::Client, url: &str, query: &[(&str, String)]) -> Result<String, String> {
    let response = client.get(url).query(query).send().await.map_err(|e| format!("Could not reach Open-Meteo: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Open-Meteo answered {}", response.status().as_u16()));
    }
    response.text().await.map_err(|e| format!("Could not read Open-Meteo's reply: {e}"))
}

/// Finds the place for a city name.
pub async fn find_place(city: &str) -> Result<Place, String> {
    let json = get(&client()?, GEOCODE, &[("name", city.to_string()), ("count", "1".into()), ("language", "en".into()), ("format", "json".into())]).await?;
    parse_place(&json).ok_or_else(|| format!("No place called \"{city}\" was found"))
}

/// The weather now at a place.
pub async fn current(place: &Place) -> Result<Weather, String> {
    let json = get(
        &client()?,
        FORECAST,
        &[("latitude", place.lat.to_string()), ("longitude", place.lon.to_string()), ("current", "temperature_2m,weather_code,is_day".into()), ("timezone", "auto".into())],
    )
    .await?;
    parse_forecast(&place.name, &json).ok_or_else(|| "Open-Meteo's reply had no weather in it".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_place_open_meteo_returns() {
        let json = r#"{"results":[{"id":2643123,"name":"Manchester","latitude":53.48095,"longitude":-2.23743,"country":"United Kingdom"}],"generationtime_ms":0.1}"#;
        assert_eq!(parse_place(json), Some(Place { name: "Manchester".into(), lat: 53.48095, lon: -2.23743 }));
        assert_eq!(parse_place(r#"{"generationtime_ms":0.12}"#), None);
        assert_eq!(parse_place(r#"{"results":[{"name":"X","latitude":123.0,"longitude":0.0}]}"#), None);
        assert_eq!(parse_place("not json"), None);
    }

    #[test]
    fn reads_the_weather_open_meteo_returns() {
        let json = r#"{"latitude":53.47,"current_units":{"time":"iso8601"},"current":{"time":"2026-10-22T09:00","interval":900,"temperature_2m":7.1,"weather_code":63,"is_day":1}}"#;
        let w = parse_forecast("Manchester", json).unwrap();
        assert_eq!((w.temp, w.icon, w.label), (7, "rain", "Rain"));
        assert_eq!(parse_forecast("X", r#"{"current":{"temperature_2m":-0.6,"weather_code":0,"is_day":0}}"#).unwrap().temp, -1);
        assert_eq!(parse_forecast("X", r#"{"current":{"temperature_2m":0.0,"weather_code":0,"is_day":0}}"#).unwrap().icon, "moon");
        assert!(parse_forecast("X", "{}").is_none());
    }

    #[test]
    fn codes_map_to_icons() {
        assert_eq!(describe(0, true), ("sun", "Clear"));
        assert_eq!(describe(3, false).0, "cloud");
        assert_eq!(describe(81, true).0, "rain");
        assert_eq!(describe(73, true).0, "snow");
        assert_eq!(describe(96, true).0, "storm");
        assert_eq!(describe(200, true).0, "cloud");
    }
}
