import test from 'node:test'
import assert from 'node:assert/strict'
import { createRuleController } from '../frontend/src/controller.ts'
const rule={name:'test',accountIds:[],clientKeyIds:['key_a'],models:['gpt-6-astra']}
const snapshot=()=>({version:3,rules:[],pluginVersion:'0.3.2',keys:[{id:'key_a',name:'Key',enabled:true}],accounts:[],models:['gpt-6-astra','gpt-6.1-sol'],instance:{id:'test',enabled:true,running:true,revision:695,artifactSha256:'b'.repeat(64),versionWarning:null,loadError:null,runtime:{status:'running',drainingRevisions:[]}}})
function deferred(){let resolve,reject;const promise=new Promise((r,j)=>{resolve=r;reject=j});return {promise,resolve,reject}}
function fixture(save){const calls=[];const c=createRuleController(async(path,data)=>{calls.push({path,data});return path==='api/snapshot'?snapshot():save(data)});return {c,calls}}
test('partial save result keeps catalogs, runtime and real package version',async()=>{
 const {c,calls}=fixture(()=>({rules:[rule],version:4}));await c.load();assert.equal(await c.save([rule]),true)
 assert.equal(c.snapshot.value.pluginVersion,'0.3.2');assert.equal(c.snapshot.value.instance.running,true)
 assert.equal(c.snapshot.value.keys[0].id,'key_a');assert.equal(c.snapshot.value.models.length,2);assert.equal(c.snapshot.value.version,4)
 assert.equal(calls[1].data.expectedVersion,3)
})
test('no optimistic mutation before server confirms persistence',async()=>{
 const d=deferred();const {c}=fixture(()=>d.promise);await c.load();const p=c.save([rule])
 assert.deepEqual(c.snapshot.value.rules,[]);assert.equal(c.saving.value,true)
 d.resolve({rules:[rule],version:4});await p;assert.deepEqual(c.snapshot.value.rules,[rule])
})
test('failed save preserves snapshot and draft, and does not erase error with auto reload',async()=>{
 const {c,calls}=fixture(()=>{throw Object.assign(new Error('invalid rule'),{status:400})});await c.load();const draft=[structuredClone(rule)]
 assert.equal(await c.save(draft),false);assert.deepEqual(draft,[rule]);assert.deepEqual(c.snapshot.value.rules,[])
 assert.equal(c.message.value,'invalid rule');assert.equal(calls.length,2);assert.equal(c.ready.value,true)
})
test('conflict blocks resubmission until explicit refresh',async()=>{
 const {c,calls}=fixture(()=>{throw Object.assign(new Error('conflict'),{status:409})});await c.load();assert.equal(await c.save([rule]),false)
 assert.equal(c.needsRefresh.value,true);assert.match(c.message.value,/conflict/);assert.equal(await c.save([rule]),false);assert.equal(calls.length,2)
 await c.load();assert.equal(c.ready.value,true)
})
test('network failure is an unknown result, never auto retried',async()=>{
 const {c,calls}=fixture(()=>{throw new Error('network')});await c.load();await c.save([rule]);assert.equal(c.needsRefresh.value,true);assert.equal(calls.length,2)
})
test('concurrent clicks and refresh while saving cannot race versions',async()=>{
 const d=deferred();const {c,calls}=fixture(()=>d.promise);await c.load();const p=c.save([rule]);assert.equal(await c.save([]),false);assert.equal(await c.load(),false)
 assert.equal(calls.length,2);d.resolve({rules:[rule],version:4});await p;assert.equal(c.snapshot.value.version,4)
})
test('initial load failure never reports an editable empty snapshot',async()=>{
 const c=createRuleController(async()=>{throw new Error('offline')});assert.equal(await c.load(),false);assert.equal(c.snapshot.value,null);assert.equal(c.ready.value,false);assert.equal(c.loading.value,false)
})
test('malformed save reply leaves existing state intact and requires verification',async()=>{
 const {c}=fixture(()=>({rules:null}));await c.load();assert.equal(await c.save([rule]),false);assert.equal(c.snapshot.value.version,3);assert.equal(c.needsRefresh.value,true)
})
test('deletion only changes the saved list after success',async()=>{
 const {c}=fixture(()=>({rules:[],version:5}));await c.load();c.snapshot.value={...c.snapshot.value,rules:[rule],version:4};await c.save([])
 assert.deepEqual(c.snapshot.value.rules,[]);assert.equal(c.snapshot.value.keys.length,1)
})
test('invalid snapshot contract is visible and does not replace existing catalog',async()=>{
 let first=true;const c=createRuleController(async()=>{if(first){first=false;return snapshot()}return {version:4,rules:[]}});await c.load();assert.equal(await c.load(),false)
 assert.equal(c.snapshot.value.pluginVersion,'0.3.2');assert.equal(c.ready.value,false)
})

test('snapshot and rule saving work without a middleware configuration contract',async()=>{
 const {c,calls}=fixture(()=>({rules:[rule],version:4}))
 assert.equal(await c.load(),true);assert.equal('bindings' in c.snapshot.value.instance,false)
 assert.equal(c.saveBindings,undefined);assert.equal(await c.save([rule]),true)
 assert.deepEqual(calls.map(x=>x.path),['api/snapshot','api/rules'])
})
