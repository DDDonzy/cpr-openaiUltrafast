"""Exercise management/state behavior through the real plugin binary, never production."""
import copy,json,unittest
from test_ipc import Harness,MANIFEST,CONFIG

BINDING={'contribution':'donzy.ultrafast.middleware','stage':'attempt','clientKeyIds':['key_a'],'accountGroupIds':[],'providerIds':['openai'],'models':['gpt-6-astra','gpt-6.1-sol'],'identityBindings':[]}

class ManagementHarness(Harness):
 def __init__(self,config=None):
  super().__init__(config)
  self.doc=None;self.version=None;self.bodies={};self.bindings=[copy.deepcopy(BINDING)];self.history=[]
  self.fixture_keys=[{'id':'key_a','name':'验收 Key','enabled':True},{'id':'key_b','name':'未绑定的 Key','enabled':True}]
  self.fixture_accounts=[{'account_id':'acct_a','provider_id':'openai','name':'验收账号','email':'demo@example.test','group_ids':[],'enabled':True,'updated_at_ms':0}]
 def manage(self,path,value=None,method=None):
  method=method or ('GET' if value is None else 'POST')
  params={'method':method,'path':path,'query':'','content_type':None if value is None else 'application/json','headers':[]}
  i=self.start('management.handle',params,b'' if value is None else json.dumps(value).encode(),stage='management')
  meta,payload=self.read()
  while meta.get('type')=='callback':
   self.history.append(meta['method']);result={};payload=b''
   if meta['method']=='host.state.get':
    result={'record':None if self.doc is None else {'value':self.doc,'version':self.version,'schema_version':1}}
   elif meta['method']=='host.state.put':
    assert meta['params']['namespace']=='ultrafast-rules'
    if meta['params']['expected_version']!=self.version:
     self.send({'type':'error','id':meta['id'],'error':{'code':'conflict','message':'stale','send_state':'not_sent'}});meta,payload=self.read();continue
    self.doc=meta['params']['value'];self.rules=self.doc['rules'];self.version=(self.version or 0)+1;result={'version':self.version}
   elif meta['method']=='host.keys.list':
    result={'keys':self.fixture_keys,'next_cursor':None}
   elif meta['method']=='host.data.accounts.list':
    payload=json.dumps({'schema_version':1,'next_cursor':None,'accounts':self.fixture_accounts}).encode()
   elif meta['method']=='host.http.dispatch':
    assert meta['params']['uri']=='/api/admin/plugins/instances',meta
    self.bodies['inst']=json.dumps({'code':200,'data':[{'id':'test','enabled':True,'running':True,'artifactSha256':'b'*64,'compatibilityWarning':None,'loadError':None,'revision':7,'bindings':self.bindings,'runtime':{'status':'running','drainingRevisions':[]}}]}).encode()
    result={'status':200,'version':'HTTP/1.1','headers':[],'body':{'kind':'handle','handle':'inst'},'response':None,'session':False}
   elif meta['method']=='host.http.body_read':
    payload=self.bodies.get('inst',b'');self.bodies['inst']=b'';result={'eof':not bool(payload),'trailers':None}
   elif meta['method']!='host.http.body_close':raise AssertionError(meta)
   self.send({'type':'result','id':meta['id'],'result':result},payload)
   meta,payload=self.read()
  assert meta['type']=='result' and meta['id']==i,meta
  return meta['result']['status'],json.loads(payload)

