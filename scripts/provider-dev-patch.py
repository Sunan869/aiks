"""Temporary feature-branch integration batch; remove before final merge review."""
from pathlib import Path
import re

def edit(path, fn):
    f = Path(path); old = f.read_text(encoding='utf-8'); new = fn(old)
    if new != old:
        f.write_text(new, encoding='utf-8'); print('Updated', path)
def once(s, old, new):
    if new in s: return s
    if s.count(old) != 1: raise RuntimeError('Anchor mismatch: '+old[:100])
    return s.replace(old,new,1)
edit('crates/aiks-core/src/providers/mod.rs',lambda s: s if 'pub mod settings;' in s else 'pub mod settings;\n'+s)

def engine(s):
    if 'pub provider_diagnostics:' not in s:
        s = once(s,'    pub provider_health: std::collections::HashMap<String, bool>,','    pub provider_health: std::collections::HashMap<String, bool>,\n    #[serde(default)]\n    pub provider_diagnostics: std::collections::HashMap<String, Vec<String>>,')
    if '    pub diagnostics: std::collections::HashMap<String, Vec<String>>,' not in s:
        s = once(s,'    pub summaries: Vec<SessionSummary>,','    pub summaries: Vec<SessionSummary>,\n    #[serde(default)]\n    pub diagnostics: std::collections::HashMap<String, Vec<String>>,')
        start = s.index('    pub async fn scan(&self, source_filter: Option<&str>)')
        end = s.index('    /// Run a sync operation',start)
        s = s[:start]+'''    pub async fn scan(&self, source_filter: Option<&str>) -> ScanResult {
        let mut diagnostics = std::collections::HashMap::new();
        let selected = match source_filter {
            Some(key) => match SourceKind::from_str(key) {
                Some(source) => Some(source),
                None => {
                    diagnostics.insert("selection".into(), vec!["unknown_source_filter".into()]);
                    return ScanResult { summaries: Vec::new(), by_source: Default::default(), total: 0, diagnostics };
                }
            },
            None => None,
        };
        let mut summaries = Vec::new();
        let mut by_source = std::collections::HashMap::new();
        for (source, result) in self.registry.discover_selected(selected).await {
            match result {
                Ok(report) => {
                    by_source.insert(source.display_name().to_owned(), report.sessions.len());
                    if !report.complete {
                        diagnostics.insert(source.as_str().to_owned(), report.diagnostics.into_iter().map(|d| d.code).collect());
                    }
                    summaries.extend(report.sessions);
                }
                Err(_) => { diagnostics.insert(source.as_str().to_owned(), vec!["scan_failed".into()]); }
            }
        }
        ScanResult { total: summaries.len(), summaries, by_source, diagnostics }
    }

''' + s[end:]
    s = once(s,'            provider_health,\n            db_total,','            provider_health,\n            provider_diagnostics: scan_result.diagnostics,\n            db_total,')
    if 'pub async fn provider_descriptors(' not in s:
        s += '''\nimpl AiksEngine {
    pub async fn provider_descriptors(&self) -> Vec<crate::providers::catalog::ProviderDescriptor> {
        let health = self.registry.health_check_all().await;
        crate::providers::catalog::descriptors(&self.config, &health)
    }
}\n'''
    start=s.index('        let all_ok = checks.iter().all('); end=s.index('\n\n        DoctorResult',start)
    s=s[:start]+'''        let all_ok = checks.iter().all(|c| c.ok || crate::providers::catalog::ALL_SOURCES.iter().any(|source| c.name == source.display_name()));'''+s[end:]
    return s
edit('crates/aiks-core/src/engine/mod.rs',engine)

def lib(s):
    if 'mod provider_commands;' not in s: s=once(s,'mod embedding_commands;','mod embedding_commands;\nmod provider_commands;')
    if 'provider_commands::get_source_descriptors,' not in s: s=once(s,'tauri::generate_handler![','tauri::generate_handler![\n            provider_commands::get_source_descriptors,\n            provider_commands::save_provider_settings,')
    return s
edit('apps/aiks-desktop/src-tauri/src/lib.rs',lib)

def lock_settings(s):
    if 'let _provider_config_guard' not in s:
        anchor='pub async fn save_settings('
        start=s.index(anchor); body=s.index(') -> Result<(), String> {',start)+len(') -> Result<(), String> {')
        s=s[:body]+'\n    let _provider_config_guard = crate::provider_commands::CONFIG_SAVE_LOCK.lock().await;'+s[body:]
    return s
