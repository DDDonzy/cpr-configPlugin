<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ZButton, ZDialog, ZFormItem, ZInput, ZTextarea } from '@codex-proxy/ui'
import { groupId, parseModels, type ModelGroup } from '../domain'
const props = defineProps<{
  group: ModelGroup | null
  seedModels: string[]
  save: (group: ModelGroup) => Promise<void>
}>()
const open = defineModel<boolean>({ required: true })
const name = ref(''),
  text = ref(''),
  id = ref(''),
  busy = ref(false),
  error = ref('')
watch(open, (value) => {
  if (value) {
    name.value = props.group?.name ?? ''
    text.value = (props.group?.models ?? props.seedModels).join('\n')
    id.value = props.group?.id ?? groupId()
    error.value = ''
  }
})
const parseable = computed(() => {
  try {
    parseModels(text.value)
    return true
  } catch {
    return false
  }
})
async function save() {
  busy.value = true
  error.value = ''
  try {
    await props.save({
      id: id.value,
      name: name.value.trim(),
      models: parseModels(text.value),
    })
    open.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : '分组保存失败'
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <ZDialog
    v-model="open"
    :title="group ? '编辑模型分组' : '新建模型分组'"
    description="分组是批量编辑模板，保存后不会自动改变账号的原生权限"
    width="42rem"
  >
    <div class="editor">
      <ZFormItem label="分组名称"
        ><ZInput
          v-model="name"
          aria-label="分组名称"
          placeholder="例如：Claude 模型、图片模型"
          :disabled="busy"
      /></ZFormItem>
      <ZFormItem label="精确模型 ID"
        ><ZTextarea
          v-model="text"
          aria-label="分组模型名单"
          placeholder="每行一个模型 ID，或粘贴逗号分隔名单、JSON 数组"
          :rows="10"
          :disabled="busy"
      /></ZFormItem>
      <div v-if="error" class="notice error" role="alert">{{ error }}</div>
    </div>
    <template #footer
      ><ZButton :disabled="busy" @click="open = false">取消</ZButton
      ><ZButton
        type="primary"
        :loading="busy"
        :disabled="!name.trim() || !parseable"
        @click="save"
        >保存分组</ZButton
      ></template
    >
  </ZDialog>
</template>
