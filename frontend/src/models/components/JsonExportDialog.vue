<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ZButton, ZDialog, ZSegmented, ZTag, ZTextarea } from '@codex-proxy/ui'
import { Copy, Download } from '@lucide/vue'
import { exportDocument } from '../api'
import HelpTip from './HelpTip.vue'
const tab = ref<'base' | 'codex'>('base')
const options = [
  { label: 'Base', value: 'base' },
  { label: 'Codex', value: 'codex' },
]
const active = computed(() => exportDocument.value?.[tab.value] ?? null)
const open = computed({
  get: () => exportDocument.value !== null,
  set: (value) => {
    if (!value) exportDocument.value = null
  },
})
const area = ref<{ focus: () => void; select: () => void } | null>(null),
  feedback = ref('')
watch(exportDocument, () => {
  tab.value = 'base'
  feedback.value = ''
})
watch(tab, () => {
  feedback.value = ''
})
function selectAll() {
  area.value?.focus()
  area.value?.select()
}
const batDialogOpen = ref(false)
const batLink = 'https://github.com/DDDonzy/cpr-configPlugin/releases/latest/download/codex-setup.bat'
const batArea = ref<{ focus: () => void; select: () => void } | null>(null)
const batFeedback = ref('')
function downloadBat() {
  batFeedback.value = ''
  batDialogOpen.value = true
}
async function copyBatLink() {
  batArea.value?.focus()
  batArea.value?.select()
  try {
    if (!navigator.clipboard?.writeText) throw new Error('clipboard unavailable')
    await navigator.clipboard.writeText(batLink)
    batFeedback.value = '下载链接已复制，请粘贴到浏览器地址栏打开。'
  } catch {
    batFeedback.value = '下载链接已选中，请按 Ctrl+C（Mac：⌘C）复制，再粘贴到浏览器地址栏打开。'
  }
}
async function copy() {
  const document = exportDocument.value,
    current = active.value,
    selected = tab.value
  if (!document || !current || current.text === null) return
  selectAll()
  try {
    if (!navigator.clipboard?.writeText)
      throw new Error('clipboard unavailable')
    await navigator.clipboard.writeText(current.text)
    if (exportDocument.value === document && tab.value === selected)
      feedback.value = `已复制 ${selected === 'base' ? 'Base' : 'Codex'} JSON`
  } catch {
    if (exportDocument.value === document && tab.value === selected)
      feedback.value = '文本已选中，请按 Ctrl+C（Mac：⌘C）复制。'
  }
}
</script>
<template>
  <ZDialog v-model="open" title="导出 JSON" width="48rem">
    <div class="export-content">
      <div class="export-toolbar">
        <ZSegmented v-model="tab" aria-label="导出格式" :options="options" />
        <div class="export-meta">
          <ZTag size="small">{{ active?.name }}</ZTag
          ><ZTag v-if="exportDocument?.isDraft" size="small">未保存草稿</ZTag
          ><HelpTip
            label="导出规则"
            text="Base 保留当前基础配置。Codex 使用同一批模型及顺序，按模板生成 models.json；开启详细程度和摘要参数支持，工具结果截断预算固定为 128000，不生成默认思考等级。导出不会写入服务器或本地 Codex。通用 BAT 不含密钥，运行时读取已保存配置，不包含当前草稿。"
          />
        </div>
      </div>
      <div v-if="active?.error" class="notice error" role="alert">
        {{ active.error }}
      </div>
      <ZTextarea
        v-if="active?.text !== null"
        ref="area"
        :model-value="active?.text ?? ''"
        :aria-label="`导出 ${tab === 'base' ? 'Base' : 'Codex'} JSON`"
        readonly
        :rows="15"
        resize="none"
      />
      <p v-if="feedback" class="hint" role="status">{{ feedback }}</p>
    </div>
    <template #footer
      ><ZButton @click="open = false">关闭</ZButton
      ><ZButton v-if="tab === 'base'" :disabled="!active || active.text === null" @click="selectAll"
        >选中全部</ZButton
      ><ZButton v-else @click="downloadBat"><Download :size="15" />下载 BAT</ZButton
      ><ZButton
        type="primary"
        :disabled="!active || active.text === null"
        @click="copy"
        ><Copy :size="15" />复制 JSON</ZButton
      ></template
    >
  </ZDialog>
  <ZDialog v-model="batDialogOpen" title="下载通用 BAT" width="42rem">
    <p class="hint">CPR 的隔离页面不允许直接下载。请复制下面的 Release 下载链接，在浏览器地址栏打开；不需要修改 CPR 或放宽 iframe 权限。</p>
    <ZTextarea ref="batArea" :model-value="batLink" aria-label="BAT 下载链接" readonly :rows="3" resize="none" />
    <p class="hint">BAT 已包含在插件发布包中，也可以从插件包的 downloads 资源中提取。它不包含任何 API Key；运行后输入 URL 和 Key，读取服务器已保存配置，不包含当前未保存草稿。</p>
    <p v-if="batFeedback" class="hint" role="status">{{ batFeedback }}</p>
    <template #footer><ZButton @click="batDialogOpen = false">关闭</ZButton><ZButton type="primary" @click="copyBatLink"><Copy :size="15" />复制下载链接</ZButton></template>
  </ZDialog>
</template>
