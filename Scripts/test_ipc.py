"""Exercise the real plugin process without network calls or upstream credentials."""
import copy,json,os,pathlib,queue,struct,subprocess,threading,unittest
R=pathlib.Path(__file__).resolve().parent.parent
BIN=R/'backend/target/debug'/('cpr-ultrafast.exe' if os.name=='nt' else 'cpr-ultrafast')
MANIFEST=json.loads(subprocess.check_output([str(BIN),'--print-manifest']))
CONFIG={'rules':[{'name':'test','accountIds':['acct_a'],'clientKeyIds':['key_a'],'models':['gpt-6-astra','gpt-6.1-sol']}]}
class Harness:
 def __init__(self,config=None):
  self.rules=(CONFIG if config is None else config)['rules']
  self.p=subprocess.Popen([str(BIN)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
  self.q=queue.Queue();self.seq=99;self.calls=[]
  threading.Thread(target=self.reader,daemon=True).start()
  self.send({'type':'hello','handshake':{'protocol_version':6,'artifact_sha256':'a'*64,'plugin_id':'donzy.ultrafast','instance_id':'test','generation':1,'incarnation':'inc','configuration':CONFIG if config is None else config,'contributes':MANIFEST['contributes']}})
  ready,b=self.read();assert ready['type']=='ready' and not b
  self.start('plugin.register',{},b'','registration');reply,b=self.read();assert reply['type']=='result' and reply['result']['contributes']==MANIFEST['contributes']
 def reader(self):
  def exact(n):
   result=b''
   while len(result)<n:
    x=self.p.stdout.read(n-len(result))
    if not x:raise EOFError()
    result+=x
   return result
  try:
   while True:
    n,b=struct.unpack('>IQ',exact(12));m=json.loads(exact(n));payload=exact(b)
    context_bytes=m.pop('request_context_bytes',0);data_bytes=m.pop('data_bytes',0)
    if context_bytes:m['context']['request']=json.loads(payload[:context_bytes])
    if data_bytes:m['result' if m['type']=='result' else 'params']=json.loads(payload[context_bytes:context_bytes+data_bytes])
    self.q.put((m,payload[context_bytes+data_bytes:]))
  except Exception as e:self.q.put(e)
 def send(self,m,b=b''):
  m=copy.deepcopy(m);data=b''
  if m['type']=='hello':data=json.dumps(m['handshake'].pop('configuration'),separators=(',',':')).encode()
  elif m['type'] in ['call','result','callback']:data=json.dumps(m.pop('result' if m['type']=='result' else 'params'),separators=(',',':')).encode()
  if data:m['data_bytes']=len(data)
  meta=json.dumps(m,separators=(',',':')).encode();self.p.stdin.write(struct.pack('>IQ',len(meta),len(data)+len(b))+meta+data+b);self.p.stdin.flush()
 def read(self):
  v=self.q.get(timeout=8)
  if isinstance(v,Exception):raise AssertionError('plugin exited: '+self.p.stderr.read().decode(errors='replace'))
  return v
 def start(self,method,params,body,stage='attempt'):
  self.seq+=2;i=self.seq
  context={'call_id':i,'instance_id':'test','generation':1,'incarnation':'inc','stage':stage,'timeout_ms':3000 if stage=='registration' else 30000,'resource_stream':False,'resource_scope_id':'scope'+str(i)}
  if stage=='attempt':context.update(request_id=params['request_id'],attempt_id='attempt'+str(i),account_id=params['account_id'],credential_revision=1)
  self.send({'type':'call','id':i,'method':method,'context':context,'params':params},body)
  if method=='middleware.handle':self.send({'type':'credit','id':i,'bytes':8388608,'frames':16})
  return i
 def head(self,**kw):
  h={'settings_sources':{},'settings':None,'request_id':'req_'+str(self.seq),'client_key_id':'key_a','account_group_ids':['grp_a'],'mount':'attempt','attempt_index':1,'operation':'generate','protocol':'openai','endpoint':'/v1/responses','transport':'http_sse','provider':'openai','model':'gpt-6-astra','account_id':'acct_a','headers':[{'name':'x-client-test','value':list(b'unchanged')}]};h.update(kw);return h
 def next_reply(self,callback,error=None):
  if error:self.send({'type':'error','id':callback['id'],'error':error})
  else:self.send({'type':'result','id':callback['id'],'result':{'response':'original-response','protocol':'openai','status':200,'headers':[{'name':'x-upstream','value':list(b'unchanged')}],'body':{'handle':'original-stream','framing':'sse_event'}}})
 def exercise(self,body,head=None,error=None):
  i=self.start('middleware.handle',self.head() if head is None else head,body)
  callback,payload=self.read()
  if callback['type']=='error':return None,None,callback
  if callback['type']=='callback' and callback['method']=='host.state.get':
   assert callback['params']=={'namespace':'ultrafast-rules','key':'document'},callback
   self.send({'type':'result','id':callback['id'],'result':{'record':{'value':{'rules':self.rules},'version':1,'schema_version':1}}})
   callback,payload=self.read()
   if callback['type']=='error':return None,None,callback
  assert callback['type']=='callback' and callback['method']=='host.middleware.next',callback
  assert callback['parent_id']==i;self.calls.append(callback);self.next_reply(callback,error)
  result,b=self.read();assert result['id']==i and not b,result
  if result['type']=='result':
   end,b=self.read();assert end['type']=='end' and end['id']==i and end.get('error') is None and not b,end
  return callback['params'],payload,result
 def close(self):
  if self.p.poll() is None:
   self.send({'type':'quiesce'});self.send({'type':'shutdown'});self.p.stdin.close()
   try:self.p.wait(timeout=5)
   except subprocess.TimeoutExpired:self.p.kill();raise AssertionError('plugin did not stop')
  error=self.p.stderr.read();self.p.stdout.close();self.p.stderr.close();assert self.p.returncode==0,error
 def __enter__(self):return self
 def __exit__(self,*args):self.close()
class ProcessTests(unittest.TestCase):
 def test_http_and_websocket_retries_rewrite_each_request_preserve_stream(self):
  with Harness() as h:
   for transport,model in [('http_sse','gpt-6-astra'),('web_socket','gpt-6.1-sol'),('http_json','gpt-6-astra')]:
    for attempt in [1,2]:
     body=json.dumps({'model':model,'input':[],'service_tier':'priority','previous_response_id':'keep'}).encode()
     p,b,result=h.exercise(body,h.head(transport=transport,attempt_index=attempt,model=model))
     self.assertEqual(p['body'],'replace');self.assertFalse(p.get('header_mutations'));self.assertIsNone(p.get('settings'))
     parsed=json.loads(b);self.assertEqual(parsed['service_tier'],'ultrafast');self.assertEqual(parsed['previous_response_id'],'keep')
     self.assertEqual(result['result']['body'],{'kind':'pass_through','body':{'handle':'original-stream','framing':'sse_event'}})
     self.assertEqual(result['result']['response'],'original-response');self.assertFalse(result['result'].get('header_mutations'))
   self.assertEqual(len(h.calls),6)

 def test_unmatched_requests_preserve_invalid_or_arbitrary_bytes(self):
  with Harness() as h:
   for kw in [dict(client_key_id='other'),dict(account_id='other'),dict(model='gpt-6-luna'),dict(operation='compact'),dict(provider='xai')]:
    p,b,result=h.exercise(b'not parsed outside scope',h.head(**kw));self.assertEqual(p['body'],'preserve');self.assertEqual(b,b'');self.assertEqual(result['type'],'result')
 def test_empty_configuration_is_a_noop(self):
  with Harness({'rules':[]}) as h:
   p,b,_=h.exercise(b'not parsed');self.assertEqual(p['body'],'preserve');self.assertEqual(b,b'')
 def test_already_ultrafast_preserves_bytes_and_nested_tier(self):
  with Harness() as h:
   p,b,_=h.exercise(br'{"service_tier":"ultrafast", "input":{"service_tier":"default"}}');self.assertEqual(p['body'],'preserve');self.assertEqual(b,b'')
 def test_bad_matched_json_is_rejected_before_upstream(self):
  with Harness() as h:
   for body in [b'[]',b'{',br'{"service_tier":"fast","service_tier":"default"}']:
    p,b,result=h.exercise(body);self.assertIsNone(p);self.assertEqual(result['error']['code'],'invalid_input')
   self.assertEqual(h.calls,[])
 def test_upstream_error_send_state_and_details_remain_intact(self):
  error={'code':'upstream','message':'quota exceeded','send_state':'sent','http_status':429,'details':{'provider_code':'rate_limit','retry_after_ms':1000}}
  with Harness() as h:
   _,_,result=h.exercise(b'{"input":[]}',error=error);self.assertEqual(result['error'],error)
 def test_cancellation_while_waiting_for_upstream_is_forwarded(self):
  with Harness() as h:
   i=h.start('middleware.handle',h.head(),b'{"input":[]}');callback,_=h.read()
   if callback['method']=='host.state.get':
    h.send({'type':'result','id':callback['id'],'result':{'record':{'value':{'rules':h.rules},'version':1,'schema_version':1}}});callback,_=h.read()
   self.assertEqual(callback['method'],'host.middleware.next')
   h.send({'type':'cancel','id':i});events=[]
   for _ in range(5):
    message,_=h.read();events.append(message)
    if message['type']=='cancel' and message['id']==callback['id']:h.send({'type':'cancelled','id':callback['id']})
    if message['type']=='cancelled' and message['id']==i:break
   self.assertTrue(any(e['type']=='cancelled' and e['id']==i for e in events),events)
if __name__=='__main__':unittest.main(verbosity=2)




