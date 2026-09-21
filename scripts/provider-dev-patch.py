"""Temporary feature-only reconciliation of reviewed conflict hunks."""
from pathlib import Path
import subprocess
import re
MAIN='106def824e5172792bb049bd81101401f4bc7b98'
def git(*args,check=True):
    return subprocess.run(['git',*args],text=True,capture_output=True,check=check)
def edit(path,fn):
    p=Path(path); old=p.read_text(encoding='utf-8'); new=fn(old)
    if old!=new: p.write_text(new,encoding='utf-8'); print('Updated',path)
def once(s,old,new):
    if new in s:return s
    if s.count(old)!=1:raise RuntimeError('Anchor mismatch '+old[:120])
    return s.replace(old,new,1)

git('config','user.name','github-actions[bot]');git('config','user.email','41898282+github-actions[bot]@users.noreply.github.com')
if git('merge-base','--is-ancestor',MAIN,'HEAD',check=False).returncode:
    result=git('merge','--no-commit','--no-ff',MAIN,check=False)
    conflicts=git('diff','--name-only','--diff-filter=U').stdout.splitlines()
    allowed={'apps/aiks-desktop/src/pages/SessionsPage.tsx','crates/aiks-core/src/model/mod.rs','crates/aiks-core/src/providers/mod.rs','crates/aiks-core/src/sync/engine.rs'}
    if set(conflicts)!=allowed: raise RuntimeError('Conflict set changed; stop for review: '+repr(conflicts))
    pattern=r'^<<<<<<< HEAD\n(.*?)^=======\n(.*?)^>>>>>>> '+MAIN+r'\n'
    for path in conflicts:
        p=Path(path);s=p.read_text(encoding='utf-8')
        if path.endswith('SessionsPage.tsx'):
            s=git('show',MAIN+':'+path).stdout
            s=once(s,'import { formatSourceName } from "../source-display";','import { useSourceName, useSourceCatalog } from "../ProviderCatalog";\nimport { sourceFilterOptions } from "../api/provider-catalog-model";')
            s=once(s,'export default function SessionsPage({ onViewDetail }: Props) {','export default function SessionsPage({ onViewDetail }: Props) {\n  const formatSourceName = useSourceName();\n  const { sources, error: catalogError } = useSourceCatalog();')
            s,n=re.subn(r'  const sourceOptions = \[.*?\n  \];','  const sourceOptions = [{ value: "", label: "全部来源" }, ...sourceFilterOptions(sources)];',s,count=1,flags=re.S)
            if n!=1:raise RuntimeError('Missing source options')
            s=once(s,'{sourceOptions.map(s => (','{sourceOptions.map(option => (')
            s=once(s,'<option key={s} value={s}>{s ? SOURCE_LABELS[s] ?? formatSourceName(s) : "全部来源"}</option>','<option key={option.value} value={option.value}>{option.label}</option>')
            s=once(s,'      {error && (','      {catalogError && <p role="alert" className="mb-3 text-xs text-red-600">来源筛选目录加载失败：{catalogError}</p>}\n      {error && (')
            assert '导入分享链接' in s and 'getApi().importShareUrl(url)' in s
        else:
            def resolve(m):
                ours,theirs=m.group(1),m.group(2)
                if path.endswith('sync/engine.rs'):
                    assert 'let selected_provider' in ours and 'source_filter' in theirs
                    return ours
                # All inspected model/provider hunks are independent added variants/modules/factories.
                return ours+theirs
            s,n=re.subn(pattern,resolve,s,flags=re.M|re.S)
            if n==0:raise RuntimeError('No reviewed hunks found '+path)
        if '<<<<<<<' in s or '>>>>>>>' in s:raise RuntimeError('Unresolved content '+path)
        p.write_text(s,encoding='utf-8');git('add',path)
    print('Reconciled four reviewed conflict files; preserved share import and existing search updates')

def catalog(s):
    if 'pub configurable: bool' in s:return s
    s=once(s,'pub const ALL_SOURCES: [SourceKind; 16]','pub const ALL_SOURCES: [SourceKind; 19]')
    start=s.index('pub const ALL_SOURCES:');end=s.index('];',start)
    s=s[:end]+'    SourceKind::ChatgptShare, SourceKind::ClaudeShare, SourceKind::GeminiShare,\n'+s[end:]
    s=once(s,"    pub config_key: &'static str,","    pub config_key: &'static str,\n    pub configurable: bool,")
    anchor='            let (config_key, enabled, paths) ='
    if anchor not in s:raise RuntimeError('Catalog map layout changed')
    s=s.replace(anchor,'''            if matches!(source, SourceKind::ChatgptShare | SourceKind::ClaudeShare | SourceKind::GeminiShare) {
                return ProviderDescriptor { key: source.as_str(), display_name: source.display_name(), config_key: "", configurable: false, enabled: true, paths: Vec::new(), status: "managed".into(), message: "通过工作记录页导入分享链接；没有可配置的本地源目录".into() };
            }
'''+anchor,1)
    s=once(s,'                config_key,\n','                config_key,\n                configurable: true,\n')
    return s
edit('crates/aiks-core/src/providers/catalog.rs',catalog)
edit('crates/aiks-core/src/providers/settings.rs',lambda s: once(s,'    ensure!(key == source.as_str(), "Use the stable provider key");','    ensure!(key == source.as_str(), "Use the stable provider key");\n    ensure!(!matches!(source, SourceKind::ChatgptShare | SourceKind::ClaudeShare | SourceKind::GeminiShare), "Managed share sources have no local provider configuration");'))
edit('apps/aiks-desktop/src/api/provider-catalog-model.ts',lambda s: once(s,'  config_key: string;','  config_key: string;\n  configurable?: boolean;'))
edit('apps/aiks-desktop/src/pages/SourcesPage.tsx',lambda s:s.replace('{sources.map(source =>','{sources.filter(source => source.configurable !== false).map(source =>'))
def mocks(s):
    if '["chatgpt_share"' in s:return s
    s=once(s,'    ["continue", "Continue", "continue"], ["aider", "Aider", "aider"],','    ["continue", "Continue", "continue"], ["aider", "Aider", "aider"],\n    ["chatgpt_share", "ChatGPT", ""], ["claude_share", "Claude", ""], ["gemini_share", "Gemini", ""],')
    s=s.replace('config_key, enabled: true','config_key, configurable: config_key !== "", enabled: true')
    return s
edit('apps/aiks-desktop/src/api/provider-catalog.mock.ts',mocks)
edit('apps/aiks-desktop/src/api/provider-catalog.test.ts',lambda s:s.replace('all sixteen actual Core source keys','all nineteen Core keys, including the three managed share sources').replace('.size).toBe(16);','.size).toBe(19);\n    expect(fixture.filter(d => d.configurable !== false)).toHaveLength(16);'))
edit('crates/aiks-core/tests/provider_catalog.rs',lambda s:s.replace('sixteen_unique','nineteen_unique').replace('assert_eq!(keys.len(), 16);','assert_eq!(keys.len(), 19);\n    assert_eq!(catalog.iter().filter(|d| d.configurable).count(), 16);\n    assert!(patch_provider_toml("", "chatgpt_share", true, &[]).is_err());'))
