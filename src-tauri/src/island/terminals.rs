//! Counts the terminals open for each agent by looking for its program in the process list,
//! so "sessions" on the pill means terminals you have open, not logs that were touched.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::agents::Kind;

const EVERY: Duration = Duration::from_secs(3);

/// Position of an agent in the counts: Claude, Codex, OpenCode, Copilot.
pub fn index(kind: Kind) -> usize {
    match kind {
        Kind::Claude => 0,
        Kind::Codex => 1,
        Kind::OpenCode => 2,
        Kind::Copilot => 3,
    }
}

/// Which agent a program is the terminal app of. The Claude desktop chat app shares the name
/// `claude.exe`, so it is told apart by where it is installed.
pub fn agent_of(name: &str, exe: Option<&Path>) -> Option<Kind> {
    let name = name.to_ascii_lowercase();
    let name = name.trim_end_matches(".exe");
    let path = exe.map(|p| p.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match name {
        "claude" if !(path.contains("anthropicclaude") || path.contains("windowsapps") || path.contains("program files")) => Some(Kind::Claude),
        "codex" => Some(Kind::Codex),
        "opencode" => Some(Kind::OpenCode),
        _ => None,
    }
}

pub struct Counter {
    sys: System,
    next: Instant,
    last: [u32; 4],
}

impl Counter {
    pub fn new() -> Self {
        Self { sys: System::new(), next: Instant::now(), last: [0; 4] }
    }

    /// Terminals open per agent. A program whose parent is another copy of itself is one terminal, not two.
    pub fn tick(&mut self) -> [u32; 4] {
        if Instant::now() < self.next {
            return self.last;
        }
        self.next = Instant::now() + EVERY;
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
        let mut counts = [0u32; 4];
        for p in self.sys.processes().values() {
            let Some(kind) = agent_of(&p.name().to_string_lossy(), p.exe()) else { continue };
            let nested = p
                .parent()
                .and_then(|pid| self.sys.process(pid))
                .is_some_and(|parent| agent_of(&parent.name().to_string_lossy(), parent.exe()) == Some(kind));
            if !nested {
                counts[index(kind)] += 1;
            }
        }
        self.last = counts;
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_the_terminal_app_from_the_desktop_app() {
        let cli = Path::new("C:\\Users\\me\\.local\\bin\\claude.exe");
        let desktop = Path::new("C:\\Users\\me\\AppData\\Local\\AnthropicClaude\\app-1.0\\claude.exe");
        assert_eq!(agent_of("claude.exe", Some(cli)), Some(Kind::Claude));
        assert_eq!(agent_of("Claude.exe", Some(desktop)), None);
        assert_eq!(agent_of("codex", None), Some(Kind::Codex));
        assert_eq!(agent_of("node.exe", Some(cli)), None);
    }
}
