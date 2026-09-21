use std::path::{Path, PathBuf};
use anyhow::{ensure, Result};
use crate::config::ExternalProviderConfig;
use crate::model::SourceKind;
use super::{cline_family, local_io::ScopedReader};

pub(crate) fn roots(source: SourceKind, config: &ExternalProviderConfig) -> Vec<PathBuf> {
    if !config.path.trim().is_empty() { return vec![PathBuf::from(config.path.trim())]; }
    if !config.paths.is_empty() { return config.paths.iter().filter(|p| !p.trim().is_empty()).map(|p| PathBuf::from(p.trim())).collect(); }
    let variables: &[&str] = match source {
        SourceKind::QwenCode => &["QWEN_RUNTIME_DIR", "QWEN_HOME"], SourceKind::Continue => &["CONTINUE_GLOBAL_DIR"],
        SourceKind::Cursor => &["CURSOR_USER_DIR"], SourceKind::GithubCopilot => &["COPILOT_CLI_HOME"],
        SourceKind::KimiCode => &["KIMI_CODE_HOME", "KIMI_SHARE_DIR", "KIMI_HOME"], _ => &[],
    };
    for name in variables { if let Ok(value) = std::env::var(name) { if !value.trim().is_empty() { return vec![PathBuf::from(value.trim())]; } } }
    let Some(home) = dirs::home_dir() else { return Vec::new(); };
    let editor = |name: &str| -> PathBuf {
        #[cfg(target_os = "macos")]
        { home.join("Library/Application Support").join(name).join("User") }
        #[cfg(windows)]
        { std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Roaming")).join(name).join("User") }
        #[cfg(not(any(windows, target_os = "macos")))]
        { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join(name).join("User") }
    };
    match source {
        SourceKind::QwenCode => vec![home.join(".qwen")], SourceKind::Continue => vec![home.join(".continue")],
        SourceKind::CursorAgent => vec![home.join(".cursor")], SourceKind::Cursor => vec![editor("Cursor")],
        SourceKind::Cline | SourceKind::RooCode | SourceKind::KiloCode => ["Code", "Code - Insiders", "Cursor", "VSCodium", "Codium"].iter().map(|n| editor(n)).collect(),
        SourceKind::GithubCopilot => vec![home.join(".copilot"), editor("Code"), editor("Code - Insiders")],
        SourceKind::KimiCode => vec![home.join(".kimi-code"), home.join(".kimi")],
        SourceKind::Antigravity => vec![home.join(".gemini/antigravity-cli"), home.join(".gemini/antigravity")],
        _ => Vec::new(), // Aider has no implicit recursive home-directory search.
    }
}
fn dirs_in(io: &ScopedReader, path: &Path) -> Result<Vec<PathBuf>> {
    io.children(path)?.into_iter().filter_map(|p| match io.checked_path(&p) { Ok(abs) if abs.is_dir() => Some(Ok(p)), Ok(_) => None, Err(e) => Some(Err(e)) }).collect()
}
fn add_file(io: &ScopedReader, list: &mut Vec<PathBuf>, path: PathBuf) -> Result<()> {
    if io.exists(&path)? && io.checked_path(&path)?.is_file() { list.push(path); }
    ensure!(list.len() <= io.limits().max_entries, "provider candidate budget exceeded"); Ok(())
}
pub(crate) fn candidates(io: &ScopedReader, source: SourceKind) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    match source {
        SourceKind::QwenCode | SourceKind::CursorAgent => {
            for project in dirs_in(io, Path::new("projects"))? {
                let sub = project.join(if source == SourceKind::QwenCode { "chats" } else { "agent-transcripts" });
                let entries = if source == SourceKind::QwenCode { io.children(&sub)? } else { io.walk_files(&sub, 3)? };
                for path in entries { if path.extension().and_then(|x| x.to_str()) == Some("jsonl") { add_file(io, &mut files, path)?; } }
            }
        }
        SourceKind::Continue => for path in io.children(Path::new("sessions"))? {
            if path.extension().and_then(|x| x.to_str()) == Some("json") && path.file_name().and_then(|x| x.to_str()) != Some("sessions.json") { add_file(io, &mut files, path)?; }
        },
        SourceKind::Cline | SourceKind::RooCode | SourceKind::KiloCode => for base in cline_family::bases(io, source)? {
            for task in dirs_in(io, &base.join("tasks"))? {
                let api = task.join("api_conversation_history.json");
                let chosen = if io.exists(&api)? { api } else { task.join("ui_messages.json") };
                add_file(io, &mut files, chosen)?;
            }
        },
        SourceKind::Aider => {
            for path in io.walk_files(Path::new(""), 16)? { if path.file_name().and_then(|n| n.to_str()) == Some(".aider.chat.history.md") { files.push(path); } }
        }
        SourceKind::KimiCode => for workspace in dirs_in(io, Path::new("sessions"))? {
            for dir in dirs_in(io, &workspace)? {
                let modern = dir.join("agents/main/wire.jsonl");
                let chosen = if io.exists(&modern)? { modern } else { dir.join("context.jsonl") };
                add_file(io, &mut files, chosen)?;
            }
        },
        SourceKind::Cursor => {
            add_file(io, &mut files, PathBuf::from("globalStorage/state.vscdb"))?;
            for workspace in dirs_in(io, Path::new("workspaceStorage"))? { add_file(io, &mut files, workspace.join("state.vscdb"))?; }
        }
        SourceKind::GithubCopilot => {
            for session in dirs_in(io, Path::new("session-state"))? { add_file(io, &mut files, session.join("events.jsonl"))?; }
            for workspace in dirs_in(io, Path::new("workspaceStorage"))? {
                let entries = io.children(&workspace.join("chatSessions"))?;
                for path in entries {
                    if path.extension().and_then(|x| x.to_str()) == Some("jsonl") { add_file(io, &mut files, path)?; }
                    else if path.extension().and_then(|x| x.to_str()) == Some("json") && !io.exists(&path.with_extension("jsonl"))? { add_file(io, &mut files, path)?; }
                }
            }
        }
        SourceKind::Antigravity => for dir in dirs_in(io, Path::new("brain"))? {
            let full = dir.join(".system_generated/logs/transcript_full.jsonl");
            let chosen = if io.exists(&full)? { full } else { dir.join(".system_generated/logs/transcript.jsonl") };
            add_file(io, &mut files, chosen)?;
        },
        _ => {}
    }
    ensure!(files.len() <= io.limits().max_entries, "provider candidate budget exceeded");
    files.sort(); Ok(files)
}
pub(crate) fn admitted(source: SourceKind, path: &Path) -> bool {
    let c: Vec<_> = path.iter().filter_map(|s| s.to_str()).collect();
    let last = c.last().copied().unwrap_or("");
    match source {
        SourceKind::QwenCode => c.len() == 4 && c[0] == "projects" && c[2] == "chats" && last.ends_with(".jsonl"),
        SourceKind::Continue => c.len() == 2 && c[0] == "sessions" && last.ends_with(".json") && last != "sessions.json",
        SourceKind::CursorAgent => (4..=7).contains(&c.len()) && c[0] == "projects" && c[2] == "agent-transcripts" && last.ends_with(".jsonl"),
        SourceKind::Cline | SourceKind::RooCode | SourceKind::KiloCode => {
            let task = c.len() == 3 && c[0] == "tasks" || c.len() == 5 && c[0] == "globalStorage" && c[1] == cline_family::extension(source) && c[2] == "tasks";
            task && matches!(last, "api_conversation_history.json" | "ui_messages.json")
        }
        SourceKind::Aider => last == ".aider.chat.history.md" && c.len() <= 17,
        SourceKind::KimiCode => c.first() == Some(&"sessions") && (c.len() == 4 && last == "context.jsonl" || c.len() == 6 && path.ends_with("agents/main/wire.jsonl")),
        SourceKind::Cursor => path == Path::new("globalStorage/state.vscdb") || c.len() == 3 && c[0] == "workspaceStorage" && last == "state.vscdb",
        SourceKind::GithubCopilot => c.len() == 3 && c[0] == "session-state" && last == "events.jsonl" || c.len() == 4 && c[0] == "workspaceStorage" && c[2] == "chatSessions" && (last.ends_with(".json") || last.ends_with(".jsonl")),
        SourceKind::Antigravity => c.len() == 5 && c[0] == "brain" && c[2] == ".system_generated" && c[3] == "logs" && matches!(last, "transcript_full.jsonl" | "transcript.jsonl"),
        _ => false,
    }
}
pub(crate) fn marker(io: &ScopedReader, source: SourceKind) -> Result<bool> {
    let markers: &[&str] = match source {
        SourceKind::QwenCode | SourceKind::CursorAgent => &["projects"], SourceKind::Continue | SourceKind::KimiCode => &["sessions"],
        SourceKind::Cursor => &["globalStorage/state.vscdb", "workspaceStorage"], SourceKind::GithubCopilot => &["session-state", "workspaceStorage"],
        SourceKind::Antigravity => &["brain", ".token-monitor", "conversations"], SourceKind::Aider => &[""],
        SourceKind::Cline | SourceKind::RooCode | SourceKind::KiloCode => return Ok(!cline_family::bases(io, source)?.is_empty()), _ => &[],
    };
    for m in markers { if io.exists(Path::new(m))? { return Ok(true); } } Ok(false)
}
