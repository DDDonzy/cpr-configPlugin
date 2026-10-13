"""真实插件子进程的 SDK 握手、受管回调、写入范围和生命周期测试，不访问真实服务器"""
import base64,zlib,pathlib,json,struct,subprocess,threading,queue,copy,unittest,time,os,tempfile,hashlib
ROOT=pathlib.Path(__file__).resolve().parent.parent;BIN=pathlib.Path(os.environ.get('CPR_TEST_PLUGIN_BIN',str(ROOT/'backend/target/debug'/('cpr-config-plugin.exe' if os.name=='nt' else 'cpr-config-plugin'))))
MANIFEST=json.loads(subprocess.check_output([str(BIN),'--print-manifest']))
POLICY={'mode':'denylist','models':['claude-one']}
ACCOUNT={'id':'acct_test_gpt','name':'GPT','provider':'openai','enabled':True,'status':'normal','modelAccess':copy.deepcopy(POLICY),'apiKey':'DO_NOT_EXPOSE_SENTINEL','concurrencyLimit':3,'weight':1}
class Harness:
 def __init__(self,root):
  env=os.environ.copy();env["CPR_MODEL_CAPABILITIES_TEST_DIR"]=str(root)
  self.proc=subprocess.Popen([str(BIN)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
  self.frames=queue.Queue();self.read_error=None;self.seq=99;self.accounts=[copy.deepcopy(ACCOUNT)];self.doc=None;self.version=None;self.fail_put=False;self.fail_native=False;self.callbacks=[];self.http_requests=[];self.writes=[];self.bodies={};self.body_seq=0
  threading.Thread(target=self.reader,daemon=True).start()
  self.send({'type':'hello','handshake':{'protocol_version':2,'artifact_sha256':'a'*64,'plugin_id':'donzy.model-routing-groups','instance_id':'test-instance','generation':1,'incarnation':'test-incarnation','configuration':{},'contributes':MANIFEST['contributes']}})
  ready,payload=self.read();assert ready['type']=='ready' and ready['protocol_version']==2 and not payload
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
    head=exact(12);n,b=struct.unpack('>IQ',head);meta=json.loads(exact(n));payload=exact(b);self.frames.put((meta,payload))
  except Exception as e:self.frames.put(e)
 def send(self,meta,payload=b''):
  data=json.dumps(meta,separators=(',',':'),ensure_ascii=False).encode();assert len(data)<=65536;self.proc.stdin.write(struct.pack('>IQ',len(data),len(payload))+data+payload);self.proc.stdin.flush()
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
 def context(self,id,stage):return {'call_id':id,'instance_id':'test-instance','generation':1,'incarnation':'test-incarnation','stage':stage,'timeout_ms':30000,'resource_stream':False,'resource_scope_id':'scope_'+str(id)}
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
  self.proc.stdout.close();self.proc.stderr.close()
 def __enter__(self):return self
 def __exit__(self,*_):self.close()

DATA={'model-a':{'display_name':'Model A','reasoning_levels':['low','medium','high'],'context_window':128000,'max_output_tokens':8000,'input_modalities':['text','image']},'model-b':{'display_name':'Model B','reasoning_levels':[],'context_window':64000,'input_modalities':['text']}}
ID=next(iter(DATA))
class Tests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(prefix='ipc-',dir=pathlib.Path(__file__).parent);self.root=pathlib.Path(self.temp.name);self.file=self.root/'models.common.json';self.file.write_text(json.dumps(DATA),encoding='utf-8')
 def tearDown(self):self.temp.cleanup()
 def req(self,h,models=None):
  status,s=h.management('models/api/snapshot');self.assertEqual(status,200);return {'expectedVersion':s['version'],'models':copy.deepcopy(DATA if models is None else models),'modelOrder':list(DATA if models is None else models)}
 def test_registration_snapshot_no_host_access(self):
  with Harness(self.root) as h:
   status,s=h.management('models/api/snapshot');self.assertEqual(status,200);self.assertEqual(s['models'],DATA);self.assertEqual(h.callbacks,[]);self.assertEqual(len(s['version']),64);self.assertEqual(s['pluginVersion'],'1.0.0');self.assertEqual(set(MANIFEST['contributes']),{'management'});self.assertEqual(MANIFEST['state'][0]['namespace'],'routing-groups')
 def test_preview_does_not_write(self):
  before=self.file.read_bytes()
  with Harness(self.root) as h:
   r=self.req(h);r['models'][ID]['display_name']='New';status,s=h.management('models/api/preview',r);self.assertEqual(status,200);self.assertIn(ID,s['diff']['changed']);self.assertEqual(self.file.read_bytes(),before);self.assertFalse((self.root/'backups').exists());self.assertFalse((self.root/'.write.lock').exists())
 def test_save_backup_and_format(self):
  before=self.file.read_bytes()
  with Harness(self.root) as h:
   r=self.req(h);r['models'][ID]['display_name']='New';status,s=h.management('models/api/save',r);self.assertEqual(status,200);self.assertEqual(s['snapshot']['models'][ID]['display_name'],'New');self.assertNotEqual(s['snapshot']['version'],r['expectedVersion']);backups=list((self.root/'backups').glob('*.json'));self.assertEqual(len(backups),1);self.assertEqual(backups[0].read_bytes(),before);self.assertIn('"reasoning_levels": ["low", "medium"',self.file.read_text());self.assertEqual(h.callbacks,[])
 def test_stale_save_preserves_newer_file(self):
  with Harness(self.root) as h:
   r=self.req(h);other=copy.deepcopy(DATA);other[ID]['display_name']='External';self.file.write_text(json.dumps(other),encoding='utf-8');newer=self.file.read_bytes();status,s=h.management('models/api/save',r);self.assertEqual(status,409);self.assertEqual(self.file.read_bytes(),newer);self.assertFalse((self.root/'backups').exists())
 def test_second_instance_stale_write(self):
  with Harness(self.root) as a,Harness(self.root) as b:
   r=self.req(a);r2=self.req(b);r['models'][ID]['display_name']='First';self.assertEqual(a.management('models/api/save',r)[0],200);r2['models'][ID]['display_name']='Second';self.assertEqual(b.management('models/api/save',r2)[0],409);self.assertEqual(json.loads(self.file.read_text())[ID]['display_name'],'First')
 def test_reject_client_fields_without_writes(self):
  with Harness(self.root) as h:
   for field in ('default_reasoning','description','supports_verbosity'):
    r=self.req(h);r['models'][ID][field]=True;self.assertEqual(h.management('models/api/save',r)[0],400)
   self.assertFalse((self.root/'backups').exists())
 def test_invalid_token_values(self):
  with Harness(self.root) as h:
   for value in (0,-1,1.5,True,'100',None):
    r=self.req(h);r['models'][ID]['max_output_tokens']=value;self.assertEqual(h.management('models/api/save',r)[0],400)
 def test_unknown_output_omitted(self):
  with Harness(self.root) as h:
   r=self.req(h);r['models'][ID].pop('max_output_tokens');status,s=h.management('models/api/save',r);self.assertEqual(status,200);self.assertNotIn('max_output_tokens',s['snapshot']['models'][ID])
 def test_all_deleted_only_by_explicit_save(self):
  with Harness(self.root) as h:
   r=self.req(h,{});self.assertEqual(h.management('models/api/preview',r)[0],200);self.assertNotEqual(json.loads(self.file.read_text()),{});self.assertEqual(h.management('models/api/save',r)[0],200);self.assertEqual(self.file.read_text(),'{}\n')
 def test_corruption_and_missing_file_not_initialized(self):
  with Harness(self.root) as h:
   r=self.req(h);self.file.write_text('broken');self.assertEqual(h.management('models/api/save',r)[0],400);self.assertEqual(self.file.read_text(),'broken');self.file.unlink();self.assertEqual(h.management('models/api/snapshot')[0],503);self.assertFalse(self.file.exists())
 def test_unknown_route_and_unknown_request_fields(self):
  with Harness(self.root) as h:
   r=self.req(h);r['path']='/etc/passwd';self.assertEqual(h.management('models/api/save',r)[0],400);self.assertEqual(h.management('models/api/unknown')[0],404);self.assertEqual(h.callbacks,[])
 def test_large_input_rejected(self):
  with Harness(self.root) as h:
   r=self.req(h);r['models'][ID]['display_name']='x'*(1024*1024+10000);self.assertEqual(h.management('models/api/save',r)[0],400)
 def test_noop_save_does_not_backup(self):
  before=self.file.read_bytes()
  with Harness(self.root) as h:
   r=self.req(h);self.assertEqual(h.management('models/api/save',r)[0],200);self.assertEqual(before,self.file.read_bytes());self.assertFalse((self.root/'backups').exists())
 def test_protected_local_config_not_changed(self):
  p=pathlib.Path.home()/'.codex'/'models.json';before=p.read_bytes() if p.exists() else None
  with Harness(self.root) as h:self.assertEqual(h.management('models/api/snapshot')[0],200)
  self.assertEqual(p.read_bytes() if p.exists() else None,before)

 def test_reorder_only_changes_json_order_and_is_backed_up(self):
  with Harness(self.root) as h:
   r=self.req(h);before=self.file.read_bytes();r['modelOrder']=list(reversed(r['modelOrder']));status,preview=h.management('models/api/preview',r);self.assertEqual(status,200);self.assertTrue(preview['diff']['orderChanged']);self.assertEqual(preview['diff']['changed'],[]);self.assertEqual(self.file.read_bytes(),before)
   status,s=h.management('models/api/save',r);self.assertEqual(status,200);self.assertEqual(s['snapshot']['modelOrder'],r['modelOrder']);self.assertEqual(list(json.loads(self.file.read_text())),r['modelOrder']);self.assertEqual(s['snapshot']['models'],DATA);self.assertTrue((self.root/'backups').exists())
   r2=self.req(h);r2['modelOrder']=r['modelOrder'];before=self.file.read_bytes();self.assertEqual(h.management('models/api/save',r2)[0],200);self.assertEqual(self.file.read_bytes(),before)
 def test_tampered_order_never_writes(self):
  with Harness(self.root) as h:
   for order in [[],[ID]*len(DATA),list(DATA)[:-1]+['unknown']]:
    r=self.req(h);r['modelOrder']=order;before=self.file.read_bytes();self.assertEqual(h.management('models/api/save',r)[0],400);self.assertEqual(self.file.read_bytes(),before)
 def test_numeric_model_ids_obey_explicit_order(self):
  with Harness(self.root) as h:
   data={'10':copy.deepcopy(DATA[ID]),'2':copy.deepcopy(DATA[ID])};r=self.req(h,data);r['modelOrder']=['10','2'];status,s=h.management('models/api/save',r);self.assertEqual(status,200);self.assertEqual(s['snapshot']['modelOrder'],['10','2']);self.assertEqual(list(json.loads(self.file.read_text())),['10','2'])

class CombinedTests(unittest.TestCase):
 setUp=Tests.setUp
 tearDown=Tests.tearDown
 req=Tests.req
 def test_download_payload_matches_packaged_resource(self):
  with Harness(self.root) as h:
   result,body=h.call('management.handle',{'method':'GET','path':'api/setup-download','query':'','content_type':None,'headers':[]})
   self.assertEqual(result['status'],200);self.assertEqual(body,(ROOT/'dist/codex-setup.bat').read_bytes());self.assertEqual(h.callbacks,[])
 def test_modules_have_no_cross_writes(self):
  with Harness(self.root) as h:
   h.doc={'groups':[{'id':'g_test','name':'Existing','models':['claude-one']}],'bindings':{}};h.version=7
   original=copy.deepcopy(h.doc);req=self.req(h);req['models'][ID]['display_name']='Updated'
   self.assertEqual(h.management('models/api/save',req)[0],200);self.assertEqual(h.doc,original);self.assertEqual(h.writes,[])
   before=self.file.read_bytes();status,view=h.management('api/snapshot');self.assertEqual(status,200);self.assertEqual(view['state']['version'],7)
   status,_=h.management('api/groups',{'expectedVersion':7,'groups':[{'id':'g_test','name':'Renamed','models':['claude-one']}]})
   self.assertEqual(status,200);self.assertEqual(self.file.read_bytes(),before);self.assertEqual(h.writes,[])
if __name__=='__main__':unittest.main(verbosity=2)
