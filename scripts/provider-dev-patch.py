"""Temporary feature-only merge probe. Never pushes unresolved merges."""
from pathlib import Path
import subprocess

MAIN = '106def824e5172792bb049bd81101401f4bc7b98'
def git(*args, check=True):
    return subprocess.run(['git', *args], text=True, capture_output=True, check=check)
git('config','user.name','github-actions[bot]')
git('config','user.email','41898282+github-actions[bot]@users.noreply.github.com')
if git('merge-base','--is-ancestor',MAIN,'HEAD',check=False).returncode:
    result = git('merge','--no-commit','--no-ff',MAIN,check=False)
    print(result.stdout, result.stderr)
    conflicts = git('diff','--name-only','--diff-filter=U').stdout.splitlines()
    for path in conflicts:
        print('\nCONFLICT:', path)
        lines = Path(path).read_text(encoding='utf-8').splitlines()
        active = False
        for line in lines:
            if line.startswith('<<<<<<<'): active = True
            if active: print(line)
            if line.startswith('>>>>>>>'): active = False
    if conflicts or result.returncode:
        raise SystemExit('Merge probe stopped for explicit conflict review; no branch changes pushed')
