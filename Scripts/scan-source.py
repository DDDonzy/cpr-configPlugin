"""Fail closed before publication; generated artifacts and runtime evidence are not source."""
from pathlib import Path
import subprocess,re,sys
R=Path(__file__).resolve().parent.parent
files=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard'],cwd=R,text=True).splitlines()
patterns=[r'github_pat_[A-Za-z0-9_]{20,}',r'gh[pousr]_[A-Za-z0-9]{20,}',r'sk[_-][A-Za-z0-9_\-]{35,}',r'(?i)Donzy123',r'161\.153\.30\.10',r'168\.107\.78\.155',r'(?i)\b(?:_AUTH|_VPS)\.md',r'\b(?:acct|key|grp)_01[a-z0-9]{20,}']
fail=[]
for name in files:
 p=R/name
 if not p.is_file():continue
 if p.suffix in ('.dll','.tgz') or name in ('Scripts/scan-source.py','.gitignore'):continue
 text=p.read_text(encoding='utf-8',errors='replace')
 for pattern in patterns:
  if re.search(pattern,text):fail.append((name,pattern))
if fail:
 # Show file and detector, never the matched secret.
 for name,pattern in fail:print('BLOCKED',name,pattern)
 sys.exit(1)
print('Source scan passed:',len(files),'files; no runtime credentials or deployment data detected.')
