//! What a hook request becomes, and the exact JSON Claude Code expects back. No I/O here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const QUESTION: &str = "question";
pub const PERMISSION: &str = "permission";

/// Longest command or path shown in a permission summary.
const SUMMARY_CHARS: usize = 300;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Choice {
    pub label: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Question {
    pub header: String,
    pub question: String,
    pub multi_select: bool,
    pub options: Vec<Choice>,
}

/// One request waiting for an answer. `input` is the hook's `tool_input` untouched.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub id: u64,
    pub kind: &'static str,
    pub agent: &'static str,
    pub project: Option<String>,
    pub session: String,
    pub tool: String,
    pub questions: Vec<Question>,
    pub summary: String,
    pub input: Value,
    pub at: u64,
}

/// The request a hook call describes. `None` when the body has nothing the Island can show,
/// which the caller answers with an empty reply so Claude Code asks in the terminal.
pub fn new_ask(id: u64, kind: &'static str, body: &Value, at: u64) -> Option<Ask> {
    let tool = body.get("tool_name")?.as_str()?.to_string();
    let input = body.get("tool_input").cloned().unwrap_or(Value::Null);
    let questions: Vec<Question> = if kind == QUESTION {
        let list: Vec<Question> = serde_json::from_value(input.get("questions")?.clone()).ok()?;
        if list.is_empty() || list.iter().any(|q| q.question.is_empty()) {
            return None;
        }
        list
    } else {
        Vec::new()
    };
    let summary = match questions.first() {
        Some(q) => q.question.clone(),
        None => summarise(&tool, &input),
    };
    Some(Ask {
        id,
        kind,
        agent: "claude",
        project: body.get("cwd").and_then(Value::as_str).and_then(project_name),
        session: body.get("session_id").and_then(Value::as_str).unwrap_or_default().to_string(),
        tool,
        questions,
        summary,
        input,
        at,
    })
}

/// The folder name of a working directory, with either kind of slash.
pub fn project_name(cwd: &str) -> Option<String> {
    cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).map(str::to_string)
}

/// "Bash: npm test", "Edit: C:\repo\a.rs", or just the tool name when its input has nothing to show.
pub fn summarise(tool: &str, input: &Value) -> String {
    let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let detail = ["command", "file_path", "notebook_path", "url", "pattern", "path", "description"].into_iter().find_map(text);
    match detail {
        Some(d) if d.chars().count() > SUMMARY_CHARS => format!("{tool}: {}\u{2026}", d.chars().take(SUMMARY_CHARS).collect::<String>()),
        Some(d) => format!("{tool}: {d}"),
        None => tool.to_string(),
    }
}

/// Answers the question hook: allow, with the original questions and an `answers` object keyed by
/// question text. A multi-select value is the chosen labels joined by ", " (the caller joins them).
/// Keys that are not one of the questions are dropped.
pub fn question_reply(ask: &Ask, answers: &HashMap<String, String>) -> Result<Value, String> {
    let mut input = ask.input.as_object().cloned().ok_or("That question has no input to answer.")?;
    let mut chosen = Map::new();
    for q in &ask.questions {
        if let Some(a) = answers.get(&q.question) {
            chosen.insert(q.question.clone(), Value::String(a.clone()));
        }
    }
    if chosen.is_empty() {
        return Err("No answer matches the questions asked.".into());
    }
    input.insert("answers".into(), Value::Object(chosen));
    Ok(json!({ "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "allow",
        "updatedInput": Value::Object(input),
    } }))
}

