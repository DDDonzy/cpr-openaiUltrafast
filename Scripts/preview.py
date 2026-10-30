"""Loopback-only integration preview: real plugin, fake host state, zero upstream access."""
from pathlib import Path
from http.server import HTTPServer,BaseHTTPRequestHandler
from urllib.parse import urlparse,parse_qs
import json,subprocess,threading
from test_management import ManagementHarness
R=Path(__file__).resolve().parent.parent
PORT=18949
h=ManagementHarness({'rules':[]})
h.fixture_models=['gpt-6-astra','gpt-6.1-sol','gpt-6-luna','gpt-6-sol','gpt-5.6-terra','custom/provider-model','future-model-preview']
# Fake persistence belongs only to this harness process
h.doc={'rules':[]};h.version=3
h.fixture_keys=[{'id':'key_a' if i==0 else f'key_demo_{i}','name':name,'enabled':True} for i,name in enumerate(['Primary','Team Workspace','Research','Development','Evaluation','Personal','Project Alpha','Project Beta'])]
h.fixture_accounts=[{'account_id':'acct_a' if i==0 else f'acct_demo_{i}','provider_id':'openai','name':name,'email':None,'group_ids':[],'enabled':True,'updated_at_ms':0} for i,name in enumerate(['OpenAI Primary','API Account A','API Account B','OpenAI Research','OpenAI Workspace','OpenAI Personal','Account with a deliberately long label for wrapping validation'])]
module=(R/'frontend/node_modules/@codex-proxy/ui/dist/theme/index.js').as_uri()
script=f"import {{resolveTheme,DEFAULT_THEME_COLOR,DEFAULT_CUSTOM_THEME_COLOR}} from '{module}';console.log(JSON.stringify(Object.fromEntries(['light','dark'].map(mode=>[mode,resolveTheme(mode,DEFAULT_THEME_COLOR,DEFAULT_CUSTOM_THEME_COLOR).tokens]))));"
themes=json.loads(subprocess.check_output(['node','--input-type=module','-e',script],text=True))
bridge=f'''window.codexProxyPlugin={{version:2,async request(input){{const r=await fetch('http://127.0.0.1:{PORT}/rpc',{{method:'POST',headers:{{'content-type':'application/json'}},body:JSON.stringify(input),credentials:'omit'}});return {{status:r.status,contentType:r.headers.get('content-type'),body:await r.arrayBuffer()}}}}}};'''
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,status,body,kind='application/json'):
  if isinstance(body,str):body=body.encode()
  self.send_response(status);self.send_header('Content-Type',kind+'; charset=utf-8');self.send_header('Access-Control-Allow-Origin','*');self.send_header('Access-Control-Allow-Headers','content-type');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def do_OPTIONS(self):self.reply(204,b'')
 def do_GET(self):
  u=urlparse(self.path);q=parse_qs(u.query);mode='light' if q.get('theme')==['light'] else 'dark'
  if u.path=='/embedded':
   width='390px' if q.get('narrow')==['1'] else '1600px'
   return self.reply(200,f'''<!doctype html><html><head><meta charset="utf-8"><title>Ultrafast 本地隔离验收</title><style>body{{margin:0;background:{'#f3f5f9' if mode=='light' else '#0b1018'};color:{'#172235' if mode=='light' else '#ced9e7'};font:14px sans-serif}}main{{max-width:{width};margin:auto;padding:20px}}header{{margin:0 0 20px}}iframe{{border:0;width:100%;height:1600px}}</style></head><body><main><header><h2>Ultrafast 本地隔离验收</h2><p>真实 Rust 插件 + 虚构账号和 Key · 不连接生产 CPR 或模型</p></header><iframe title="Ultrafast 验收页面" sandbox="allow-scripts" src="/?theme={mode}"></iframe></main></body></html>''','text/html')
  if u.path=='/fixture.js':return self.reply(200,bridge,'text/javascript')
  allowed={'/':'index.html','/index.html':'index.html','/app.js':'app.js','/app.css':'app.css'}
  if u.path not in allowed:return self.reply(404,'{}')
  name=allowed[u.path];data=(R/'web'/name).read_bytes()
  if name=='index.html':
   css=';'.join(f'{k}:{v}' for k,v in themes[mode].items())
   data=data.decode().replace('<head>',f'<head><style>:root{{{css}}}body{{padding:4px;box-sizing:border-box}}</style><script src="fixture.js"></script>').encode()
  self.reply(200,data,'text/html' if name.endswith('html') else 'text/javascript' if name.endswith('js') else 'text/css')
 def do_POST(self):
  n=int(self.headers.get('content-length',0))
  if not 0<=n<=1024*1024:return self.reply(413,'{}')
  body=json.loads(self.rfile.read(n))
  if self.path=='/__stop':
   self.reply(200,'{}');threading.Thread(target=server.shutdown,daemon=True).start();return
  if self.path=='/simulate-conflict':h.version=(h.version or 0)+1;return self.reply(200,'{}')
  if self.path!='/rpc':return self.reply(404,'{}')
  if body.get('path') not in ('api/snapshot','api/rules'):return self.reply(404,'{}')
  status,data=h.manage(body['path'],json.loads(body['body']) if body.get('body') else None,body.get('method'))
  self.reply(status,json.dumps(data,ensure_ascii=False))
server=HTTPServer(('127.0.0.1',PORT),Handler)
print(f'Isolated preview: http://127.0.0.1:{PORT}/embedded',flush=True)
try:server.serve_forever(poll_interval=0.2)
except KeyboardInterrupt:pass
finally:server.server_close();h.close()
