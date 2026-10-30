import test from 'node:test'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { parseModelIds, validModelId } from '../frontend/src/model-ui.ts'
test('model IDs are not tied to a version-specific catalog',()=>{
 for(const m of ['gpt-6-luna','custom/model:2026','provider:model','future-model'])assert.equal(validModelId(m),true)
 for(const m of ['', ' bad', 'bad model', 'bad\0', 'x'.repeat(129)])assert.equal(validModelId(m),false)
 assert.deepEqual(parseModelIds('gpt-6-luna, custom/model，future\nfuture'),['gpt-6-luna','custom/model','future'])
})
test('plugin page and controller no longer expose middleware configuration',async()=>{
 const app=await readFile(new URL('../frontend/src/App.vue',import.meta.url),'utf8')
 const controller=await readFile(new URL('../frontend/src/controller.ts',import.meta.url),'utf8')
 assert.doesNotMatch(app,/BindingEditor|请求中间件配置|bindingsOpen|bindingUnrestricted/)
 assert.doesNotMatch(controller,/api\/bindings|saveBindings/)
})
test('necessary execution hook remains declared independently of rule UI',async()=>{
 const manifest=JSON.parse(await readFile(new URL('../plugin.json',import.meta.url),'utf8'))
 assert.deepEqual(manifest.contributes.middleware.stages,['attempt'])
 assert.equal(manifest.state[0].schemaVersion,2)
 assert.equal('enum' in manifest.state[0].schema.properties.rules.items.properties.models.items,false)
})
