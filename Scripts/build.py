"""Build only source and generic artifacts. Requires Python, Node, Rust and Go."""
from pathlib import Path
import os,subprocess,sys,argparse
R=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--go',default='go');p.add_argument('--skip-npm-ci',action='store_true');p.add_argument('--skip-go',action='store_true');a=p.parse_args()
def run(args,cwd=R,env=None):subprocess.run(args,cwd=cwd,env=env,check=True)
run([sys.executable,'Scripts/build-bat.py'])
npm='npm.cmd' if os.name=='nt' else 'npm'
if not a.skip_npm_ci:run([npm,'ci','--ignore-scripts'],R/'frontend')
run([npm,'run','build'],R/'frontend')
run(['cargo','build','--manifest-path','backend/Cargo.toml','--locked'])
sysroot=subprocess.check_output(['rustc','--print','sysroot'],text=True).strip()
host=next(x.split(': ',1)[1] for x in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if x.startswith('host: '))
env=os.environ.copy();env['CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER']=str(Path(sysroot)/'lib/rustlib'/host/'bin'/('rust-lld.exe' if os.name=='nt' else 'rust-lld'))
run(['cargo','build','--manifest-path','backend/Cargo.toml','--release','--target','aarch64-unknown-linux-musl','--locked'],env=env)
goenv=os.environ.copy();goenv.update(CGO_ENABLED='0',GOOS='linux',GOARCH='arm64')
if not a.skip_go:run([a.go,'build','-trimpath','-ldflags=-s -w','-o',str(R/'dist/codex-setup-server-linux-arm64'),'.'],R/'codex-setup/server',goenv)
run([sys.executable,'Scripts/package.py','--binary',str(R/'backend/target/aarch64-unknown-linux-musl/release/cpr-config-plugin'),'--normalizer',str(R/'backend/target/debug'/('cpr-config-plugin.exe' if os.name=='nt' else 'cpr-config-plugin'))])