class ManagementTests(unittest.TestCase):
 def test_snapshot_separates_package_version_and_state_version(self):
  with ManagementHarness() as h:
   status,j=h.manage('api/snapshot')
   self.assertEqual(status,200);self.assertEqual(j['rules'],h.rules)
   self.assertEqual(j['pluginVersion'],MANIFEST['version']);self.assertIsNone(j['version'])
   self.assertEqual(j['keys'][0]['id'],'key_a');self.assertEqual(j['accounts'][0]['account_id'],'acct_a')
   self.assertEqual(j['instance']['bindings'],h.bindings);self.assertTrue(j['instance']['running'])
 def test_save_and_clear_persist_with_exact_versions(self):
  with ManagementHarness() as h:
   status,j=h.manage('api/rules',{'rules':CONFIG['rules'],'expectedVersion':None})
   self.assertEqual(status,200);self.assertEqual(j['version'],1);self.assertEqual(j['rules'],CONFIG['rules'])
   status,s=h.manage('api/snapshot');self.assertEqual(status,200);self.assertEqual(s['rules'],j['rules'])
   status,j=h.manage('api/rules',{'rules':[],'expectedVersion':1});self.assertEqual(status,200)
   self.assertEqual(j,{'version':2,'rules':[]});self.assertEqual(h.doc,{'rules':[]})
 def test_malformed_scope_cannot_be_silently_expanded(self):
  for field,value in [('accountIds','acct_a'),('accountIds',None),('clientKeyIds',17),('extra',True),('accountIds',['acct_a','acct_a'])]:
   with self.subTest(field=field,value=value),ManagementHarness() as h:
    rule=copy.deepcopy(CONFIG['rules'][0]);rule[field]=value
    status,_=h.manage('api/rules',{'rules':[rule],'expectedVersion':None});self.assertEqual(status,400)
    self.assertIsNone(h.doc);self.assertNotIn('host.state.put',h.history)
 def test_invalid_or_missing_cas_rejected(self):
  cases=[{'rules':CONFIG['rules'],'expectedVersion':v} for v in ['1',-1,1.5,True]]
  cases += [{'rules':CONFIG['rules']},{'rules':CONFIG['rules'],'expectedVersion':None,'unknown':True}]
  for body in cases:
   with self.subTest(body=body),ManagementHarness() as h:
    status,_=h.manage('api/rules',body);self.assertEqual(status,400);self.assertIsNone(h.doc)
 def test_scope_models_names_and_document_size_are_bounded(self):
  large=[{'name':str(i),'clientKeyIds':[('key_'+str(j)).ljust(128,'x') for j in range(128)],'accountIds':[],'models':['gpt-6-astra']} for i in range(5)]
  invalid=[[],['gpt-6-luna'],['gpt-6-astra','gpt-6-astra']]
  cases=[dict(CONFIG['rules'][0],models=models) for models in invalid]
  cases += [dict(CONFIG['rules'][0],name=' '),dict(CONFIG['rules'][0],name='x\n'),dict(CONFIG['rules'][0],name='x'*129),dict(CONFIG['rules'][0],accountIds=[],clientKeyIds=[])]
  for rules in [[r] for r in cases]+[large,CONFIG['rules']*65]:
   with self.subTest(rules=len(rules)),ManagementHarness() as h:
    status,_=h.manage('api/rules',{'rules':rules,'expectedVersion':None});self.assertEqual(status,400);self.assertIsNone(h.doc)
 def test_binding_and_identity_mismatches_do_not_save(self):
  for change in [dict(clientKeyIds=['key_b']),dict(clientKeyIds=[]),dict(accountIds=['acct_missing'])]:
   with self.subTest(change=change),ManagementHarness() as h:
    rule=dict(CONFIG['rules'][0],**change)
    status,j=h.manage('api/rules',{'rules':[rule],'expectedVersion':None});self.assertEqual(status,400,j)
    self.assertIsNone(h.doc)
  with ManagementHarness() as h:
   h.bindings=[]
   self.assertEqual(h.manage('api/rules',{'rules':CONFIG['rules'],'expectedVersion':None})[0],400)
   self.assertEqual(h.manage('api/rules',{'rules':[],'expectedVersion':None})[0],200)
 def test_account_only_rule_requires_unrestricted_key_binding(self):
  with ManagementHarness() as h:
   h.bindings[0]['clientKeyIds']=[]
   self.assertEqual(h.manage('api/rules',{'rules':[dict(CONFIG['rules'][0],clientKeyIds=[])],'expectedVersion':None})[0],200)
 def test_conflict_never_overwrites_persisted_rules(self):
  with ManagementHarness() as h:
   self.assertEqual(h.manage('api/rules',{'rules':CONFIG['rules'],'expectedVersion':None})[0],200)
   status,j=h.manage('api/rules',{'rules':[],'expectedVersion':None})
   self.assertEqual(status,409);self.assertEqual(j['error']['code'],'conflict');self.assertEqual(h.doc,CONFIG)
 def test_clear_rules_invalidates_warmed_cache(self):
  with ManagementHarness() as h:
   p,_,_=h.exercise(b'{"input":[]}');self.assertEqual(p['body'],'replace')
   self.assertEqual(h.manage('api/rules',{'rules':[],'expectedVersion':None})[0],200)
   p,_,_=h.exercise(b'not parsed');self.assertEqual(p['body'],'preserve')
 def test_failed_save_invalidates_cache_and_rereads(self):
  with ManagementHarness() as h:
   h.exercise(b'{"input":[]}');h.doc={'rules':[]};h.rules=[];h.version=2
   self.assertEqual(h.manage('api/rules',{'rules':CONFIG['rules'],'expectedVersion':1})[0],409)
   p,_,_=h.exercise(b'not parsed');self.assertEqual(p['body'],'preserve')
 def test_empty_persisted_state_does_not_restore_legacy_rules(self):
  with ManagementHarness() as h:
   h.doc={'rules':[]};h.version=1
   self.assertEqual(h.manage('api/snapshot')[1]['rules'],[])
   p,_,_=h.exercise(b'not parsed');self.assertEqual(p['body'],'preserve')
if __name__=='__main__':unittest.main(verbosity=2)

