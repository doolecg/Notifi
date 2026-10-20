//! Running package-manager commands off the UI thread, with a timeout and no console window.

use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// The name to spawn: Scoop ships as a `.cmd` shim on Windows.
pub fn program(name: &str) -> String {
    if cfg!(windows) && name == "scoop" { "scoop.cmd".into() } else { name.into() }
}

pub struct Output {
    pub stdout: String,
}

/// Runs `program args`, returning its output. `Err` when it cannot start or runs past `timeout`.
pub fn run(name: &str, args: &[&str], timeout: Duration) -> Result<Output, String> {
    let mut cmd = command(&program(name));
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("{name} could not be started: {e}"))?;
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = out.read_to_end(&mut bytes);
        bytes
    });
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = err.read_to_end(&mut bytes);
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{name} took too long and was stopped"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let bytes = reader.join().unwrap_or_default();
    let _ = drain.join();
    Ok(Output { stdout: String::from_utf8_lossy(&bytes).into_owned() })
}

/// Whether the tool starts at all.
pub fn exists(name: &str, probe: &str) -> bool {
    run(name, &[probe], Duration::from_secs(8)).is_ok()
}
