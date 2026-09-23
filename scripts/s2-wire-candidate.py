"""Apply two SHA-guarded auth wires; CI emits blobs and never updates a branch."""
from pathlib import Path
import subprocess

def edit(path, expected, transform):
    actual = subprocess.check_output(['git', 'hash-object', path], text=True).strip()
    assert actual == expected, (path, actual, expected)
    p = Path(path)
    p.write_text(transform(p.read_text()))

def migration(text):
    old = '            .context("run V19 atomic team directory migration")?;'
    assert text.count(old) == 1
    return text.replace(old, old + '\n        tx.execute_batch(include_str!("../../migrations/019_team_auth_sessions.sql"))\n            .context("run team authentication migration")?;')

edit('crates/aiks-core/src/storage/db.rs', '12d7390a702f79b80444db737d8a4f7731ceaa41', migration)

def authorize(text):
    start = text.index('    pub fn authorize_url(&self, state: &str) -> Result<String, TeamError> {')
    end = text.index('    pub async fn exchange_code', start)
    old = text[start:end]
    assert old.count('self.settings.settings().dingtalk') == 1
    helper = old.replace('    pub fn authorize_url(&self, state: &str) -> Result<String, TeamError> {',
        '    pub(crate) fn authorize_url(settings: &ValidatedTeamSettings, state: &str) -> Result<String, TeamError> {')
    helper = helper.replace('self.settings.settings().dingtalk', 'settings.settings().dingtalk')
    helper = '\n'.join(line[4:] if line.startswith('    ') else line for line in helper.splitlines())
    return text[:start] + '    pub fn authorize_url(&self, state: &str) -> Result<String, TeamError> {\n        authorize_url(&self.settings, state)\n    }\n\n' + text[end:] + '\n' + helper.rstrip() + '\n'

edit('apps/aiks-service/src/team/dingtalk/mod.rs', 'ba5fc35ee010c617fd64b89af51424b04ccc5c52', authorize)
print('Applied exact auth wiring candidate; canonical branch verification is still required.')
