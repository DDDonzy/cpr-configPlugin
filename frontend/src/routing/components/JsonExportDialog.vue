<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ZButton, ZDialog, ZTextarea } from '@codex-proxy/ui'
import { Copy } from '@lucide/vue'
import { exportDocument } from '../api'
const open = computed({
  get: () => exportDocument.value !== null,
  set: (value) => {
    if (!value) exportDocument.value = null
  },
})
const area = ref<{ focus: () => void; select: () => void } | null>(null)
const feedback = ref('')
watch(exportDocument, () => {
  feedback.value = ''
})
function selectAll() {
  area.value?.focus()
  area.value?.select()
}
async function copy() {
  const current = exportDocument.value
  if (!current) return
  selectAll()
  try {
    if (!navigator.clipboard?.writeText)
      throw new Error('clipboard unavailable')
    await navigator.clipboard.writeText(current.text)
    if (exportDocument.value === current) feedback.value = '已复制 JSON'
  } catch {
    if (exportDocument.value === current)
      feedback.value = '文本已选中，请按 Ctrl+C（Mac：⌘C）复制。'
  }
}
</script>
<template>
  <ZDialog
    v-model="open"
    :title="`导出 ${exportDocument?.name ?? ''}`"
    description="复制 JSON 后保存为文件，或粘贴到相应的导入窗口。"
    width="46rem"
  >
    <ZTextarea
      ref="area"
      :model-value="exportDocument?.text ?? ''"
      aria-label="导出 JSON"
      readonly
      :rows="13"
      resize="none"
    />
    <p v-if="feedback" class="hint" role="status">{{ feedback }}</p>
    <template #footer
      ><ZButton @click="open = false">关闭</ZButton
      ><ZButton @click="selectAll">选中全部</ZButton
      ><ZButton type="primary" @click="copy"
        ><Copy :size="15" />复制 JSON</ZButton
      ></template
    >
  </ZDialog>
</template>
