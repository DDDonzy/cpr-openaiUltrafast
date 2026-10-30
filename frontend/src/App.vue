<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { ZButton, ZCard, ZCheckbox, ZDialog, ZEmpty, ZForm, ZFormItem, ZInput, ZScrollbar, ZTable, ZTag, type TableColumn } from '@codex-proxy/ui'
import { Info, Plus, RefreshCw, Save } from '@lucide/vue'
import ModelPicker from './ModelPicker.vue'
import { api, type Rule } from './api'
import { createRuleController } from './controller'
import { copyRule, emptyRule, keyBlockedReason, ruleValidation, rulesWithDraft } from './rule-ui'
import { useVisibleDialogViewport } from './dialog-viewport'

const { snapshot, loading, saving, ready, locked, message, messageType, load, save } = createRuleController(api)
const dialog = useVisibleDialogViewport(), detailsOpen = dialog(), deleteOpen = dialog(), discardOpen = dialog()
const editIndex = ref(-1), deleteIndex = ref(-1)
const nameInput = ref<InstanceType<typeof ZInput> | null>(null)
const keySearch = ref(''), accountSearch = ref('')
const draft = ref<Rule>(emptyRule([])), baselineDraft = ref(JSON.stringify(draft.value))
const rules = computed(() => snapshot.value?.rules ?? [])
const keys = computed(() => snapshot.value?.keys ?? [])
const accounts = computed(() => (snapshot.value?.accounts ?? []).filter(a => a.provider_id === 'openai'))
const models = computed(() => snapshot.value?.models ?? [])
const instance = computed(() => snapshot.value?.instance)
const editable = computed(() => ready.value && !locked.value && !!instance.value?.enabled && !!instance.value?.running)
const draftDirty = computed(() => JSON.stringify(draft.value) !== baselineDraft.value)
const keyReason = (key: typeof keys.value[number]) => keyBlockedReason(key)
const selectableKeys = computed(() => keys.value.filter(k => !keyReason(k)))
const visibleKeys = computed(() => keys.value.filter(k => `${k.name} ${k.id}`.toLowerCase().includes(keySearch.value.trim().toLowerCase())))
const visibleAccounts = computed(() => accounts.value.filter(a => `${a.name} ${a.email ?? ''}`.toLowerCase().includes(accountSearch.value.trim().toLowerCase())))
const allKeys = computed(() => selectableKeys.value.length > 0 && selectableKeys.value.every(k => draft.value.clientKeyIds.includes(k.id)))
const keyName = (id: string) => keys.value.find(k => k.id === id)?.name ?? `${id}（已失效）`
const accountName = (id: string) => { const a = accounts.value.find(a => a.account_id === id); return a ? a.email || a.name : `${id}（已失效）` }
const absentKeys = computed(() => draft.value.clientKeyIds.filter(id => !keys.value.some(k => k.id === id)))
const absentAccounts = computed(() => draft.value.accountIds.filter(id => !accounts.value.some(a => a.account_id === id)))
const validation = computed(() => ruleValidation(draft.value, keys.value) || (absentAccounts.value.length ? '部分账号已不存在，请移除后保存' : ''))
const canSave = computed(() => editable.value && !validation.value && (editIndex.value >= 0 || rules.value.length < 64))
const statusError = computed(() => instance.value?.loadError || instance.value?.versionWarning || (snapshot.value && !instance.value?.running ? '插件未运行，请检查运行详情' : ''))
type RuleRow = Rule & { index: number }
const rows = computed<RuleRow[]>(() => rules.value.map((r, index) => ({ ...r, index })))
const columns: TableColumn<RuleRow>[] = [
  { key: 'name', label: '规则', kind: 'custom', minWidth: 160 },
  { key: 'keys', label: '客户端 Key', kind: 'custom', minWidth: 160 },
  { key: 'accounts', label: '上游账号', kind: 'custom', minWidth: 200 },
  { key: 'models', label: '模型', kind: 'custom', minWidth: 200 },
  { key: 'actions', label: '操作', kind: 'actions', width: 130 },
]
type PendingAction = { kind: 'new' | 'refresh' } | { kind: 'edit'; index: number }
const pendingAction = ref<PendingAction | null>(null)
function resetDraft(rule = emptyRule(models.value), index = -1) {
  draft.value = copyRule(rule); baselineDraft.value = JSON.stringify(draft.value)
  editIndex.value = index; keySearch.value = ''; accountSearch.value = ''
}
async function applyAction(action: PendingAction) {
  if (locked.value) return
  if (action.kind === 'refresh') { if (await load()) resetDraft(); return }
  if (!editable.value) return
  if (action.kind === 'edit') resetDraft(rules.value[action.index]!, action.index)
  else resetDraft()
  message.value = ''
  await nextTick()
  nameInput.value?.focus()
}
function requestAction(action: PendingAction) {
  if (locked.value) return
  if (draftDirty.value) { pendingAction.value = action; discardOpen.value = true }
  else void applyAction(action)
}
async function discardDraft() {
  if (!pendingAction.value) return
  const action = pendingAction.value; pendingAction.value = null; discardOpen.value = false
  await applyAction(action)
}
function toggle(field: 'clientKeyIds' | 'accountIds' | 'models', id: string, checked: boolean) {
  if (!editable.value) return
  draft.value[field] = checked ? [...new Set([...draft.value[field], id])] : draft.value[field].filter(x => x !== id)
}
function toggleAllKeys() {
  if (!editable.value) return
  const eligible = new Set(selectableKeys.value.map(k => k.id))
  draft.value.clientKeyIds = allKeys.value ? draft.value.clientKeyIds.filter(id => !eligible.has(id)) : [...new Set([...draft.value.clientKeyIds, ...eligible])]
}
async function submitDraft() {
  if (!canSave.value) return
  if (await save(rulesWithDraft(rules.value, editIndex.value, draft.value))) resetDraft()
}
function askDelete(index: number) {
  if (!editable.value || draftDirty.value || editIndex.value >= 0) return
  deleteIndex.value = index; message.value = ''; deleteOpen.value = true
}
async function confirmDelete() {
  if (!editable.value || deleteIndex.value < 0) return
  if (await save(rules.value.filter((_, i) => i !== deleteIndex.value))) deleteOpen.value = false
}
onMounted(async () => { if (await load()) resetDraft() })
</script>

