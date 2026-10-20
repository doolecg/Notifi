//! winget (Windows). It has no JSON output for listings, so its column-aligned tables are parsed
//! by header position.

use std::time::Duration;

use super::{exec, packages::Pkg, table};

const COMMON: &[&str] = &["--accept-source-agreements", "--disable-interactivity"];

fn run(args: &[&str]) -> Result<String, String> {
    let all: Vec<&str> = args.iter().chain(COMMON).copied().collect();
    exec::run("winget", &all, Duration::from_secs(180)).map(|o| o.stdout)
}

fn value(s: &str) -> String {
    s.trim_start_matches(['<', '>']).trim().to_string()
}

pub fn parse_list(text: &str) -> Vec<Pkg> {
    let mut out = Vec::new();
    for headers in [&["Name", "Id", "Version", "Available", "Source"][..], &["Name", "Id", "Version", "Source"][..]] {
        for row in table::by_headers(text, headers) {
            if out.iter().any(|p: &Pkg| p.id == row[1]) {
                continue;
            }
            let has_available = headers.len() == 5;
            let available = has_available.then(|| value(&row[3])).filter(|v| !v.is_empty());
            let origin = row.last().cloned().unwrap_or_default();
            out.push(Pkg {
                manager: "winget".into(),
                id: row[1].clone(),
                name: row[0].clone(),
                version: value(&row[2]),
                available,
                origin,
            });
        }
    }
    out
}

pub fn parse_search(text: &str) -> Vec<Pkg> {
    let rows = {
        let with_match = table::by_headers(text, &["Name", "Id", "Version", "Match", "Source"]);
        if with_match.is_empty() { table::by_headers(text, &["Name", "Id", "Version", "Source"]) } else { with_match }
    };
    rows.into_iter()
        .map(|r| Pkg {
            manager: "winget".into(),
            id: r[1].clone(),
            name: r[0].clone(),
            version: value(&r[2]),
            available: None,
            origin: r.last().cloned().unwrap_or_default(),
        })
        .collect()
}

pub fn installed() -> Result<Vec<Pkg>, String> {
    Ok(parse_list(&run(&["list"])?))
}

/// Upgradable packages, unknown installed versions included.
pub fn updates() -> Result<Vec<Pkg>, String> {
    Ok(parse_list(&run(&["upgrade", "--include-unknown"])?).into_iter().filter(|p| p.available.is_some()).collect())
}

pub fn search(query: &str) -> Result<Vec<Pkg>, String> {
    Ok(parse_search(&run(&["search", query])?))
}

pub fn install_args(id: &str) -> Vec<String> {
    ["install", "--id", id, "--exact", "--silent", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"].map(String::from).to_vec()
}

pub fn update_args(id: &str) -> Vec<String> {
    ["upgrade", "--id", id, "--exact", "--silent", "--include-unknown", "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"].map(String::from).to_vec()
}

pub fn remove_args(id: &str) -> Vec<String> {
    ["uninstall", "--id", id, "--exact", "--silent", "--disable-interactivity"].map(String::from).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPGRADE: &str = "\r   - \r   \\ \rName                      Id                        Version      Available    Source\r\n\
-----------------------------------------------------------------------------------------\r\n\
Git                       Git.Git                   2.45.0       2.46.1       winget\r\n\
Visual Studio Code        Microsoft.VisualStudioCode 1.90.0      1.94.2       winget\r\n\
Some Tool                 Vendor.Tool               Unknown      3.0          winget\r\n\
3 upgrades available.\r\n";

    #[test]
    fn upgrade_table_with_spinner_prefix() {
        let rows = parse_list(UPGRADE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "Git.Git");
        assert_eq!(rows[0].available.as_deref(), Some("2.46.1"));
        assert_eq!(rows[2].version, "Unknown");
    }

    #[test]
    fn list_without_available_column() {
        let text = "Name      Id          Version  Source\n------------------------------------\nPython 3  Python.Python.3  3.12.1  winget\nLocal App  Local.App  1.0\n";
        let rows = parse_list(text);
        assert_eq!(rows[0].name, "Python 3");
        assert_eq!(rows[0].available, None);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn truncated_versions_and_blocks() {
        let text = "Name   Id     Version  Available  Source\n----------------------------------\nA      A.A    < 1.0    2.0        winget\n\nThe following packages need explicit targeting:\nName   Id     Version  Available  Source\n----------------------------------\nB      B.B    1.0      1.1        winget\n";
        let rows = parse_list(text);
        assert_eq!(rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["A.A", "B.B"]);
        assert_eq!(rows[0].version, "1.0");
    }

    #[test]
    fn search_table() {
        let text = "Name    Id        Version  Match       Source\n--------------------------------------------\nGit     Git.Git   2.46.1   Tag: git    winget\n";
        let rows = parse_search(text);
        assert_eq!(rows[0].id, "Git.Git");
        assert_eq!(rows[0].origin, "winget");
    }

    #[test]
    fn empty_output() {
        assert!(parse_list("No installed package found matching input criteria.\n").is_empty());
    }
}
