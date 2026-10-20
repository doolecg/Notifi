//! Questions and permission requests from Claude Code, shown in the Island.
//!
//! Claude Code calls a local HTTP hook (`server`), the call waits in `AskState`, and the Island
//! answers through the commands below. `hooks` installs the hooks in Claude Code's settings.

mod hooks;
mod logic;
mod server;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
        Mutex,
    },
};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use logic::{Ask, PERMISSION, QUESTION};
use server::Info;

struct Entry {
    ask: Ask,
    reply: Sender<Value>,
}

/// Requests waiting for an answer, and the server's port and token once it is listening.
#[derive(Default)]
pub struct AskState {
    pending: Mutex<Vec<Entry>>,
    next: AtomicU64,
    info: Mutex<Option<Info>>,
}

impl AskState {
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn add(&self, ask: Ask, reply: Sender<Value>) {
        self.pending.lock().unwrap().push(Entry { ask, reply });
    }

    fn take(&self, id: u64) -> Option<Entry> {
        let mut pending = self.pending.lock().unwrap();
        let at = pending.iter().position(|e| e.ask.id == id)?;
        Some(pending.remove(at))
    }

    fn info(&self) -> Option<Info> {
        self.info.lock().unwrap().clone()
    }

    /// Hands the reply to the waiting hook call. A request of the wrong kind stays queued.
    fn resolve(&self, id: u64, kind: &str, reply: impl FnOnce(&Ask) -> Result<Value, String>) -> Result<(), String> {
        let mut pending = self.pending.lock().unwrap();
        let at = pending.iter().position(|e| e.ask.id == id).ok_or("That request is no longer waiting.")?;
        if pending[at].ask.kind != kind {
            return Err(format!("That request is not a {kind}."));
        }
        let value = reply(&pending[at].ask)?;
        let entry = pending.remove(at);
        entry.reply.send(value).map_err(|_| "That request is no longer waiting.".to_string())
    }
}

pub fn start(app: &AppHandle) {
    server::start(app);
}

#[tauri::command]
pub fn ask_list(state: State<AskState>) -> Vec<Ask> {
    state.pending.lock().unwrap().iter().map(|e| e.ask.clone()).collect()
}

/// `answers` maps each question's text to the chosen label, the labels joined by ", " for a
/// multi-select, or free text.
#[tauri::command]
pub fn ask_answer(id: u64, answers: HashMap<String, String>, state: State<AskState>) -> Result<(), String> {
    state.resolve(id, QUESTION, |ask| logic::question_reply(ask, &answers))
}

#[tauri::command]
pub fn ask_decide(id: u64, allow: bool, message: Option<String>, state: State<AskState>) -> Result<(), String> {
    state.resolve(id, PERMISSION, |_| Ok(logic::permission_reply(allow, message.as_deref())))
}

/// Lets Claude Code show its own prompt in the terminal.
#[tauri::command]
pub fn ask_skip(id: u64, state: State<AskState>) -> Result<(), String> {
    let entry = state.take(id).ok_or("That request is no longer waiting.")?;
    entry.reply.send(logic::skip_reply(entry.ask.kind)).map_err(|_| "That request is no longer waiting.".to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksStatus {
    /// Both hooks are in Claude Code's settings and point at this server.
    installed: bool,
    /// 0 when the server could not start.
    port: u16,
    settings_path: String,
    /// The settings file holds other hooks, which Notifi never touches.
    other_hooks_kept: bool,
}

fn hooks_status(state: &AskState) -> HooksStatus {
    let (path, info) = (hooks::settings_path(), state.info());
    let (installed, other_hooks_kept) = match (&path, &info) {
        (Some(p), Some(i)) => hooks::status(p, i.port, &i.token),
        _ => (false, false),
    };
    HooksStatus {
        installed,
        port: info.map_or(0, |i| i.port),
        settings_path: path.map(|p| p.display().to_string()).unwrap_or_default(),
        other_hooks_kept,
    }
}

fn target(state: &AskState) -> Result<(std::path::PathBuf, Info), String> {
    let info = state.info().ok_or("The hook server is not running, so there is nothing to install.")?;
    let path = hooks::settings_path().ok_or("Could not find your home folder.")?;
    Ok((path, info))
}

#[tauri::command]
pub fn ask_hooks_status(state: State<AskState>) -> HooksStatus {
    hooks_status(&state)
}

#[tauri::command]
pub fn ask_hooks_install(state: State<AskState>) -> Result<HooksStatus, String> {
    let (path, info) = target(&state)?;
    hooks::install(&path, info.port, &info.token)?;
    Ok(hooks_status(&state))
}

#[tauri::command]
pub fn ask_hooks_remove(state: State<AskState>) -> Result<HooksStatus, String> {
    let (path, info) = target(&state)?;
    hooks::remove(&path, &info.token)?;
    Ok(hooks_status(&state))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use serde_json::json;

    use super::*;

    fn queued(state: &AskState, kind: &'static str, body: Value) -> (u64, mpsc::Receiver<Value>) {
        let id = state.next_id();
        let (tx, rx) = mpsc::channel();
        state.add(logic::new_ask(id, kind, &body, 0).unwrap(), tx);
        (id, rx)
    }

    fn question() -> Value {
        json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [{ "header": "h", "question": "Pick?", "multiSelect": false, "options": [] }] } })
    }

    fn permission() -> Value {
        json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })
    }

    #[test]
    fn ids_count_up_from_one() {
        let state = AskState::default();
        assert_eq!((state.next_id(), state.next_id()), (1, 2));
    }

    #[test]
    fn an_answer_reaches_the_waiting_call_once() {
        let state = AskState::default();
        let (id, rx) = queued(&state, QUESTION, question());
        let answers = HashMap::from([("Pick?".to_string(), "A".to_string())]);
        state.resolve(id, QUESTION, |ask| logic::question_reply(ask, &answers)).unwrap();
        assert_eq!(rx.try_recv().unwrap()["hookSpecificOutput"]["updatedInput"]["answers"]["Pick?"], "A");
        assert!(state.pending.lock().unwrap().is_empty());
        assert!(state.resolve(id, QUESTION, |ask| logic::question_reply(ask, &answers)).is_err());
    }

    #[test]
    fn a_bad_answer_or_the_wrong_kind_leaves_the_request_queued() {
        let state = AskState::default();
        let (q, _qrx) = queued(&state, QUESTION, question());
        let (p, _prx) = queued(&state, PERMISSION, permission());
        assert!(state.resolve(q, QUESTION, |ask| logic::question_reply(ask, &HashMap::new())).is_err());
        assert!(state.resolve(q, PERMISSION, |_| Ok(json!({}))).is_err());
        assert!(state.resolve(p, QUESTION, |_| Ok(json!({}))).is_err());
        assert_eq!(state.pending.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_hung_up_call_is_an_error_not_a_hang() {
        let state = AskState::default();
        let (id, rx) = queued(&state, PERMISSION, permission());
        drop(rx);
        assert!(state.resolve(id, PERMISSION, |_| Ok(json!({}))).is_err());
        assert!(state.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn take_removes_only_the_named_request() {
        let state = AskState::default();
        let (a, _ra) = queued(&state, PERMISSION, permission());
        let (b, _rb) = queued(&state, PERMISSION, permission());
        assert_eq!(state.take(a).unwrap().ask.id, a);
        assert!(state.take(a).is_none());
        assert_eq!(state.pending.lock().unwrap()[0].ask.id, b);
    }
}