<template>
  <div class="uf-page" :aria-busy="locked">
    <div class="uf-toolbar">
      <div class="uf-actions">
        <ZTag v-if="!snapshot" type="info">{{ loading ? '正在加载' : '尚未加载' }}</ZTag>
        <template v-else>
          <ZTag :type="rules.length ? 'primary' : 'default'">{{ rules.length ? `${rules.length} 条规则` : '未开启强制规则' }}</ZTag>
          <span class="uf-secondary">仅保存的规则生效</span>
        </template>
      </div>
      <div class="uf-actions">
        <ZButton size="small" variant="text" :disabled="!snapshot" @click="detailsOpen = true"><template #icon><Info :size="14" /></template>运行详情</ZButton>
        <ZButton size="small" variant="text" :loading="loading" :disabled="locked" @click="requestAction({ kind: 'refresh' })"><template #icon><RefreshCw :size="14" /></template>刷新</ZButton>
        <ZButton size="small" :disabled="!editable" @click="requestAction({ kind: 'new' })"><template #icon><Plus :size="14" /></template>新建规则</ZButton>
      </div>
    </div>
    <p v-if="statusError" class="uf-error" role="alert">{{ statusError }}</p>
    <p v-if="message && !deleteOpen" class="uf-feedback" :class="messageType" :role="messageType === 'danger' ? 'alert' : 'status'">{{ message }}</p>

    <p v-for="warning in snapshot?.modelWarnings ?? []" :key="warning" class="uf-footnote">{{ warning }}</p>
    <div class="uf-board">
      <ZCard title="客户端 Key" padding="compact" class="uf-target-card">
        <template #description>已选 {{ draft.clientKeyIds.length }} / 可选 {{ selectableKeys.length }}</template>
        <template #actions><ZButton size="small" variant="text" :disabled="!editable || !selectableKeys.length" @click="toggleAllKeys">{{ allKeys ? '取消全选' : '全选可用' }}</ZButton></template>
        <div class="uf-list-body">
          <ZInput v-model="keySearch" clearable placeholder="搜索 Key" class="uf-field" />
          <ZScrollbar height="268px" class="uf-selection-scroll">
            <div class="uf-selection-list">
              <ZCheckbox v-for="key in visibleKeys" :key="key.id" class="uf-selection-row"
                :model-value="draft.clientKeyIds.includes(key.id)" :disabled="!editable || (!!keyReason(key) && !draft.clientKeyIds.includes(key.id))"
                @update:model-value="checked => toggle('clientKeyIds', key.id, checked)">
                <template #label><span class="uf-list-label">{{ key.name }}</span><span class="uf-list-meta">{{ keyReason(key) || '可用于当前规则' }}</span></template>
              </ZCheckbox>
              <ZEmpty v-if="!visibleKeys.length" size="small" surface="none" :description="loading ? '正在加载 Key' : '没有匹配的 Key'" />
            </div>
          </ZScrollbar>
          <div v-if="absentKeys.length" class="uf-actions"><ZTag v-for="id in absentKeys" :key="id" type="warning" closable :disabled="!editable" @close="toggle('clientKeyIds', id, false)">{{ keyName(id) }}</ZTag></div>
          <p class="uf-footnote uf-push-bottom">只需在规则里选择 Key，不改变 Key 原有权限与分组</p>
        </div>
      </ZCard>

      <ZCard title="上游账号" padding="compact" class="uf-target-card">
        <template #description>{{ draft.accountIds.length ? `已选 ${draft.accountIds.length} / ${accounts.length}` : '跟随 Key 分组' }}</template>
        <template #actions><ZButton size="small" variant="text" :disabled="!editable || !draft.accountIds.length" @click="draft.accountIds = []">取消限定</ZButton></template>
        <div class="uf-list-body">
          <ZInput v-model="accountSearch" clearable placeholder="搜索账号" class="uf-field" />
          <ZScrollbar height="268px" class="uf-selection-scroll">
            <div class="uf-selection-list">
              <ZCheckbox v-for="account in visibleAccounts" :key="account.account_id" class="uf-selection-row"
                :model-value="draft.accountIds.includes(account.account_id)" :disabled="!editable || (!account.enabled && !draft.accountIds.includes(account.account_id))"
                @update:model-value="checked => toggle('accountIds', account.account_id, checked)">
                <template #label><span class="uf-list-label">{{ account.email || account.name }}</span><span class="uf-list-meta">OpenAI{{ account.enabled ? '' : ' · 已停用' }}</span></template>
              </ZCheckbox>
              <ZEmpty v-if="!visibleAccounts.length" size="small" surface="none" :description="loading ? '正在加载账号' : '没有匹配的账号'" />
            </div>
          </ZScrollbar>
          <div v-if="absentAccounts.length" class="uf-actions"><ZTag v-for="id in absentAccounts" :key="id" type="warning" closable :disabled="!editable" @close="toggle('accountIds', id, false)">{{ accountName(id) }}</ZTag></div>
          <p class="uf-footnote uf-push-bottom">账号可不选，仍使用 Key 原有分组与权限</p>
        </div>
      </ZCard>

      <ZCard :title="editIndex >= 0 ? '编辑规则' : '新建规则'" padding="compact" class="uf-target-card uf-settings-card">
        <template #description>{{ editIndex >= 0 ? `正在编辑第 ${editIndex + 1} 条规则` : '选择范围后保存新规则' }}</template>
        <ZForm class="uf-rule-form">
          <ZFormItem label="规则名称" required>
            <ZInput ref="nameInput" v-model="draft.name" :disabled="!editable" class="uf-field" placeholder="例如：指定 Key 的加速请求" />
          </ZFormItem>
          <fieldset class="uf-model-field"><legend>模型 *</legend>
            <ModelPicker v-model="draft.models" :options="models" :disabled="!editable" />
          </fieldset>
          <div class="uf-tier"><span class="uf-secondary">强制档位</span><ZTag type="primary" size="small">ultrafast</ZTag></div>
          <p class="uf-footnote">匹配 Key、账号和模型后覆盖客户端档位，不改变路由或上游返回结果</p>
          <div class="uf-save-area">
            <span class="uf-secondary" role="status">{{ !ready ? '请先加载可用配置' : validation || `${draft.clientKeyIds.length || '全部'} 个 Key · ${draft.accountIds.length ? `${draft.accountIds.length} 个账号` : '跟随分组'} · ${draft.models.length} 个模型` }}</span>
            <ZButton type="primary" class="uf-save-button" @click="submitDraft" :disabled="!canSave" :loading="saving"><template #icon><Save :size="16" /></template>{{ editIndex >= 0 ? '保存修改' : '保存规则' }}</ZButton>
            <ZButton v-if="draftDirty || editIndex >= 0" size="small" variant="text" :disabled="locked" @click="requestAction({ kind: 'new' })">{{ editIndex >= 0 ? '取消编辑' : '清空草稿' }}</ZButton>
          </div>
        </ZForm>
      </ZCard>
    </div>

    <ZCard title="已保存规则" padding="compact">
      <template #description>{{ rules.length ? `${rules.length} / 64 条 · 多条规则取并集` : '目前不强制改写任何请求' }}</template>
      <ZTable class="uf-rules-table" :columns="columns" :data="rows" row-key="index" size="small" :loading="loading && !snapshot" show-header-when-empty empty-text="暂无规则，在上方选择范围并保存">
        <template #name="{ row }"><span class="uf-table-name">{{ row.name }}</span><span v-if="editIndex === row.index" class="uf-list-meta">正在编辑</span></template>
        <template #keys="{ row }"><span class="uf-table-text">{{ row.clientKeyIds.length ? row.clientKeyIds.map(keyName).join('、') : '全部 Key' }}</span></template>
        <template #accounts="{ row }"><span class="uf-table-text">{{ row.accountIds.length ? row.accountIds.map(accountName).join('、') : '跟随 Key 分组' }}</span></template>
        <template #models="{ row }"><div class="uf-model-tags"><ZTag v-for="model in row.models" :key="model" size="small">{{ model }}</ZTag></div></template>
        <template #actions="{ row }"><div class="uf-actions">
          <ZButton size="small" variant="text" :disabled="!editable" @click="requestAction({ kind: 'edit', index: row.index })">编辑</ZButton>
          <ZButton size="small" variant="text" type="danger" :disabled="!editable || draftDirty || editIndex >= 0" @click="askDelete(row.index)">删除</ZButton>
        </div></template>
      </ZTable>
    </ZCard>

    <ZDialog v-model="detailsOpen" title="运行详情" width="640px" :draggable="false">
      <dl class="uf-details">
        <div><dt>插件版本</dt><dd>{{ snapshot?.pluginVersion ?? '—' }}</dd></div>
        <div><dt>运行状态</dt><dd>{{ instance?.running ? '运行中' : '未运行' }}</dd></div>
        <div><dt>实例修订 / 规则版本</dt><dd>{{ instance?.revision ?? '—' }} / {{ snapshot?.version ?? '未初始化' }}</dd></div>
      </dl>
      <p class="uf-detail-note">按已保存规则中的 Key、账号和模型决定是否强制 Ultrafast，无需另外配置请求入口。</p>
      <p class="uf-detail-note">不修改账号路由、分组或 Key 权限；选择模型不代表上游支持该加速档位。</p>
      <p v-if="instance?.runtime?.drainingRevisions.length" class="uf-detail-note">旧修订 {{ instance.runtime.drainingRevisions.join('、') }} 等待引用释放，新修订 {{ instance.revision }} 已运行</p>
      <p v-if="statusError" class="uf-error">{{ statusError }}</p>
      <p class="uf-footnote">空规则只是取消插件强制，不拦截客户端自己指定 ultrafast</p>
      <template #footer><ZButton @click="detailsOpen = false">关闭</ZButton></template>
    </ZDialog>
    <ZDialog v-model="discardOpen" title="放弃未保存的修改？" width="440px" :draggable="false" :close-on-click-modal="false">
      <p>当前草稿尚未保存，继续将放弃这些修改，已保存规则不会改变</p>
      <template #footer><ZButton @click="discardOpen = false">继续编辑</ZButton><ZButton type="primary" @click="discardDraft">放弃并继续</ZButton></template>
    </ZDialog>
    <ZDialog v-model="deleteOpen" title="删除规则" width="440px" :draggable="false" :before-close="() => !saving">
      <p>确定删除「{{ rules[deleteIndex]?.name }}」？其他规则和客户端自己的档位选择不受影响</p>
      <p v-if="messageType === 'danger' && message" role="alert" class="uf-error">{{ message }}</p>
      <template #footer><ZButton :disabled="saving" @click="deleteOpen = false">取消</ZButton><ZButton type="danger" :disabled="!editable" :loading="saving" @click="confirmDelete">确认删除</ZButton></template>
    </ZDialog>
  </div>
</template>


