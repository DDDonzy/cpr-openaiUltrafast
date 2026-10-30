import type { KeyFact, Rule } from './api.ts'
import { MAX_MODELS, validModelId } from './model-ui.ts'

export function keyBlockedReason(key: KeyFact): string {
  return key.enabled ? '' : 'Key 已停用'
}
export function copyRule(rule: Rule): Rule {
  return { name: rule.name, clientKeyIds: [...rule.clientKeyIds], accountIds: [...rule.accountIds], models: [...rule.models] }
}
export function emptyRule(_models: string[] = []): Rule {
  return { name: '', clientKeyIds: [], accountIds: [], models: [] }
}
export function rulesWithDraft(rules: Rule[], index: number, draft: Rule): Rule[] {
  if (index < -1 || index >= rules.length) throw new Error('规则位置已变化，请刷新后重新编辑')
  const next = rules.map(copyRule), value = copyRule(draft)
  value.name = value.name.trim()
  if (index === -1) next.push(value); else next[index] = value
  return next
}
export function ruleValidation(rule: Rule, keys: KeyFact[]): string {
  if (!rule.name.trim()) return '填写规则名称后保存'
  if ([...rule.name].length > 128 || /[\u0000-\u001f\u007f]/u.test(rule.name)) return '规则名称最多 128 个字符，不能包含控制字符'
  if (!rule.models.length) return '至少选择一个模型'
  if (rule.models.length > MAX_MODELS || rule.models.some(m => !validModelId(m)) || new Set(rule.models).size !== rule.models.length) return '模型 ID 不合法、重复或超过 256 个'
  if (!rule.clientKeyIds.length && !rule.accountIds.length) return '至少选择一个 Key 或账号'
  for (const id of rule.clientKeyIds) {
    const key = keys.find(k => k.id === id)
    if (!key) return '所选 Key 已不存在，请重新选择'
    const reason = keyBlockedReason(key)
    if (reason) return `${key.name}：${reason}`
  }
  return ''
}
