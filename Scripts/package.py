"""Package the normalized native manifest and immutable resources; never include data."""
from pathlib import Path
import argparse,subprocess,json,hashlib,tarfile,io,gzip,os
R=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--normalizer',required=True);a=p.parse_args()
manifest=json.loads(subprocess.check_output([a.normalizer,'--print-manifest']))
files={'bin/plugin':Path(a.binary)}
for page in ('routing','models'):
 for name in ('index.html','app.js','app.css'):files[f'web/{page}/{name}']=R/'frontend/dist'/page/name
files['downloads/codex-setup.bat']=R/'dist/codex-setup.bat'
assert set(manifest['resources'])==set(files)-{'bin/plugin'}
assert files['downloads/codex-setup.bat'].read_bytes()==(R/'codex-setup/dist/codex-setup.bat').read_bytes()
manifest['package']={'protocolVersion':6,'target':{'os':'linux','architecture':'aarch64'},'files':{n:hashlib.sha256(f.read_bytes()).hexdigest() for n,f in sorted(files.items())}}
name=f"{manifest['publisher']}.{manifest['name']}-{manifest['version']}-aarch64-unknown-linux-gnu.tar.gz"
out=R/'dist'/name;contents={'plugin.json':(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n').encode(),**{n:f.read_bytes() for n,f in files.items()}}
with out.open('wb') as dst:
 with gzip.GzipFile(filename='',mode='wb',fileobj=dst,mtime=0) as gz:
  with tarfile.open(fileobj=gz,mode='w') as tar:
   for path,data in sorted(contents.items()):
    info=tarfile.TarInfo(path);info.size=len(data);info.mode=0o755 if path=='bin/plugin' else 0o644;info.mtime=0;tar.addfile(info,io.BytesIO(data))
sha=hashlib.sha256(out.read_bytes()).hexdigest();Path(str(out)+'.sha256').write_text(sha+'  '+name+'\n',encoding='ascii');print(out);print(sha)
