"""Package the SDK-normalized manifest and verified Linux ARM64 binary."""
from pathlib import Path
import gzip,hashlib,io,json,os,struct,subprocess,tarfile
R=Path(__file__).resolve().parent.parent
normalizer=R/'backend/target/debug'/('cpr-ultrafast.exe' if os.name=='nt' else 'cpr-ultrafast')
manifest=json.loads(subprocess.check_output([str(normalizer),'--print-manifest']))
binary=(R/'backend/target/aarch64-unknown-linux-musl/release/cpr-ultrafast').read_bytes()
assert binary[:6]==b'\x7fELF\x02\x01' and struct.unpack('<H',binary[18:20])[0]==183,'Not a Linux ARM64 ELF'
assert manifest['contributes']['middleware']['version']==6
resources={name:(R/name).read_bytes() for name in manifest.get('resources',{})}
files={'bin/plugin':binary,**resources}
manifest['package']={'protocolVersion':6,'target':{'os':'linux','architecture':'aarch64'},'files':{n:hashlib.sha256(d).hexdigest() for n,d in sorted(files.items())}}
contents={**files,'plugin.json':(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n').encode()}
name=f"donzy.ultrafast-{manifest['version']}-aarch64-unknown-linux-musl.tar.gz"
(R/'dist').mkdir(exist_ok=True)
out=R/'dist'/name
with out.open('wb') as f:
 with gzip.GzipFile(filename='',fileobj=f,mode='wb',mtime=0) as z:
  with tarfile.open(fileobj=z,mode='w') as t:
   for path,data in sorted(contents.items()):
    info=tarfile.TarInfo(path);info.size=len(data);info.mode=0o755 if path=='bin/plugin' else 0o644;info.mtime=0;t.addfile(info,io.BytesIO(data))
sha=hashlib.sha256(out.read_bytes()).hexdigest()
Path(str(out)+'.sha256').write_text(sha+'  '+name+'\n',encoding='ascii')
print(json.dumps({'package':str(out),'sha256':sha,'binary_bytes':len(binary)},indent=2))

