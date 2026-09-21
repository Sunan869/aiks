"""Temporary, idempotent feature-branch integration edits; removed before merge review."""
from pathlib import Path

def edit(path, fn):
    file = Path(path)
    old = file.read_text(encoding='utf-8')
    new = fn(old)
    if new != old:
        file.write_text(new, encoding='utf-8')
        print('Updated', path)

def replace_once(text, old, new):
    if new in text:
        return text
    if text.count(old) != 1:
        raise RuntimeError('Expected exactly one anchor: ' + old[:100])
    return text.replace(old, new, 1)

sources = [('Antigravity','antigravity','Antigravity'), ('Cursor','cursor','Cursor'), ('CursorAgent','cursor_agent','Cursor Agent'), ('Cline','cline','Cline'), ('RooCode','roo_code','Roo Code'), ('KiloCode','kilo_code','Kilo Code'), ('GithubCopilot','github_copilot','GitHub Copilot'), ('KimiCode','kimi_code','Kimi Code'), ('QwenCode','qwen_code','Qwen Code'), ('Continue','continue','Continue'), ('Aider','aider','Aider')]

def model(s):
    if '    Antigravity,' not in s:
        s = replace_once(s, '    WorkBuddy,\n', '    WorkBuddy,\n' + ''.join(f'    {name},\n' for name,_,_ in sources))
        s = replace_once(s, '            SourceKind::WorkBuddy => "workbuddy",', '            SourceKind::WorkBuddy => "workbuddy",\n' + ''.join(f'            SourceKind::{name} => "{key}",\n' for name,key,_ in sources).rstrip())
        s = replace_once(s, '            SourceKind::WorkBuddy => "WorkBuddy",', '            SourceKind::WorkBuddy => "WorkBuddy",\n' + ''.join(f'            SourceKind::{name} => "{label}",\n' for name,_,label in sources).rstrip())
        anchor = '            "workbuddy" | "work_buddy" => Some(SourceKind::WorkBuddy),'
        s = replace_once(s, anchor, anchor + '\n' + ''.join(f'            "{key}" => Some(SourceKind::{name}),\n' for name,key,_ in sources).rstrip())
    return s
edit('crates/aiks-core/src/model/mod.rs', model)

def config(s):
    if 'pub struct ExternalProviderConfig' not in s:
        fields = ''.join(('    #[serde(rename = "continue")]\n    pub continue_dev: ExternalProviderConfig,\n' if key == 'continue' else f'    pub {key}: ExternalProviderConfig,\n') for _,key,_ in sources)
        s = replace_once(s, '    pub workbuddy: ProviderConfig,\n', '    pub workbuddy: ProviderConfig,\n' + fields)
        s += '\n#[derive(Debug, Clone, Serialize, Deserialize)]\n#[serde(default)]\npub struct ExternalProviderConfig {\n    pub enabled: bool,\n    pub path: String,\n    pub paths: Vec<String>,\n}\nimpl Default for ExternalProviderConfig {\n    fn default() -> Self { Self { enabled: true, path: String::new(), paths: Vec::new() } }\n}\n'
    return s
edit('crates/aiks-core/src/config/mod.rs', config)

