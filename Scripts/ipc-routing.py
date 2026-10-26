import os
"""真实插件子进程的 SDK 握手、受管回调、写入范围和生命周期测试，不访问真实服务器"""
import base64,zlib,pathlib,json,struct,subprocess,threading,queue,copy,unittest,time
ROOT=pathlib.Path(__file__).resolve().parent.parent;BIN=pathlib.Path(os.environ.get('CPR_TEST_PLUGIN_BIN',str(ROOT/'backend/target/debug'/('cpr-config-plugin.exe' if os.name=='nt' else 'cpr-config-plugin'))))
MANIFEST=json.loads(subprocess.check_output([str(BIN),'--print-manifest']))
POLICY={'mode':'denylist','models':['claude-one']}
ACCOUNT={'id':'acct_test_gpt','name':'GPT','provider':'openai','enabled':True,'status':'normal','modelAccess':copy.deepcopy(POLICY),'apiKey':'DO_NOT_EXPOSE_SENTINEL','concurrencyLimit':3,'weight':1}
class Harness:
 def __init__(self,doc=None,version=None,fail_put=False,fail_native=False):
  self.proc=subprocess.Popen([str(BIN)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  self.frames=queue.Queue();self.read_error=None;self.seq=99;self.accounts=[copy.deepcopy(ACCOUNT)];self.doc=doc;self.version=version;self.fail_put=fail_put;self.fail_native=fail_native;self.callbacks=[];self.http_requests=[];self.writes=[];self.bodies={};self.body_seq=0
  threading.Thread(target=self.reader,daemon=True).start()
  self.send({'type':'hello','handshake':{'protocol_version':6,'artifact_sha256':'a'*64,'plugin_id':'donzy.model-routing-groups','instance_id':'test-instance','generation':1,'incarnation':'test-incarnation','configuration':{},'contributes':MANIFEST['contributes']}})
  ready,payload=self.read();assert ready['type']=='ready' and ready['protocol_version']==6 and not payload
  self.call('plugin.register',{},b'','registration')
 def reader(self):
  try:
   def exact(n):
    data=b''
    while len(data)<n:
     part=self.proc.stdout.read(n-len(data))
     if not part:raise EOFError()
     data+=part
    return data
   while True:
    head=exact(12);n,b=struct.unpack('>IQ',head);meta=json.loads(exact(n));payload=exact(b)
    request_bytes=meta.pop('request_context_bytes',0);data_bytes=meta.pop('data_bytes',0)
    if request_bytes:meta['context']['request']=json.loads(payload[:request_bytes])
    data=payload[request_bytes:request_bytes+data_bytes];payload=payload[request_bytes+data_bytes:]
    if data_bytes:
     value=json.loads(data)
     if meta['type']=='hello':meta['handshake']['configuration']=value
     else:meta['result' if meta['type']=='result' else 'params']=value
    self.frames.put((meta,payload))
  except Exception as e:self.frames.put(e)
 def send(self,meta,payload=b''):
  meta=copy.deepcopy(meta);business=b''
  if meta['type']=='hello':business=json.dumps(meta['handshake'].pop('configuration'),separators=(',',':')).encode()
  elif meta['type'] in ['call','callback','result']:business=json.dumps(meta.pop('result' if meta['type']=='result' else 'params'),separators=(',',':'),ensure_ascii=False).encode()
  if business:meta['data_bytes']=len(business)
  data=json.dumps(meta,separators=(',',':'),ensure_ascii=False).encode();assert len(data)<=65536;self.proc.stdin.write(struct.pack('>IQ',len(data),len(business)+len(payload))+data+business+payload);self.proc.stdin.flush()
 def read(self):
  f=self.frames.get(timeout=5)
  if isinstance(f,Exception):raise RuntimeError('Plugin exited unexpectedly: '+self.proc.stderr.read().decode(errors='replace')) from f
  return f
 def callback(self,meta,payload):
  method=meta['method'];params=meta['params'];self.callbacks.append(method);result=None;body=b'';error=None
  if method=='host.state.get':
   result={'record':None if self.doc is None else {'value':{'encoding':'zlib-base64','data':base64.b64encode(zlib.compress(json.dumps(self.doc).encode())).decode()},'version':self.version,'schema_version':1}}
  elif method=='host.state.put':
   if self.fail_put:error={'code':'conflict','message':'test conflict','send_state':'not_sent'}
   else:
    assert params['expected_version']==self.version
    self.doc=json.loads(zlib.decompress(base64.b64decode(params['value']['data'])));self.version=(self.version or 0)+1;result={'version':self.version}
  elif method=='host.http.dispatch':
   uri=params['uri'];verb=params['method'];self.http_requests.append((verb,uri));headers=params['headers'];assert all(h['name'] not in ['authorization','cookie'] for h in headers)
   assert uri.startswith('/api/admin/accounts')
   code=200;status=200;data=None
   if verb=='GET' and uri.startswith('/api/admin/accounts?'):data={'items':copy.deepcopy(self.accounts),'page':{},'summary':{}}
   elif verb=='POST' and uri=='/api/admin/accounts/batch-update':
    patch=json.loads(payload);self.writes.append(patch);assert set(patch)=={'accountIds','modelAccess'}
    if self.fail_native:code=1001;status=503
    else:
     for account in self.accounts:
      if account['id'] in patch['accountIds']:account['modelAccess']=copy.deepcopy(patch['modelAccess'])
    data={'updated':1}
   elif uri.startswith('/api/admin/accounts/models?') or uri=='/api/admin/accounts/models/refresh':
    if 'acct_second' in uri or (verb=='POST' and json.loads(payload).get('accountId')=='acct_second'):data={'models':[{'id':'claude-two'},{'id':'custom-image'},{'id':'__not_public'}]}
    elif 'acct_failed' in uri or (verb=='POST' and json.loads(payload).get('accountId')=='acct_failed'):code=50301;status=503
    else:data={'models':[{'id':'claude-one'},{'id':'custom-image'},{'id':'__not_public'}]}
   elif verb=='GET' and uri.startswith('/api/admin/accounts/detail?accountId='):
    aid=uri.split('=',1)[1];cfg=getattr(self,'details',{}).get(aid)
    if cfg=='FAIL':code=50001;status=500
    else:data={'account':{'id':aid,'apiKey':'DETAIL_KEY_SENTINEL'},'credentialConfiguration':cfg}
   else:raise AssertionError((verb,uri))
   self.body_seq+=1;handle='body_'+str(self.body_seq);self.bodies[handle]=json.dumps({'code':code,'message':'','data':data}).encode()
   result={'status':status,'version':'HTTP/1.1','headers':[],'body':{'kind':'handle','handle':handle},'response':None,'session':False}
  elif method=='host.http.body_read':
   data=self.bodies[params['handle']]
   if data:self.bodies[params['handle']]=b'';result={'eof':False,'trailers':None};body=data
   else:result={'eof':True,'trailers':None}
  elif method=='host.http.body_close':result={}
  else:raise AssertionError('Unexpected callback '+method)
  if error:self.send({'type':'error','id':meta['id'],'error':error})
  else:self.send({'type':'result','id':meta['id'],'result':result},body)
 def context(self,id,stage):return {'call_id':id,'instance_id':'test-instance','generation':1,'incarnation':'test-incarnation','stage':stage,'timeout_ms':3000 if stage=='registration' else 30000,'resource_stream':False,'resource_scope_id':'scope_'+str(id)}
 def call(self,method,params,payload=b'',stage='management'):
  self.seq+=2;id=self.seq;self.send({'type':'call','id':id,'method':method,'context':self.context(id,stage),'params':params},payload)
  while True:
   meta,body=self.read()
   if meta['type']=='callback':self.callback(meta,body)
   elif meta['type']=='credit':continue
   elif meta['type']=='result' and meta['id']==id:return meta['result'],body
   elif meta['type']=='error' and meta['id']==id:raise AssertionError(meta)
   else:raise AssertionError(meta)
 def management(self,path,value=None,method=None):
  method=method or ('GET' if value is None else 'POST');params={'method':method,'path':path,'query':'','content_type':None if value is None else 'application/json','headers':[]}
  result,payload=self.call('management.handle',params,b'' if value is None else json.dumps(value).encode());return result['status'],json.loads(payload)
 def close(self):
  if self.proc.poll() is None:
   self.send({'type':'quiesce'});self.send({'type':'shutdown'});self.proc.stdin.close()
   try:self.proc.wait(timeout=5)
   except subprocess.TimeoutExpired:self.proc.kill();raise AssertionError('Shutdown did not drain')
  assert self.proc.returncode==0,self.proc.stderr.read().decode(errors='replace')
 def __enter__(self):return self
 def __exit__(self,*_):self.close()
def request(models=['claude-one','custom-image'],**kw):
 r={'accountId':'acct_test_gpt','before':copy.deepcopy(POLICY),'after':{'mode':'denylist','models':list(models)},'groupIds':[],'manualModels':list(models),'expectedGroupVersion':None};r.update(kw);return r
class Tests(unittest.TestCase):
 def test_oversized_draft_preview_succeeds_apply_rejects_without_side_effects(self):
  with Harness() as h:
   req=request([f'model-{i}' for i in range(500)])
   status,s=h.management('api/preview',req);self.assertEqual(status,200);self.assertEqual(len(s['diff']['after']['models']),500);self.assertEqual(h.writes,[])
   status,s=h.management('api/apply',req);self.assertEqual(status,400);self.assertIn('256',s['error']['message']);self.assertEqual(h.writes,[]);self.assertIsNone(h.doc)
 def test_oversized_group_rejected_only_at_save(self):
  with Harness() as h:
   status,s=h.management('api/groups',{'expectedVersion':None,'groups':[{'id':'large','name':'Large','models':[f'm{i}' for i in range(257)]}]});self.assertEqual(status,400);self.assertIsNone(h.doc);self.assertEqual(h.writes,[])
 def test_oversized_group_union_previews_but_cannot_apply(self):
  groups=[{'id':'a','name':'one','models':[f'a{i}' for i in range(200)]},{'id':'b','name':'two','models':[f'b{i}' for i in range(200)]}]
  with Harness({'groups':groups,'bindings':{}},1) as h:
   req=request(groups[0]['models']+groups[1]['models'],groupIds=['a','b'],manualModels=[],expectedGroupVersion=1)
   status,s=h.management('api/preview',req);self.assertEqual(status,200)
   status,s=h.management('api/apply',req);self.assertEqual(status,400);self.assertEqual(h.writes,[]);self.assertEqual(h.version,1)

 def test_global_load_refreshes_every_account_no_policy_write(self):
  with Harness() as h:
   a=copy.deepcopy(ACCOUNT);a['id']='acct_second';a['name']='Second';h.accounts.append(a)
   status,s=h.management('api/catalog-all',{'refresh':True});self.assertEqual(status,200);self.assertEqual(s['models'],['claude-one','claude-two','custom-image']);self.assertEqual(h.http_requests.count(('POST','/api/admin/accounts/models/refresh')),2);self.assertFalse(any('/models?' in uri for verb,uri in h.http_requests));self.assertEqual(h.writes,[]);self.assertIsNone(h.doc)
 def test_global_refresh_rejects_unknown_fields(self):
  with Harness() as h:
   status,s=h.management('api/catalog-all',{'refresh':True,'url':'https://untrusted.invalid'});self.assertEqual(status,400);self.assertEqual(h.http_requests,[])

 def test_global_catalog_union_no_policy_filter_no_mutations(self):
  with Harness() as h:
   a=copy.deepcopy(ACCOUNT);a['id']='acct_second';a['name']='Second';a['enabled']=False;a['modelAccess']={'mode':'allowlist','models':['other-model']};h.accounts.append(a)
   status,s=h.management('api/catalog-all');self.assertEqual(status,200);self.assertEqual(s['models'],['claude-one','claude-two','custom-image']);self.assertEqual(s['accountCount'],2);self.assertEqual(s['warnings'],[]);self.assertEqual(h.writes,[]);self.assertIsNone(h.doc)
 def test_global_catalog_partial_warning_not_silent_success(self):
  with Harness() as h:
   a=copy.deepcopy(ACCOUNT);a['id']='acct_failed';a['name']='Unavailable';h.accounts.append(a)
   status,s=h.management('api/catalog-all');self.assertEqual(status,200);self.assertEqual(len(s['warnings']),1);self.assertIn('Unavailable',s['warnings'][0]);self.assertEqual(s['models'],['claude-one','custom-image']);self.assertEqual(h.writes,[])
 def test_global_catalog_unknown_method_rejected(self):
  with Harness() as h:
   status,s=h.management('api/catalog-all',{},method='DELETE');self.assertEqual(status,404);self.assertEqual(h.writes,[])

 def test_snapshot_projects_identity_without_tokens(self):
  with Harness() as h:
   a=copy.deepcopy(ACCOUNT);a.update({'email':'person@example.test','authenticationKind':'oauth','planType':'pro','planTypeDisplay':'Pro','accessToken':'TOKEN_SENTINEL','refreshToken':'REFRESH_SENTINEL','userId':'USER_SENTINEL','groups':[{'id':'grp_test','name':'DEFAULT','color':'#05FF0080','enabled':True}],'usage':{'lastUsedAt':'2026-10-22T03:00:00Z','requestCount':5},'quota':{'windows':[{'key':'QUOTA_SENTINEL'}]}})
   h.accounts=[a]
   status,s=h.management('api/snapshot');self.assertEqual(status,200);text=json.dumps(s,ensure_ascii=False)
   for x in ['DO_NOT_EXPOSE_SENTINEL','TOKEN_SENTINEL','REFRESH_SENTINEL','USER_SENTINEL','QUOTA_SENTINEL']:self.assertNotIn(x,text)
   ident=s['accounts'][0]['identity'];self.assertEqual(ident['email'],'person@example.test');self.assertEqual(ident['planTypeDisplay'],'Pro');self.assertEqual(ident['groups'][0]['name'],'DEFAULT');self.assertEqual(ident['lastUsedAt'],'2026-10-22T03:00:00Z');self.assertEqual(h.writes,[])
 def test_upstreams_reads_only_api_key_details_and_sanitizes(self):
  with Harness() as h:
   o=copy.deepcopy(ACCOUNT);o.update({'id':'acct_oauth','email':'person@example.test','authenticationKind':'oauth'})
   rows=[o]
   for aid,name in [('acct_key','Relay'),('acct_bad','Bad'),('acct_down','Down')]:
    k=copy.deepcopy(ACCOUNT);k.update({'id':aid,'name':name,'authenticationKind':'api_key'});rows.append(k)
   h.accounts=rows
   h.details={'acct_key':{'base_url':'HTTPS://Relay.Example.test:8443/PATH_SECRET/v1?token=QUERY_SECRET#frag','transport':'prefer_websocket'},'acct_bad':{'base_url':'https://user:PASS_SECRET@relay.example.test/v1','transport':'http'},'acct_down':'FAIL'}
   status,s=h.management('api/upstreams');self.assertEqual(status,200);text=json.dumps(s)
   for x in ['PATH_SECRET','QUERY_SECRET','PASS_SECRET','DETAIL_KEY_SENTINEL','DO_NOT_EXPOSE_SENTINEL','frag']:self.assertNotIn(x,text)
   u=s['upstreams'];self.assertEqual(u['acct_oauth'],{'kind':'oauth','status':'ok'})
   self.assertEqual(u['acct_key']['address'],{'url':'https://relay.example.test:8443','host':'relay.example.test:8443','loopback':False});self.assertEqual(u['acct_key']['transport'],'prefer_websocket')
   self.assertEqual(u['acct_bad']['status'],'unavailable');self.assertNotIn('address',u['acct_bad']);self.assertEqual(u['acct_down']['status'],'unavailable')
   self.assertEqual(len(s['warnings']),1);self.assertIn('2',s['warnings'][0])
   details=sorted(uri for verb,uri in h.http_requests if '/detail?' in uri)
   self.assertEqual(details,['/api/admin/accounts/detail?accountId=acct_bad','/api/admin/accounts/detail?accountId=acct_down','/api/admin/accounts/detail?accountId=acct_key'])
   self.assertTrue(all(verb=='GET' for verb,uri in h.http_requests));self.assertEqual(h.writes,[]);self.assertIsNone(h.doc)
 def test_upstreams_get_rejects_body(self):
  with Harness() as h:
   status,s=h.management('api/upstreams',{'x':1},method='GET');self.assertEqual(status,400);self.assertEqual(h.http_requests,[])
 def test_preview_names_account_by_identity(self):
  with Harness() as h:
   h.accounts[0].update({'email':'person@example.test','authenticationKind':'oauth'})
   status,s=h.management('api/preview',request());self.assertEqual(status,200);self.assertEqual(s['diff']['accountName'],'person@example.test')
 def test_registration_and_snapshot_redaction(self):
  with Harness() as h:
   reg,p=h.call('management.register',{},b'','registration');data=json.loads(p);self.assertEqual(len(data['pages']),2);self.assertEqual(len(data['routes']),13)
   status,s=h.management('api/snapshot');self.assertEqual(status,200);self.assertNotIn('DO_NOT_EXPOSE_SENTINEL',json.dumps(s));self.assertEqual(s['accounts'][0]['modelAccess'],POLICY);self.assertEqual(h.writes,[])
 def test_large_groups_return_binary_snapshot_below_metadata_limit(self):
  groups=[{'id':f'g{i}','name':f'group{i}','models':[f'vendor-model-family-long-prefix-{i}-{j}' for j in range(256)]} for i in range(16)]
  doc={'groups':groups,'bindings':{}}
  self.assertGreater(len(json.dumps(doc).encode()),65536)
  with Harness(doc,1) as h:
   status,result=h.management('api/snapshot');self.assertEqual(status,200);self.assertEqual(len(result['state']['value']['groups']),16);self.assertEqual(h.writes,[])
 def test_preview_has_no_side_effect(self):
  with Harness() as h:
   status,s=h.management('api/preview',request());self.assertEqual(status,200);self.assertEqual(s['diff']['added'],['custom-image']);self.assertEqual(h.writes,[]);self.assertIsNone(h.doc)
 def test_apply_only_native_model_access(self):
  with Harness() as h:
   status,s=h.management('api/apply',request());self.assertEqual(status,200);self.assertTrue(s['nativeApplied']);self.assertEqual(len(h.writes),1);self.assertEqual(set(h.writes[0]),{'accountIds','modelAccess'});self.assertEqual(h.accounts[0]['concurrencyLimit'],3);self.assertIn('acct_test_gpt',h.doc['bindings'])
 def test_association_only_does_not_rewrite_native_policy(self):
  doc={'groups':[{'id':'g1','name':'Claude','models':['claude-one']}],'bindings':{}}
  with Harness(doc,1) as h:
   status,s=h.management('api/apply',request(['claude-one'],groupIds=['g1'],manualModels=[],expectedGroupVersion=1));self.assertEqual(status,200);self.assertEqual(h.writes,[]);self.assertEqual(h.doc['bindings']['acct_test_gpt']['groupIds'],['g1'])
 def test_stale_native_policy_rejected_before_write(self):
  with Harness() as h:
   status,s=h.management('api/apply',request(before={'mode':'all','models':[]}));self.assertEqual(status,409);self.assertEqual(h.writes,[])
 def test_wildcard_rejected_before_write(self):
  with Harness() as h:
   status,s=h.management('api/apply',request(['claude-*']));self.assertEqual(status,400);self.assertEqual(h.writes,[])
 def test_group_cas_conflict_does_not_write(self):
  with Harness({'groups':[],'bindings':{}},1) as h:
   status,s=h.management('api/groups',{'expectedVersion':2,'groups':[]});self.assertEqual(status,409);self.assertEqual(h.version,1);self.assertEqual(h.writes,[])
 def test_group_expansion_tamper_rejected(self):
  doc={'groups':[{'id':'g1','name':'Claude','models':['claude-two']}],'bindings':{}}
  with Harness(doc,1) as h:
   status,s=h.management('api/apply',request(['claude-one'],groupIds=['g1'],manualModels=[],expectedGroupVersion=1));self.assertEqual(status,400);self.assertEqual(h.writes,[])
 def test_private_state_failure_reports_native_success(self):
  with Harness(fail_put=True) as h:
   status,s=h.management('api/apply',request());self.assertEqual(status,200);self.assertTrue(s['nativeApplied']);self.assertEqual(len(s['warnings']),1);self.assertEqual(h.accounts[0]['modelAccess']['models'],['claude-one','custom-image'])
 def test_native_error_not_replayed(self):
  with Harness(fail_native=True) as h:
   status,s=h.management('api/apply',request());self.assertEqual(status,502);self.assertEqual(len(h.writes),1);self.assertEqual(h.accounts[0]['modelAccess'],POLICY)
 def test_catalog_no_model_execution(self):
  with Harness() as h:
   status,s=h.management('api/catalog',{'accountId':'acct_test_gpt','refresh':False});self.assertEqual(status,200);self.assertEqual(s['models'],['claude-one','custom-image']);self.assertFalse(any('model.execute' in x or 'auth.' in x for x in h.callbacks))
 def test_unknown_fields_and_route_not_accepted(self):
  with Harness() as h:
   status,s=h.management('api/apply',{**request(),'url':'https://invalid.example'});self.assertEqual(status,400);status,s=h.management('api/not-declared');self.assertEqual(status,404);self.assertEqual(h.writes,[])
 def test_cancelled_read_does_not_mutate_native(self):
  with Harness() as h:
   h.seq+=2;id=h.seq;h.send({'type':'call','id':id,'method':'management.handle','context':h.context(id,'management'),'params':{'method':'GET','path':'api/snapshot','query':'','content_type':None,'headers':[]}})
   meta,body=h.read();self.assertEqual(meta['type'],'callback');h.send({'type':'cancel','id':id})
   while True:
    msg,p=h.read()
    if msg['type']=='cancelled' and msg['id']==id:break
    if msg['type']=='cancel':h.send({'type':'cancelled','id':msg['id']})
    elif msg['type']=='error' and msg['id']==id:break
    else:self.assertEqual(msg['type'],'credit')
   self.assertEqual(h.writes,[])
if __name__=='__main__':
 result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests));pathlib.Path(__file__).with_name('ipc-test-result.json').write_text(json.dumps({'tests':result.testsRun,'failures':len(result.failures),'errors':len(result.errors),'passed':result.wasSuccessful()},indent=2));raise SystemExit(not result.wasSuccessful())

