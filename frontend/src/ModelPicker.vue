<script setup lang="ts">
import { computed, ref } from 'vue'
import { ZButton, ZCheckbox, ZInput, ZScrollbar, ZTag } from '@codex-proxy/ui'
import { MAX_MODELS, parseModelIds, validModelId } from './model-ui'
const props = defineProps<{ modelValue: string[]; options: string[]; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>()
const query = ref(''), error = ref('')
const visible = computed(() => [...new Set([...props.options, ...props.modelValue])].filter(m => m.toLowerCase().includes(query.value.trim().toLowerCase())).sort())
function toggle(id: string, checked: boolean) {
  if (props.disabled) return
  const next = checked ? [...new Set([...props.modelValue, id])] : props.modelValue.filter(m => m !== id)
  if (next.length > MAX_MODELS) { error.value = '最多选择 256 个模型'; return }
  error.value = ''; emit('update:modelValue', next)
}
function add() {
  if (props.disabled) return
  const ids = parseModelIds(query.value)
  if (!ids.length || ids.some(id => !validModelId(id))) { error.value = '请输入不含空白、控制字符且不超过 128 字符的模型 ID'; return }
  const next = [...new Set([...props.modelValue, ...ids])]
  if (next.length > MAX_MODELS) { error.value = '最多选择 256 个模型'; return }
  emit('update:modelValue', next); query.value = ''; error.value = ''
}
</script>
<template>
  <div class="uf-model-picker">
    <div class="uf-model-input" @keydown.enter.prevent="add"><ZInput v-model="query" :disabled="disabled" placeholder="搜索或输入模型 ID，逗号分隔可批量添加" /><ZButton size="small" :disabled="disabled || !query.trim()" @click="add">添加</ZButton></div>
    <div v-if="modelValue.length" class="uf-model-tags"><ZTag v-for="id in modelValue" :key="id" closable :disabled="disabled" @close="toggle(id, false)">{{ id }}</ZTag></div>
    <p v-else class="uf-footnote">尚未选择模型，请从候选列表勾选或手动添加</p>
    <ZScrollbar v-if="visible.length" height="132px"><div class="uf-model-options"><ZCheckbox v-for="id in visible" :key="id" :label="id" :disabled="disabled" :model-value="modelValue.includes(id)" @update:model-value="checked => toggle(id, checked)" /></div></ZScrollbar>
    <p v-if="error" class="uf-error" role="alert">{{ error }}</p>
    <p class="uf-footnote">候选来自账号、已保存规则及实例 modelOptions。可填写其他模型，但不代表上游支持 Ultrafast。</p>
  </div>
</template>
