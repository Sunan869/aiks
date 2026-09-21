"""Temporary branch-only validation batch. Product changes are committed by CI."""
from pathlib import Path
import os,subprocess

def edit(path,fn):
    p=Path(path);old=p.read_text(encoding='utf-8');new=fn(old)
    if old!=new:p.write_text(new,encoding='utf-8');print('Updated',path)
def once(s,old,new):
    if new in s:return s
    if s.count(old)!=1:raise RuntimeError('Anchor mismatch '+old[:120])
    return s.replace(old,new,1)
edit('crates/aiks-core/tests/support/multi_provider_flow.rs',lambda s:s.replace('let sink = SiYuanSink::new(config.siyuan.clone());','let sink = SiYuanSink::new(config.siyuan.clone()).unwrap();'))
worker=Path('crates/aiks-core/src/pipeline/worker.rs').read_text(encoding='utf-8')
if 'let discovered = provider.discover_sessions().await?;' in worker:
    result=subprocess.run(['cargo','test','-p','aiks-core','--test','provider_runtime_regressions','--','--test-threads=1'],text=True,capture_output=True,env={**os.environ,'CARGO_TERM_COLOR':'never'})
    output=result.stdout+result.stderr
    print(output[-16000:])
    assert result.returncode!=0 and '2 failed' in output and 'a_valid_session_can_finish_pipeline_when_its_neighbor_is_damaged ... FAILED' in output and 'corrupt_cursor_database_is_not_reported_healthy ... FAILED' in output, 'Expected behavioral RED was not observed; stop rather than patch blindly'
edit('crates/aiks-core/src/pipeline/worker.rs',lambda s:once(s,'    let discovered = provider.discover_sessions().await?;', '''    let report = provider.discover_report().await?;
    if !report.complete {
        warn!(source = source.as_str(), diagnostics = report.diagnostics.len(), "Incomplete provider snapshot; preserving valid pipeline sessions");
    }
    let discovered = report.sessions;'''))
def health(s):
    if 'Cursor database schema is unreadable' in s:return s
    anchor='                    if self.source != SourceKind::Antigravity {'
    addition='''                    if self.source == SourceKind::Cursor {
                        let paths = match local_paths::candidates(&io, self.source) {
                            Ok(paths) => paths,
                            Err(_) => return ProviderHealth::Error { message: "Cursor database discovery is incomplete".into() },
                        };
                        if let Some(path) = paths.first() {
                            let schema = io.open_readonly(path).and_then(|conn| {
                                conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('cursorDiskKV','ItemTable')", [], |r| r.get::<_, i64>(0)).map_err(Into::into)
                            });
                            match schema {
                                Ok(0) => return ProviderHealth::Unsupported { message: "Cursor database schema is unsupported".into() },
                                Err(_) => return ProviderHealth::Error { message: "Cursor database schema is unreadable".into() },
                                Ok(_) => {}
                            }
                        }
                    }
'''
    return once(s,anchor,addition+anchor)
edit('crates/aiks-core/src/providers/native.rs',health)
# SQL byte limits must count UTF-8 bytes even when upstream stored the value as TEXT.
for file in ['cursor/mod.rs','cline_family.rs']:
    edit('crates/aiks-core/src/providers/'+file,lambda s:s.replace('length(value)','length(CAST(value AS BLOB))'))