def provider(s):
    if 'pub mod native;' not in s:
        s = 'pub mod catalog;\npub mod discovery;\npub mod local_io;\npub mod native;\nmod local_paths;\nmod message_parts;\nmod qwen;\nmod continue_dev;\nmod cursor_agent;\nmod cline_family;\nmod aider;\nmod kimi;\nmod cursor;\nmod copilot;\nmod antigravity;\n' + s
    if 'Unsupported { message: String }' not in s:
        s = replace_once(s, '    NotConfigured,\n', '    NotConfigured,\n    Unsupported { message: String },\n')
        s = replace_once(s, '            ProviderHealth::Ok => "OK",', '            ProviderHealth::Ok => "OK",\n            ProviderHealth::Unsupported { message } => message.as_str(),')
    if 'async fn discover_report(&self)' not in s:
        s = replace_once(s, '    /// Load a full normalized session from a summary', '''    /// Compatibility default for existing providers; new sources report scope and completeness.
    async fn discover_report(&self) -> anyhow::Result<discovery::DiscoveryReport> {
        Ok(discovery::DiscoveryReport { sessions: self.discover_sessions().await?, ..Default::default() })
    }

    /// Load a full normalized session from a summary''')
    if 'pub async fn discover_selected(' not in s:
        start = s.index('    pub async fn discover_all(&self)')
        end = s.index('    /// Perform health check on all providers.', start)
        s = s[:start] + '''    pub async fn discover_selected(&self, selected: Option<SourceKind>) -> Vec<(SourceKind, anyhow::Result<discovery::DiscoveryReport>)> {
        use std::future::{Future, poll_fn};
        use std::pin::Pin;
        use std::task::Poll;
        type Output = (SourceKind, anyhow::Result<discovery::DiscoveryReport>);
        let limit = tokio::sync::Semaphore::new(4);
        let mut pending: Vec<Pin<Box<dyn Future<Output = Output> + Send + '_>>> = self.providers.iter()
            .filter(|p| selected.is_none_or(|s| p.source() == s))
            .map(|p| {
                let limit = &limit;
                Box::pin(async move {
                    let _permit = limit.acquire().await.expect("local provider semaphore stays open");
                    (p.source(), p.discover_report().await)
                }) as Pin<Box<dyn Future<Output = Output> + Send + '_>>
            }).collect();
        let mut reports = Vec::new();
        while !pending.is_empty() {
            let (index, result) = poll_fn(|cx| {
                for (index, future) in pending.iter_mut().enumerate() {
                    if let Poll::Ready(output) = future.as_mut().poll(cx) { return Poll::Ready((index, output)); }
                }
                Poll::Pending
            }).await;
            drop(pending.swap_remove(index));
            reports.push(result);
        }
        reports
    }

    pub async fn discover_all(&self) -> Vec<SessionSummary> {
        self.discover_selected(None).await.into_iter().filter_map(|(source, result)| match result {
            Ok(report) => {
                if !report.complete { tracing::warn!(source = source.as_str(), diagnostics = report.diagnostics.len(), "Provider discovery incomplete; valid sessions retained"); }
                Some(report.sessions)
            }
            Err(_) => { tracing::warn!(source = source.as_str(), "Provider discovery failed"); None }
        }).flatten().collect()
    }

    pub async fn discover_all_detailed(&self) -> Vec<(SourceKind, anyhow::Result<Vec<SessionSummary>>)> {
        self.discover_selected(None).await.into_iter().map(|(source, result)| {
            let result = result.and_then(|report| {
                anyhow::ensure!(report.complete, "Provider scan incomplete; missing detection prohibited");
                Ok(report.sessions)
            });
            (source, result)
        }).collect()
    }

''' + s[end:]
    if 'catalog::EXTERNAL_SOURCES' not in s:
        s = replace_once(s, '    ProviderRegistry::new(providers)', '''    for source in catalog::EXTERNAL_SOURCES {
        if let Some(settings) = catalog::external_config(config, source).filter(|p| p.enabled) {
            match native::NativeProvider::new(source, settings) {
                Ok(provider) => providers.push(Box::new(provider)),
                Err(_) => tracing::warn!(source = source.as_str(), "External provider configuration invalid"),
            }
        }
    }
    ProviderRegistry::new(providers)''')
    return s
edit('crates/aiks-core/src/providers/mod.rs', provider)

def sync(s):
    if 'let selected_provider =' not in s:
        start = s.index('        let all_summaries = registry.discover_all().await;')
        end = s.index('\n        info!(', start)
        s = s[:start] + '''        let selected_provider = opts.source_filter.as_deref().map(|key| {
            crate::model::SourceKind::from_str(key).ok_or_else(|| anyhow::anyhow!("Unknown source filter"))
        }).transpose()?;
        let mut summaries = Vec::new();
        for (source, result) in registry.discover_selected(selected_provider).await {
            match result {
                Ok(report) => {
                    if !report.complete { warn!(source = source.as_str(), "Provider scan incomplete; missing detection disabled"); }
                    summaries.extend(report.sessions);
                }
                Err(_) => warn!(source = source.as_str(), "Provider scan failed"),
            }
        }
        let total_discovered = summaries.len();
''' + s[end:]
    if 'let mut covered_scopes' not in s:
        s = replace_once(s, '        let per_source = registry.discover_all_detailed().await;', '        let per_source = registry.discover_selected(None).await;\n        let mut covered_scopes = std::collections::HashMap::new();')
        s = replace_once(s, '''                Ok(sessions) => {
                    scanned_sources.insert(source.as_str().to_string());
                    for s in sessions {''', '''                Ok(report) => {
                    if !report.complete { continue; }
                    covered_scopes.insert(source.as_str().to_string(), report.covered_paths);
                    scanned_sources.insert(source.as_str().to_string());
                    for s in report.sessions {''')
        s = replace_once(s, '            let key = (stored.source.clone(), stored.external_session_id.clone());', '''            if let Some(scopes) = covered_scopes.get(&stored.source) {
                if !scopes.is_empty() && !stored.source_path.as_ref().is_some_and(|path| scopes.iter().any(|scope| std::path::Path::new(path).starts_with(scope))) { continue; }
            }
            let key = (stored.source.clone(), stored.external_session_id.clone());''')
    return s
edit('crates/aiks-core/src/sync/engine.rs', sync)
edit('crates/aiks-core/src/providers/qwen.rs', lambda s: s.replace('use serde_json::Value;\n', ''))
# New providers have never been released: namespace every local store consistently,
# retaining the actual upstream ID explicitly; never change identity as root counts change.
edit('crates/aiks-core/tests/multi_provider_acceptance.rs', lambda s: s.replace('assert_eq!(s.external_session_id, "s1");', 'assert_eq!(s.metadata["upstream_session_id"], "s1");'))
