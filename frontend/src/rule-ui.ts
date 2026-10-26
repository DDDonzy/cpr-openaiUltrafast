import type { Binding, KeyFact, Rule } from './api.ts'

export function middlewareBindings(bindings: Binding[]): Binding[] {
  return bindings.filter(b => b.contribution === 'donzy.ultrafast.middleware' && b.stage === 'attempt' && (!b.providerIds.length || b.providerIds.includes('openai')))
}
function modelAllowed(b: Binding, model: string) { return !b.models.length || b.models.includes(model) }
export function anyKeyAllowed(bindings: Binding[], models: string[]): boolean {
  return models.length > 0 && models.every(model => bindings.some(b => !b.clientKeyIds.length && modelAllowed(b, model)))
}
export function keyBlockedReason(key: KeyFact, bindings: Binding[], models: string[]): string {
  if (!key.enabled) return 'Key 已停用'
  const entries = bindings.filter(b => !b.clientKeyIds.length || b.clientKeyIds.includes(key.id))
  if (!entries.length) return '入口未绑定'
  if (models.some(model => !entries.some(b => modelAllowed(b, model)))) return '所选模型未绑定'
  return ''
}
export function copyRule(rule: Rule): Rule {
  return { name: rule.name, clientKeyIds: [...rule.clientKeyIds], accountIds: [...rule.accountIds], models: [...rule.models] }
}
export function emptyRule(models: string[]): Rule {
  return { name: '', clientKeyIds: [], accountIds: [], models: [...models] }
}
export function rulesWithDraft(rules: Rule[], index: number, draft: Rule): Rule[] {
  if (index < -1 || index >= rules.length) throw new Error('规则位置已变化，请刷新后重新编辑')
  const next = rules.map(copyRule), value = copyRule(draft)
  value.name = value.name.trim()
  if (index === -1) next.push(value); else next[index] = value
  return next
}
export function ruleValidation(rule: Rule, bindings: Binding[], keys: KeyFact[]): string {
  if (!rule.name.trim()) return '填写规则名称后保存'
  if ([...rule.name].length > 128 || /[\u0000-\u001f\u007f]/u.test(rule.name)) return '规则名称最多 128 个字符，不能包含控制字符'
  if (!rule.models.length) return '至少选择一个模型'
  if (!rule.clientKeyIds.length && !rule.accountIds.length) return '至少选择一个 Key 或账号'
  if (!rule.clientKeyIds.length && !anyKeyAllowed(bindings, rule.models)) return '当前入口限制了 Key，请选择已绑定的 Key'
  for (const id of rule.clientKeyIds) {
    const key = keys.find(k => k.id === id)
    if (!key) return '所选 Key 已不存在，请重新选择'
    const reason = keyBlockedReason(key, bindings, rule.models)
    if (reason) return `${key.name}：${reason}`
  }
  return ''
}
