"""Exact reviewed wiring; CI emits blobs for review, never commits or moves refs."""
from pathlib import Path
import subprocess

def edit(path, expected, change):
    target = Path(path)
    actual = subprocess.check_output(['git', 'hash-object', str(target)], text=True).strip()
    assert actual == expected, (path, actual, expected)
    target.write_text(change(target.read_text()), encoding='utf-8')

def replace(text, old, new):
    assert text.count(old) == 1, (old[:80], text.count(old))
    return text.replace(old, new)

edit('crates/aiks-core/src/storage/db.rs', '1e44cc6e7643a1d994cc9e43f56c95b499834d33', lambda s: replace(s,
    '            .context("run V18 single-company identity migration")?;\n        tx.commit()?;',
    '            .context("run V18 single-company identity migration")?;\n        tx.execute_batch(include_str!("../../migrations/018_team_directory.sql"))\n            .context("run V19 atomic team directory migration")?;\n        tx.commit()?;'))

def config(s):
    s = replace(s, '    pub model_credentials: ModelCredentials,', '    pub model_credentials: ModelCredentials,\n    pub team: crate::team::config::TeamSettings,')
    return replace(s, '            model_credentials: ModelCredentials::default(),', '            model_credentials: ModelCredentials::default(),\n            team: crate::team::config::TeamSettings::default(),')
edit('apps/aiks-service/src/config.rs', 'd31cee345193dfda46eb8b3d921fd3c16e906693', config)

def bootstrap(s):
    s = replace(s, '    let mut bootstrap = false;', '    let mut bootstrap = false;\n    let mut check_only = false;')
    s = replace(s, '            "--bootstrap-stdin" if !bootstrap => bootstrap = true,', '            "--bootstrap-stdin" if !bootstrap => bootstrap = true,\n            "--check-config" if !check_only => check_only = true,')
    s = replace(s, '    anyhow::ensure!(bootstrap, "Bootstrap stdin is required");\n', '')
    s = replace(s, '    config.validate()?;', '''    if check_only {
        config.check_configuration_with(|name| std::env::var(name).ok())?;
        println!("configuration_valid");
        return Ok(());
    }
    anyhow::ensure!(bootstrap, "Bootstrap stdin is required");
    config.validate()?;''')
    return s
edit('apps/aiks-service/src/bootstrap.rs', '159fab53ba14364d6d42cb629510fe62f204e85e', bootstrap)

def main(s):
    return replace(s, '''        } else {
            // Never include raw config''', '''        } else if let Some(issue) = error.downcast_ref::<aiks_service::team::config::ConfigIssue>() {
            eprintln!("AIKS team configuration error: {issue}");
        } else {
            // Never include raw config''')
edit('apps/aiks-service/src/main.rs', '9449246a5f23ed8c7dbc57ce1eb4fa8584b3e1af', main)
