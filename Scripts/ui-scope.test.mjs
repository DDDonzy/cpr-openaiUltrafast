import test from 'node:test'
import assert from 'node:assert/strict'
import { middlewareBindings, anyKeyAllowed, keyBlockedReason, emptyRule, copyRule, rulesWithDraft, ruleValidation } from '../frontend/src/rule-ui.ts'
const key=(id,enabled=true)=>({id,name:`Name ${id}`,enabled})
const binding=(clientKeyIds=['key_a'],models=['gpt-6-astra','gpt-6.1-sol'])=>({contribution:'donzy.ultrafast.middleware',stage:'attempt',providerIds:['openai'],models,clientKeyIds,accountGroupIds:[],identityBindings:[]})
const rule=()=>({name:'Demo',clientKeyIds:['key_a'],accountIds:[],models:['gpt-6-astra']})
test('scope comes from bindings, not Donzy name or a hard-coded key',()=>{
 assert.equal(keyBlockedReason(key('key_b'),[binding(['key_b'])],['gpt-6-astra']),'')
 assert.equal(keyBlockedReason(key('key_a'),[binding(['key_b'])],['gpt-6-astra']),'入口未绑定')
})
test('unrestricted key binding admits arbitrary keys, but does not create rules',()=>{
 assert.equal(anyKeyAllowed([binding([])],['gpt-6-astra']),true)
 assert.equal(keyBlockedReason(key('another'),[binding([])],['gpt-6-astra']),'')
 assert.deepEqual(emptyRule(['gpt-6-astra']).clientKeyIds,[])
})
test('a key must have an entry for every selected model',()=>{
 assert.equal(keyBlockedReason(key('key_a'),[binding(['key_a'],['gpt-6-astra'])],['gpt-6-astra','gpt-6.1-sol']),'所选模型未绑定')
 assert.equal(keyBlockedReason(key('key_a'),[binding(['key_a'],['gpt-6-astra']),binding(['key_a'],['gpt-6.1-sol'])],['gpt-6-astra','gpt-6.1-sol']),'')
})
test('account-only drafts require unrestricted key coverage for all selected models',()=>{
 const r={...rule(),clientKeyIds:[],accountIds:['acct_a']}
 assert.notEqual(ruleValidation(r,[binding()],[key('key_a')]),'')
 assert.equal(ruleValidation(r,[binding([])],[key('key_a')]),'')
 assert.equal(anyKeyAllowed([binding([],['gpt-6-astra'])],['gpt-6-astra','gpt-6.1-sol']),false)
})
test('other providers, stages and contributions cannot unlock the key selector',()=>{
 assert.deepEqual(middlewareBindings([{...binding([]),stage:'request'},{...binding([]),providerIds:['xai']},{...binding([]),contribution:'other'}]),[])
 assert.equal(middlewareBindings([{...binding([]),providerIds:[]}]).length,1)
})
test('draft and saved list do not share mutable arrays',()=>{
 const r=rule(),d=copyRule(r);d.models.push('gpt-6.1-sol');d.clientKeyIds.push('key_b')
 assert.deepEqual(r,rule())
 const list=rulesWithDraft([r],0,d);list[0].models.length=0;assert.equal(d.models.length,2);assert.deepEqual(r,rule())
})
test('append/edit trims names without altering other persisted rules',()=>{
 const original=[rule()];const next=rulesWithDraft(original,-1,{...rule(),name:'  Next  '})
 assert.equal(next.length,2);assert.equal(next[1].name,'Next');assert.equal(original.length,1)
 assert.equal(rulesWithDraft(next,0,{...rule(),name:'Updated'})[1].name,'Next')
})
test('stale editor index is rejected rather than silently replacing a different position',()=>{
 assert.throws(()=>rulesWithDraft([],0,rule()),/位置已变化/)
 assert.throws(()=>rulesWithDraft([rule()],2,rule()),/位置已变化/)
})
test('empty scope, names, models and disabled keys cannot be saved',()=>{
 const cases=[{...rule(),name:''},{...rule(),name:'x'.repeat(129)},{...rule(),name:'bad\n'},{...rule(),models:[]},{...rule(),clientKeyIds:[]}]
 for(const r of cases)assert.notEqual(ruleValidation(r,[binding()],[key('key_a')]),'')
 assert.notEqual(ruleValidation(rule(),[binding()],[key('key_a',false)]),'')
 assert.notEqual(ruleValidation(rule(),[binding()],[]),'')
 assert.equal(ruleValidation(rule(),[binding()],[key('key_a')]),'')
})
