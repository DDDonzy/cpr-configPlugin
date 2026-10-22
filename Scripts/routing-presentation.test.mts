import test from 'node:test'
import assert from 'node:assert/strict'
import {accountLabels,compactSource,filterAccount,matchesAccount,nativeName,relativeTime,safeColor,safeFileName,statusView,upstreamView} from '../frontend/src/routing/accountPresentation.ts'
const base={provider:'openai',enabled:true,status:'normal',modelAccess:{mode:'all',models:[]}}
const oauth=(id,email,extra={})=>({...base,id,name:'OpenAI 账号',identity:{email,authenticationKind:'oauth',planType:'pro',planTypeDisplay:'Pro',groups:[{id:'grp_t',name:'DEFAULT',enabled:true}],...extra}})
const key=(id,name,extra={})=>({...base,id,name,identity:{authenticationKind:'api_key',...extra}})
const a=oauth('acct_test_one','one@example.test'),b=oauth('acct_test_two','two@example.org',{notes:'备用'})
const r1=key('acct_test_relay_aaaaaa','Relay'),r2=key('acct_test_relay_bbbbbb','Relay')
const legacy={...base,id:'acct_legacy',name:'Legacy'}
test('same native names are told apart by email, duplicate labels get an ID suffix',()=>{
 const m=accountLabels([a,b,r1,r2,legacy])
 assert.equal(m.get(a.id),'one@example.test');assert.equal(m.get(b.id),'two@example.org')
 assert.equal(m.get(r1.id),'Relay（…aaaaaa）');assert.equal(m.get(r2.id),'Relay（…bbbbbb）');assert.equal(m.get(legacy.id),'Legacy')
 assert.equal(nativeName(a),'OpenAI 账号');assert.equal(nativeName(r1),undefined)})
test('upstream view distinguishes official, local, relay and unreadable sources',()=>{
 assert.equal(upstreamView(a,undefined,false).text,'OpenAI 官方')
 assert.equal(upstreamView(r1,undefined,true).tone,'pending')
 const local=upstreamView(r1,{kind:'api_key',status:'ok',address:{url:'http://127.0.0.1:8317',host:'127.0.0.1:8317',loopback:true}},false)
 assert.equal(local.text,'127.0.0.1:8317');assert.equal(local.tone,'local');assert.match(local.detail,/本机服务/)
 assert.equal(upstreamView(r1,{kind:'api_key',status:'timeout'},false).text,'上游地址读取超时')
 assert.equal(upstreamView(r1,{kind:'api_key',status:'unavailable'},false).text,'上游地址不可显示')
 assert.equal(upstreamView(r1,undefined,false).text,'上游地址不可显示')
 assert.equal(compactSource(a,upstreamView(a,undefined,false)),'Pro · OpenAI 账号')
 assert.equal(compactSource(r1,local),'API Key · 127.0.0.1:8317')})
test('search covers email, upstream host, CPR group and notes',()=>{
 const up={kind:'api_key',status:'ok',address:{url:'https://relay.example.test',host:'relay.example.test',loopback:false}}
 assert.ok(matchesAccount(a,'one@example.test',undefined,'EXAMPLE.TEST'));assert.ok(matchesAccount(r1,'Relay',up,'relay.example'))
 assert.ok(matchesAccount(a,'x',undefined,'default'));assert.ok(matchesAccount(b,'x',undefined,'备用'))
 assert.ok(!matchesAccount(r1,'Relay',up,'gmail'));assert.ok(matchesAccount(r1,'Relay',up,'  '))
 assert.ok(filterAccount(a,'oauth')&&!filterAccount(a,'api_key')&&filterAccount(r1,'api_key')&&!filterAccount(a,'disabled'))})
test('status, time, colour and file name helpers stay safe',()=>{
 assert.deepEqual(statusView({...a,enabled:false}),{text:'已停用',type:'default'});assert.equal(statusView({...a,status:'rate_limited'}).type,'warning')
 const now=Date.parse('2026-10-22T12:00:00Z')
 assert.equal(relativeTime('2026-10-22T11:59:40Z',now),'刚刚');assert.equal(relativeTime('2026-10-22T11:55:00Z',now),'5 分钟前')
 assert.equal(relativeTime('2026-10-22T09:00:00Z',now),'3 小时前');assert.equal(relativeTime('2026-10-20T11:00:00Z',now),'2 天前')
 assert.equal(relativeTime('2026-10-23T00:00:00Z',now),'刚刚');assert.equal(relativeTime('not a date',now),undefined)
 assert.equal(safeColor('#05FF0080'),'#05FF0080');assert.equal(safeColor('red;background:url(x)'),undefined)
 assert.equal(safeFileName('a/b:c*?.json'),'a_b_c_.json');assert.equal(safeFileName('  '),'account')})
