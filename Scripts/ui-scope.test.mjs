import test from 'node:test'
import assert from 'node:assert/strict'
import { keyBlockedReason, emptyRule, copyRule, rulesWithDraft, ruleValidation } from '../frontend/src/rule-ui.ts'
const key=(id,enabled=true)=>({id,name:`Name ${id}`,enabled})
const rule=()=>({name:'Demo',clientKeyIds:['key_a'],accountIds:[],models:['gpt-6-astra']})
test('all enabled keys are selectable without a second middleware scope',()=>{
 assert.equal(keyBlockedReason(key('key_a')),'')
 assert.equal(keyBlockedReason(key('another-key')),'')
 assert.equal(keyBlockedReason(key('disabled',false)),'Key 已停用')
})
test('account-only drafts do not require a separate key binding',()=>{
 const r={...rule(),clientKeyIds:[],accountIds:['acct_a']}
 assert.equal(ruleValidation(r,[]),'')
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
 for(const r of cases)assert.notEqual(ruleValidation(r,[key('key_a')]),'')
 assert.notEqual(ruleValidation(rule(),[key('key_a',false)]),'')
 assert.notEqual(ruleValidation(rule(),[]),'')
 assert.equal(ruleValidation(rule(),[key('key_a')]),'')
})
test('new drafts never implicitly select an entire dynamic catalog',()=>{
 assert.deepEqual(emptyRule(['gpt-6-luna','future-model']).models,[])
 assert.deepEqual(emptyRule().clientKeyIds,[])
})
test('custom model IDs and another key require only a valid rule',()=>{
 const r={...rule(),clientKeyIds:['key_b'],models:['custom/model:latest','gpt-6-luna','third-model']}
 assert.equal(ruleValidation(r,[key('key_b')]),'')
 for(const models of [['bad model'],['x'.repeat(129)],['same','same'],Array.from({length:257},(_,i)=>`m${i}`)]){
  assert.notEqual(ruleValidation({...r,models},[key('key_b')]),'')
 }
})
