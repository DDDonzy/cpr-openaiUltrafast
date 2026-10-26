"""Build native diagnostics and a static Linux ARM64 plugin, then package it."""
from pathlib import Path
import os,subprocess,sys
R=Path(__file__).resolve().parent.parent
manifest=R/'backend/Cargo.toml'
def run(args,**kw):subprocess.run(args,cwd=R,check=True,**kw)
if os.name=='nt':subprocess.run(['npm.cmd','run','build'],cwd=R/'frontend',check=True)
else:subprocess.run(['npm','run','build'],cwd=R/'frontend',check=True)
run(['node','--check',str(R/'web/app.js')])
run(['node','--test',*[str(p) for p in sorted((R/'Scripts').glob('ui-*.test.mjs'))]])
run(['cargo','fmt','--manifest-path',str(manifest),'--','--check'])
run(['cargo','clippy','--manifest-path',str(manifest),'--all-targets','--locked','--','-D','warnings'])
run(['cargo','test','--manifest-path',str(manifest),'--locked'])
run(['cargo','build','--manifest-path',str(manifest),'--locked'])
run([sys.executable,str(R/'Scripts/test_ipc.py')])
run([sys.executable,str(R/'Scripts/test_management.py')])
env=os.environ.copy()
if os.name=='nt':
 sysroot=subprocess.check_output(['rustc','--print','sysroot'],text=True).strip()
 host=next(x.split(': ',1)[1] for x in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if x.startswith('host: '))
 env['CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER']=str(Path(sysroot)/'lib/rustlib'/host/'bin/rust-lld.exe')
run(['cargo','build','--manifest-path',str(manifest),'--release','--target','aarch64-unknown-linux-musl','--locked'],env=env)
run([sys.executable,str(R/'Scripts/package.py')])