edit('apps/aiks-desktop/src-tauri/src/commands.rs',lock_settings)
edit('apps/aiks-desktop/src-tauri/src/embedding_commands.rs',lambda s: once(s,'pub async fn save_embedding_settings(settings: EmbeddingSettings) -> Result<(), String> {','pub async fn save_embedding_settings(settings: EmbeddingSettings) -> Result<(), String> {\n    let _provider_config_guard = crate::provider_commands::CONFIG_SAVE_LOCK.lock().await;'))
edit('apps/aiks-desktop/src/api/types.ts',lambda s: once(s,'  provider_health: Record<string, boolean>;','  provider_health: Record<string, boolean>;\n  provider_diagnostics?: Record<string, string[]>;'))
edit('apps/aiks-desktop/src/main.tsx',lambda s: s if '<ProviderCatalogProvider>' in s else s.replace('import App from "./App";','import App from "./App";\nimport { ProviderCatalogProvider } from "./ProviderCatalog";').replace('<App />','<ProviderCatalogProvider><App /></ProviderCatalogProvider>'))

def page_names(s):
    if 'useSourceName' not in s:
        s=s.replace('import { formatSourceName } from "../source-display";','import { useSourceName } from "../ProviderCatalog";')
        start=s.index('export default function')
        pos=s.index('  const [',start)
        s=s[:pos]+'  const formatSourceName = useSourceName();\n'+s[pos:]
    return s
for name in ['SessionsPage','SessionDetailPage','ProcessingPage','ProcessingDetailPage','KnowledgePage']:
    edit(f'apps/aiks-desktop/src/pages/{name}.tsx',page_names)

def sessions(s):
    if 'sourceFilterOptions' not in s:
        s=s.replace('import { useSourceName } from "../ProviderCatalog";','import { useSourceName, useSourceCatalog } from "../ProviderCatalog";\nimport { sourceFilterOptions } from "../api/provider-catalog-model";')
        s=once(s,'  const sourceOptions = ["", "opencode", "claude_code", "codex", "gemini_cli"];','  const { sources, error: catalogError } = useSourceCatalog();\n  const sourceOptions = [{ value: "", label: "全部来源" }, ...sourceFilterOptions(sources)];')
        s=once(s,'{sourceOptions.map(s => (\n            <option key={s} value={s}>{s ? SOURCE_LABELS[s] ?? formatSourceName(s) : "全部来源"}</option>','{sourceOptions.map(option => (\n            <option key={option.value} value={option.value}>{option.label}</option>')
        s=once(s,'      {error && (','      {catalogError && <p role="alert" className="mb-3 text-xs text-red-600">来源筛选目录加载失败：{catalogError}</p>}\n      {error && (')
    return s
edit('apps/aiks-desktop/src/pages/SessionsPage.tsx',sessions)
edit('apps/aiks-desktop/src/api/provider-catalog.test.ts',lambda s:s.replace('for (source of fixture)','for (const source of fixture)'))

def display_test(s):
    s=s.replace('expect(sourcesSource).toMatch(/"WorkBuddy"\\s*:\\s*"workbuddy"/);','expect(sourcesSource).toContain("syncCatalogSource(getApi(), source)");')
    s=s.replace('expect(sessionsSource).toContain("value={s}");','expect(sessionsSource).toContain("value={option.value}");')
    return s
edit('apps/aiks-desktop/src/api/workbuddy-display.test.ts',display_test)

def source_test(s):
    s=s.replace('import apiTypesSource', 'import { mockSourceDescriptors } from "./provider-catalog.mock";\nimport { sourceStateLabel } from "./provider-catalog-model";\nimport apiTypesSource') if 'mockSourceDescriptors' not in s else s
    s=s.replace('expect(sourcesPageSource).toContain(\'"WorkBuddy"\');','expect(mockSourceDescriptors().find(s => s.key === "workbuddy")?.display_name).toBe("WorkBuddy");')
    s=s.replace('expect(sourcesPageSource).toMatch(/"WorkBuddy"\\s*:\\s*"workbuddy"/);','expect(sourcesPageSource).toContain("syncCatalogSource");')
    s=s.replace('expect(sourcesPageSource).toContain("fullStatus?.provider_health?.[src]");','expect(sourceStateLabel({ ...mockSourceDescriptors()[0], status: "ok" })).toBe("可读取");')
    return s
edit('apps/aiks-desktop/src/api/workbuddy-source.test.ts',source_test)