pub fn permission_reply(allow: bool, message: Option<&str>) -> Value {
    let decision = match (allow, message.map(str::trim).filter(|m| !m.is_empty())) {
        (true, _) => json!({ "behavior": "allow" }),
        (false, Some(m)) => json!({ "behavior": "deny", "message": m }),
        (false, None) => json!({ "behavior": "deny" }),
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
}

/// Leaves it to Claude Code's own prompt in the terminal.
pub fn skip_reply(kind: &str) -> Value {
    if kind == QUESTION {
        json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "ask" } })
    } else {
        json!({})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question_body() -> Value {
        json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "AskUserQuestion",
            "session_id": "s1",
            "cwd": "C:\\Users\\me\\repos\\Notifi\\",
            "tool_input": { "questions": [
                { "header": "Lib", "question": "Which library?", "multiSelect": false,
                  "options": [{ "label": "React", "description": "UI" }, { "label": "Svelte", "description": "" }] },
                { "header": "Extras", "question": "Which extras?", "multiSelect": true,
                  "options": [{ "label": "A", "description": "" }, { "label": "B", "description": "" }] }
            ] }
        })
    }

    #[test]
    fn a_question_request_keeps_its_questions_and_names_the_project() {
        let ask = new_ask(7, QUESTION, &question_body(), 99).unwrap();
        assert_eq!((ask.id, ask.kind, ask.agent, ask.at), (7, "question", "claude", 99));
        assert_eq!(ask.project.as_deref(), Some("Notifi"));
        assert_eq!(ask.session, "s1");
        assert_eq!(ask.questions.len(), 2);
        assert!(ask.questions[1].multi_select);
        assert_eq!(ask.questions[0].options[0].label, "React");
        assert_eq!(ask.summary, "Which library?");
        let wire = serde_json::to_value(&ask).unwrap();
        assert_eq!(wire["questions"][1]["multiSelect"], true);
        assert!(wire.get("kind").is_some() && wire.get("input").is_some());
    }

    #[test]
    fn a_question_request_without_questions_is_not_shown() {
        let none = json!({ "tool_name": "AskUserQuestion", "tool_input": {} });
        assert!(new_ask(1, QUESTION, &none, 0).is_none());
        let empty = json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [] } });
        assert!(new_ask(1, QUESTION, &empty, 0).is_none());
        assert!(new_ask(1, QUESTION, &json!({ "tool_input": {} }), 0).is_none());
    }

    #[test]
    fn project_names_come_from_either_slash() {
        assert_eq!(project_name("/home/me/code/app").as_deref(), Some("app"));
        assert_eq!(project_name("C:\\code\\app\\").as_deref(), Some("app"));
        assert_eq!(project_name("/"), None);
        assert_eq!(project_name(""), None);
    }

    #[test]
    fn permission_summaries_show_the_command_or_path() {
        assert_eq!(summarise("Bash", &json!({ "command": " npm test " })), "Bash: npm test");
        assert_eq!(summarise("Edit", &json!({ "file_path": "C:\\a\\b.rs", "old_string": "x" })), "Edit: C:\\a\\b.rs");
        assert_eq!(summarise("WebFetch", &json!({ "url": "https://example.com" })), "WebFetch: https://example.com");
        assert_eq!(summarise("Task", &json!({ "description": "Find bugs" })), "Task: Find bugs");
        assert_eq!(summarise("Mystery", &json!({ "n": 1 })), "Mystery");
        assert_eq!(summarise("Mystery", &Value::Null), "Mystery");
        let long = summarise("Bash", &json!({ "command": "x".repeat(1000) }));
        assert_eq!(long.chars().count(), "Bash: ".len() + SUMMARY_CHARS + 1);
        assert!(long.ends_with('\u{2026}'));
    }

    #[test]
    fn a_permission_request_gets_a_summary_and_no_questions() {
        let body = json!({ "tool_name": "Bash", "tool_input": { "command": "ls" }, "cwd": "/x/y", "session_id": "z" });
        let ask = new_ask(2, PERMISSION, &body, 5).unwrap();
        assert_eq!(ask.summary, "Bash: ls");
        assert!(ask.questions.is_empty());
        assert_eq!(ask.input, json!({ "command": "ls" }));
    }

    #[test]
    fn answers_are_keyed_by_question_text_and_the_questions_are_echoed() {
        let ask = new_ask(1, QUESTION, &question_body(), 0).unwrap();
        let answers = HashMap::from([
            ("Which library?".to_string(), "React".to_string()),
            ("Which extras?".to_string(), "A, B".to_string()),
            ("Not asked".to_string(), "ignored".to_string()),
        ]);
        let reply = question_reply(&ask, &answers).unwrap();
        let out = &reply["hookSpecificOutput"];
        assert_eq!(out["hookEventName"], "PreToolUse");
        assert_eq!(out["permissionDecision"], "allow");
        assert_eq!(out["updatedInput"]["questions"], ask.input["questions"]);
        assert_eq!(out["updatedInput"]["answers"], json!({ "Which library?": "React", "Which extras?": "A, B" }));
    }

    #[test]
    fn free_text_answers_pass_through_and_unmatched_answers_are_refused() {
        let ask = new_ask(1, QUESTION, &question_body(), 0).unwrap();
        let free = HashMap::from([("Which library?".to_string(), "Solid, actually".to_string())]);
        let reply = question_reply(&ask, &free).unwrap();
        assert_eq!(reply["hookSpecificOutput"]["updatedInput"]["answers"]["Which library?"], "Solid, actually");
        let wrong = HashMap::from([("Other".to_string(), "x".to_string())]);
        assert!(question_reply(&ask, &wrong).is_err());
        assert!(question_reply(&ask, &HashMap::new()).is_err());
    }

    #[test]
    fn permission_replies_match_the_hook_schema() {
        assert_eq!(
            permission_reply(true, Some("ignored")),
            json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "allow" } } })
        );
        assert_eq!(
            permission_reply(false, Some(" Not now ")),
            json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "deny", "message": "Not now" } } })
        );
        assert_eq!(permission_reply(false, Some("  "))["hookSpecificOutput"]["decision"], json!({ "behavior": "deny" }));
        assert_eq!(permission_reply(false, None)["hookSpecificOutput"]["decision"], json!({ "behavior": "deny" }));
    }

    #[test]
    fn skipping_hands_the_prompt_back_to_the_terminal() {
        assert_eq!(
            skip_reply(QUESTION),
            json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "ask" } })
        );
        assert_eq!(skip_reply(PERMISSION), json!({}));
    }
}
