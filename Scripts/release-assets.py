from pathlib import Path
import hashlib,json,tarfile
R=Path(__file__).resolve().parent.parent
version=json.loads((R/'plugin.json').read_text(encoding='utf-8'))['version']
files=list((R/'dist').glob(f'donzy.*-{version}-*.tar.gz'))+[R/'dist/codex-setup.bat',R/'dist/codex-setup-server-linux-arm64']
assert len(files)==3
with tarfile.open(files[0],'r:gz') as t:
 bat=t.extractfile('downloads/codex-setup.bat').read()
 assert bat==(R/'dist/codex-setup.bat').read_bytes()
 manifest=json.loads(t.extractfile('plugin.json').read())
 for n,expected in manifest['package']['files'].items():assert hashlib.sha256(t.extractfile(n).read()).hexdigest()==expected
text=''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+f.name+'\n' for f in sorted(files))
(R/'dist/SHA256SUMS').write_text(text,encoding='ascii')
print(text)
