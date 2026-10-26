import { computed, ref, shallowRef } from 'vue'
import type { Rule, RuleSet, Snapshot } from './api.ts'

export type Request = <T>(path: string, data?: unknown) => Promise<T>
function isRuleSet(value: unknown): value is RuleSet {
  if (!value || typeof value !== 'object') return false
  const v = value as RuleSet
  const strings = (a: unknown) => Array.isArray(a) && a.every(x => typeof x === 'string')
  return (v.version === null || (Number.isSafeInteger(v.version) && v.version >= 0)) && Array.isArray(v.rules) && v.rules.every(r =>
    r && typeof r.name === 'string' && strings(r.accountIds) && strings(r.clientKeyIds) && strings(r.models))
}
function isSnapshot(value: unknown): value is Snapshot {
  if (!isRuleSet(value)) return false
  const v = value as Snapshot
  return typeof v.pluginVersion === 'string' && Array.isArray(v.models) && Array.isArray(v.keys) && Array.isArray(v.accounts)
    && !!v.instance && typeof v.instance.enabled === 'boolean' && typeof v.instance.running === 'boolean' && Array.isArray(v.instance.bindings)
}

/** Saved server state is authoritative; drafts belong to the editor, never this snapshot. */
export function createRuleController(request: Request) {
  const snapshot = shallowRef<Snapshot | null>(null)
  const loading = ref(false), saving = ref(false), needsRefresh = ref(false)
  const message = ref(''), messageType = ref<'danger' | 'success' | 'info'>('info')
  const ready = computed(() => snapshot.value !== null && !needsRefresh.value)
  const locked = computed(() => loading.value || saving.value)
  function error(e: unknown) {
    message.value = e instanceof Error ? e.message : '操作未完成，请刷新确认'
    messageType.value = 'danger'
  }
  async function load(): Promise<boolean> {
    if (locked.value) return false
    loading.value = true
    try {
      const s = await request<Snapshot>('api/snapshot')
      if (!isSnapshot(s)) throw new Error('页面数据格式无效，未覆盖已加载内容')
      snapshot.value = s
      needsRefresh.value = false
      message.value = ''
      return true
    } catch (e) {
      needsRefresh.value = true
      error(e)
      return false
    } finally { loading.value = false }
  }
  async function save(rules: Rule[]): Promise<boolean> {
    if (locked.value || !ready.value || !snapshot.value) return false
    saving.value = true
    const before = snapshot.value
    try {
      const s = await request<RuleSet>('api/rules', { rules: JSON.parse(JSON.stringify(rules)), expectedVersion: before.version })
      if (!isRuleSet(s)) throw new Error('保存结果格式无效，结果尚未确认，请刷新后核对')
      // POST deliberately returns only rules/version, not the catalog or runtime state
      snapshot.value = { ...before, rules: s.rules, version: s.version }
      message.value = `已保存 ${s.rules.length} 条规则`
      messageType.value = 'success'
      return true
    } catch (e) {
      const status = e instanceof Error && 'status' in e ? e.status : undefined
      needsRefresh.value = status !== 400
      error(e)
      if (needsRefresh.value) message.value += '，草稿已保留，请取消编辑并刷新后核对'
      return false
    } finally { saving.value = false }
  }
  return { snapshot, loading, saving, ready, locked, needsRefresh, message, messageType, load, save }
}
