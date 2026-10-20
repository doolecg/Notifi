//! Scoop (Windows). `scoop export` gives JSON; `status` and `search` are tables cut by their rule line.

use std::time::Duration;

use serde_json::Value;

use super::{exec, packages::Pkg, table};

fn run(args: &[&str], secs: u64) -> Result<String, String> {
    exec::run("scoop", args, Duration::from_secs(secs)).map(|o| table::strip_ansi(&o.stdout))
}

pub fn parse_export(text: &str) -> Vec<Pkg> {
    let Ok(json) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else { return Vec::new() };
    json.get("apps")
        .and_then(Value::as_array)
        .map(|apps| {
            apps.iter()
                .filter_map(|a| {
                    let name = a.get("Name")?.as_str()?.to_string();
                    Some(Pkg {
                        manager: "scoop".into(),
                        id: name.clone(),
                        name,
                        version: a.get("Version").and_then(Value::as_str).unwrap_or_default().to_string(),
                        available: None,
                        origin: a.get("Source").and_then(Value::as_str).unwrap_or_default().to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_status(text: &str) -> Vec<Pkg> {
    let (header, rows) = table::by_rule(text);
    if header.len() < 3 || header[0] != "Name" {
        return Vec::new();
    }
    rows.into_iter()
        .filter(|r| !r[0].is_empty() && !r[2].is_empty())
        .map(|r| Pkg { manager: "scoop".into(), id: r[0].clone(), name: r[0].clone(), version: r[1].clone(), available: Some(r[2].clone()), origin: String::new() })
        .collect()
}

pub fn parse_search(text: &str) -> Vec<Pkg> {
    let (header, rows) = table::by_rule(text);
    if header.len() < 3 || header[0] != "Name" {
        return Vec::new();
    }
    let source = header.iter().position(|h| h == "Source").unwrap_or(2);
    rows.into_iter()
        .filter(|r| !r[0].is_empty())
        .map(|r| Pkg { manager: "scoop".into(), id: r[0].clone(), name: r[0].clone(), version: r[1].clone(), available: None, origin: r[source].clone() })
        .collect()
}

pub fn installed() -> Result<Vec<Pkg>, String> {
    Ok(parse_export(&run(&["export"], 60)?))
}

pub fn updates() -> Result<Vec<Pkg>, String> {
    Ok(parse_status(&run(&["status"], 180)?))
}

pub fn search(query: &str) -> Result<Vec<Pkg>, String> {
    Ok(parse_search(&run(&["search", query], 120)?))
}

pub fn install_args(id: &str) -> Vec<String> {
    vec!["install".into(), id.into()]
}

pub fn update_args(id: &str) -> Vec<String> {
    vec!["update".into(), id.into()]
}

pub fn remove_args(id: &str) -> Vec<String> {
    vec!["uninstall".into(), id.into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_json_with_bom() {
        let text = "\u{feff}{\"apps\":[{\"Info\":\"\",\"Source\":\"main\",\"Name\":\"git\",\"Updated\":\"2024-01-01\",\"Version\":\"2.45.0\"}],\"buckets\":[]}";
        let apps = parse_export(text);
        assert_eq!(apps.len(), 1);
        assert_eq!((apps[0].id.as_str(), apps[0].version.as_str(), apps[0].origin.as_str()), ("git", "2.45.0", "main"));
    }

    #[test]
    fn status_table() {
        let text = "Scoop is up to date.\n\nName Installed Version Latest Version Missing Dependencies Info\n---- ----------------- -------------- -------------------- ----\ngit  2.45.0            2.46.1                              \nnode 20.1.0            20.2.0         python               Held package\n\n";
        let rows = parse_status(text);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].version.as_str(), rows[1].available.as_deref()), ("20.1.0", Some("20.2.0")));
    }

    #[test]
    fn status_everything_ok() {
        assert!(parse_status("WARN  No updates.\nEverything is ok!\n").is_empty());
    }

    #[test]
    fn search_table() {
        let text = "Results from local buckets...\n\nName   Version Source Binaries\n----   ------- ------ --------\nripgrep 14.1.0 main\nfd      9.0.0  main\n";
        let rows = parse_search(text);
        assert_eq!(rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["ripgrep", "fd"]);
        assert_eq!(rows[0].origin, "main");
    }
}
