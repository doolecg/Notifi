//! Counts the terminals open for each agent by looking for its program in the process list,
//! so "sessions" on the pill means terminals you have open, not logs that were touched.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::agents::Kind;

const EVERY: Duration = Duration::from_secs(3);

/// Position of an agent in the counts: Claude, Codex, OpenCode, Copilot, Antigravity.
pub fn index(kind: Kind) -> usize {
    match kind {
        Kind::Claude => 0,
        Kind::Codex => 1,
        Kind::OpenCode => 2,
        Kind::Copilot => 3,
        Kind::Antigravity => 4,
    }
}

/// Which agent a program is the terminal app of. The Claude desktop chat app and the Windows Copilot app share
/// the names `claude.exe` and `copilot.exe` with the command-line tools, so they are told apart by where they
/// are installed. Antigravity's desktop app is counted like OpenCode's: it hosts the conversations the way a
/// terminal does for the others.
pub fn agent_of(name: &str, exe: Option<&Path>) -> Option<Kind> {
    let name = name.to_ascii_lowercase();
    let name = name.trim_end_matches(".exe");
    let path = exe.map(|p| p.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match name {
        "claude" if !(path.contains("anthropicclaude") || path.contains("windowsapps") || path.contains("program files")) => Some(Kind::Claude),
        "codex" => Some(Kind::Codex),
        "opencode" => Some(Kind::OpenCode),
        "copilot" if !path.contains("windowsapps") => Some(Kind::Copilot),
        "agy" | "antigravity" => Some(Kind::Antigravity),
        _ => None,
    }
}

/// A command-line tool that is a script has its runtime's name (`node`), so its command line says which tool it is.
pub fn agent_of_script(name: &str, cmd: &[String]) -> Option<Kind> {
    let name = name.to_ascii_lowercase();
    if !matches!(name.trim_end_matches(".exe"), "node" | "bun" | "deno") {
        return None;
    }
    let line = cmd.join(" ").to_ascii_lowercase().replace('\\', "/");
    [("@anthropic-ai/claude-code", Kind::Claude), ("@openai/codex", Kind::Codex), ("@github/copilot", Kind::Copilot)]
        .into_iter()
        .find(|(package, _)| line.contains(package))
        .map(|(_, kind)| kind)
}

/// The agent a process is the terminal app of, by its name and install folder or, for a script, its command line.
fn agent_of_process(p: &sysinfo::Process) -> Option<Kind> {
    let name = p.name().to_string_lossy();
    agent_of(&name, p.exe()).or_else(|| {
        let cmd: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
        agent_of_script(&name, &cmd)
    })
}

pub struct Counter {
    sys: System,
    next: Instant,
    last: [u32; KINDS],
}

/// One slot per agent.
const KINDS: usize = Kind::ALL.len();

impl Counter {
    pub fn new() -> Self {
        Self { sys: System::new(), next: Instant::now(), last: [0; KINDS] }
    }

    /// Terminals open per agent. A program whose parent is another copy of itself is one terminal, not two.
    pub fn tick(&mut self) -> [u32; KINDS] {
        if Instant::now() < self.next {
            return self.last;
        }
        self.next = Instant::now() + EVERY;
        let what = ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet);
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, what);
        let mut counts = [0u32; KINDS];
        for p in self.sys.processes().values() {
            let Some(kind) = agent_of_process(p) else { continue };
            let nested = p.parent().and_then(|pid| self.sys.process(pid)).is_some_and(|parent| agent_of_process(parent) == Some(kind));
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

    #[test]
    fn finds_the_command_line_tools_and_apps_of_each_agent() {
        assert_eq!(agent_of("agy.exe", Some(Path::new("C:\\Users\\me\\AppData\\Local\\agy\\bin\\agy.exe"))), Some(Kind::Antigravity));
        assert_eq!(agent_of("Antigravity.exe", Some(Path::new("C:\\Users\\me\\AppData\\Local\\Programs\\antigravity\\Antigravity.exe"))), Some(Kind::Antigravity));
        assert_eq!(agent_of("opencode.exe", None), Some(Kind::OpenCode));
        assert_eq!(agent_of("copilot.exe", Some(Path::new("C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Packages\\GitHub.Copilot\\copilot.exe"))), Some(Kind::Copilot));
        // The Windows Copilot app is not GitHub's command line.
        assert_eq!(agent_of("Copilot.exe", Some(Path::new("C:\\Program Files\\WindowsApps\\Microsoft.Copilot_1.0\\Copilot.exe"))), None);
    }

    #[test]
    fn finds_a_script_tool_by_its_command_line() {
        let cmd = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let copilot = cmd(&["node", "C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\@github\\copilot\\index.js", "--banner"]);
        assert_eq!(agent_of_script("node.exe", &copilot), Some(Kind::Copilot));
        assert_eq!(agent_of_script("node", &cmd(&["node", "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"])), Some(Kind::Claude));
        assert_eq!(agent_of_script("bun.exe", &cmd(&["bun", "x", "@openai/codex"])), Some(Kind::Codex));
        // Some other script, or the same words in a program that is not a script runtime.
        assert_eq!(agent_of_script("node.exe", &cmd(&["node", "server.js"])), None);
        assert_eq!(agent_of_script("notepad.exe", &copilot), None);
    }
}
