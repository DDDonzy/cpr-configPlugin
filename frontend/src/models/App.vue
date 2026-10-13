<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, nextTick, ref } from 'vue'
import {
  ZButton,
  ZCheckbox,
  ZDialog,
  ZEmpty,
  ZFormItem,
  ZIconButton,
  ZInput,
  ZSegmented,
  ZTag,
  ZTextarea,
} from '@codex-proxy/ui'
import {
  ChevronDown,
  ChevronRight,
  Download,
  FileJson,
  GripVertical,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Search,
  Trash2,
  Upload,
} from '@lucide/vue'
import { api, exportDocument } from './api'
import { buildExport } from './codexExport'
import {
  changes,
  clone,
  efforts,
  format,
  integer,
  modalities,
  modalityLabels,
  parseDocument,
  moveId,
  validateOrder,
  validate,
  type Catalog,
  type Diff,
  type Model,
  type Snapshot,
} from './domain'
import JsonExportDialog from './components/JsonExportDialog.vue'
import HelpTip from './components/HelpTip.vue'
const inputSummary = (values: string[]) =>
  values.map((x) => modalityLabels[x] ?? x).join(' / ')
const props = defineProps<{ demo: boolean }>()
const snapshot = ref<Snapshot | null>(null),
  draft = ref<Catalog>({}),
  busy = ref(false),
  error = ref(''),
  success = ref(''),
  search = ref('')
const modelOrder = ref<string[]>([])
const expanded = ref<string[]>([]),
  moreId = ref<string | null>(null)
const dirty = computed(
  () =>
    !!snapshot.value &&
    format(draft.value, modelOrder.value) !==
      format(
        snapshot.value.models,
        snapshot.value.modelOrder ?? Object.keys(snapshot.value.models),
      ),
)
const rows = computed(() =>
  modelOrder.value
    .map((id) => [id, draft.value[id]!] as const)
    .filter(([id, m]) =>
      `${id} ${m.display_name}`
        .toLowerCase()
        .includes(search.value.trim().toLowerCase()),
    ),
)
const count = computed(() => Object.keys(draft.value).length)
const editOpen = ref(false),
  editId = ref<string | null>(null),
  formId = ref(''),
  formName = ref(''),
  formContext = ref(''),
  formOutput = ref(''),
  formLevels = ref<string[]>([]),
  formInputs = ref<string[]>([]),
  formError = ref('')
const deleteId = ref<string | null>(null),
  deleteOpen = ref(false),
  importOpen = ref(false),
  importText = ref(''),
  importMode = ref('merge'),
  importError = ref(''),
  inputFile = ref<HTMLInputElement | null>(null)
const discardOpen = ref(false),
  previewOpen = ref(false),
  prepared = ref<{
    version: string
    models: Catalog
    modelOrder: string[]
    diff: Diff
  } | null>(null),
  saveFailed = ref(false)
const locked = computed(() => busy.value || previewOpen.value)
async function load() {
  busy.value = true
  error.value = ''
  try {
    const value = await api.snapshot()
    validate(value.models)
    snapshot.value = value
    draft.value = clone(value.models)
    modelOrder.value = [...(value.modelOrder ?? Object.keys(value.models))]
    validateOrder(draft.value, modelOrder.value)
    success.value = ''
    expanded.value = []
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    busy.value = false
  }
}
function reload() {
  if (dirty.value) discardOpen.value = true
  else void load()
}
async function discard() {
  discardOpen.value = false
  await load()
}
function toggleExpanded(id: string) {
  expanded.value = expanded.value.includes(id)
    ? expanded.value.filter((x) => x !== id)
    : [...expanded.value, id]
}
function moreDetails(id: string) {
  toggleExpanded(id)
  moreId.value = null
}
function startEdit(id: string | null, copy = false) {
  editId.value = copy ? null : id
  const m = id ? draft.value[id] : undefined
  formId.value = copy ? '' : (id ?? '')
  formName.value = copy
    ? `${m?.display_name ?? ''} 副本`
    : (m?.display_name ?? '')
  formContext.value = m ? String(m.context_window) : ''
  formOutput.value =
    m?.max_output_tokens === undefined ? '' : String(m.max_output_tokens)
  formLevels.value = [...(m?.reasoning_levels ?? [])]
  formInputs.value = [...(m?.input_modalities ?? ['text'])]
  formError.value = ''
  moreId.value = null
  editOpen.value = true
}
function toggle(values: string[], id: string, value: boolean): string[] {
  return value ? [...values, id] : values.filter((x) => x !== id)
}
function applyEdit() {
  try {
    const id = formId.value.trim()
    if (Object.hasOwn(draft.value, id) && id !== editId.value)
      throw new Error('该模型 ID 已存在')
    const m: Model = {
      display_name: formName.value.trim(),
      reasoning_levels: efforts.filter((x) => formLevels.value.includes(x)),
      context_window: integer(formContext.value, '上下文'),
      ...(formOutput.value.trim()
        ? { max_output_tokens: integer(formOutput.value, '输出上限') }
        : {}),
      input_modalities: modalities.filter((x) => formInputs.value.includes(x)),
    }
    const next = clone(draft.value)
    if (editId.value && editId.value !== id) delete next[editId.value]
    Object.defineProperty(next, id, {
      value: m,
      enumerable: true,
      writable: true,
      configurable: true,
    })
    validate(next)
    modelOrder.value = editId.value
      ? modelOrder.value.map((x) => (x === editId.value ? id : x))
      : [...modelOrder.value, id]
    draft.value = next
    editOpen.value = false
    error.value = ''
    success.value = '已更新草稿，点击保存并确认后写入文件'
  } catch (e) {
    formError.value = (e as Error).message
  }
}
function askDelete(id: string) {
  deleteId.value = id
  deleteOpen.value = true
  moreId.value = null
}
function remove() {
  if (deleteId.value) {
    const next = clone(draft.value)
    delete next[deleteId.value]
    draft.value = next
    modelOrder.value = modelOrder.value.filter((x) => x !== deleteId.value)
    expanded.value = expanded.value.filter((x) => x !== deleteId.value)
    success.value = '已从草稿移除，文件尚未改变'
  }
  deleteOpen.value = false
}
function showJson(id?: string) {
  moreId.value = null
  exportDocument.value = buildExport(
    id ? { [id]: draft.value[id]! } : draft.value,
    id ? [id] : modelOrder.value,
    id ? `${id}.json` : 'models.common.json',
    dirty.value,
  )
}
function openImport() {
  importText.value = ''
  importError.value = ''
  importMode.value = 'merge'
  importOpen.value = true
}
async function importFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  try {
    if (file.size > 1024 * 1024) throw new Error('文件最多 1 MiB')
    importText.value = await file.text()
    importError.value = ''
  } catch (e) {
    importError.value = (e as Error).message
  } finally {
    input.value = ''
  }
}
function importDraft() {
  try {
    const { models: incoming, modelOrder: incomingOrder } = parseDocument(
      importText.value,
    )
    const next =
      importMode.value === 'replace'
        ? clone(incoming)
        : { ...clone(draft.value), ...clone(incoming) }
    validate(next)
    modelOrder.value =
      importMode.value === 'replace'
        ? [...incomingOrder]
        : [
            ...modelOrder.value,
            ...incomingOrder.filter((id) => !Object.hasOwn(draft.value, id)),
          ]
    draft.value = next
    importOpen.value = false
    error.value = ''
    success.value = 'JSON 已导入草稿，尚未写入文件'
  } catch (e) {
    importError.value = (e as Error).message
  }
}
async function preview() {
  if (!snapshot.value) return
  busy.value = true
  error.value = ''
  saveFailed.value = false
  try {
    validate(draft.value)
    const models = clone(draft.value)
    const version = snapshot.value.version
    const order = [...modelOrder.value]
    const result = await api.preview(version, models, order)
    prepared.value = { version, models, modelOrder: order, diff: result.diff }
    previewOpen.value = true
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    busy.value = false
  }
}
async function confirm() {
  if (!prepared.value) return
  busy.value = true
  error.value = ''
  try {
    const result = await api.save(
      prepared.value.version,
      prepared.value.models,
      prepared.value.modelOrder,
    )
    snapshot.value = result.snapshot
    draft.value = clone(result.snapshot.models)
    modelOrder.value = [
      ...(result.snapshot.modelOrder ?? Object.keys(result.snapshot.models)),
    ]
    previewOpen.value = false
    prepared.value = null
    success.value = '配置文件已保存，旧版本已备份。账号规则和模型调度未改变'
  } catch (e) {
    error.value = (e as Error).message
    saveFailed.value = true
  } finally {
    busy.value = false
  }
}
const summary = computed(() =>
  snapshot.value
    ? changes(
        snapshot.value.models,
        draft.value,
        snapshot.value.modelOrder ?? Object.keys(snapshot.value.models),
        modelOrder.value,
      )
    : null,
)
const dragId = ref<string | null>(null),
  dragStarted = ref(false),
  dropBefore = ref<string | null>(null),
  dragX = ref(0),
  dragY = ref(0)
let candidateOrder: string[] = [],
  startY = 0,
  activePointer: number | null = null,
  captureTarget: HTMLElement | null = null
function cleanupDrag() {
  window.removeEventListener('pointermove', dragMove)
  window.removeEventListener('pointerup', dragFinish)
  window.removeEventListener('pointercancel', dragCancel)
  if (
    captureTarget &&
    activePointer !== null &&
    captureTarget.hasPointerCapture(activePointer)
  )
    captureTarget.releasePointerCapture(activePointer)
  activePointer = null
  captureTarget = null
  dragId.value = null
  dragStarted.value = false
  dropBefore.value = null
}
function startDrag(e: PointerEvent, id: string) {
  if (locked.value || search.value.trim() || e.button !== 0) return
  cleanupDrag()
  dragId.value = id
  candidateOrder = [...modelOrder.value]
  startY = e.clientY
  dragX.value = e.clientX
  dragY.value = e.clientY
  activePointer = e.pointerId
  captureTarget = e.currentTarget as HTMLElement
  captureTarget.setPointerCapture(e.pointerId)
  e.preventDefault()
  window.addEventListener('pointermove', dragMove)
  window.addEventListener('pointerup', dragFinish)
  window.addEventListener('pointercancel', dragCancel)
}
function dragMove(e: PointerEvent) {
  if (!dragId.value || e.pointerId !== activePointer) return
  dragX.value = e.clientX
  dragY.value = e.clientY
  if (!dragStarted.value && Math.abs(e.clientY - startY) < 4) return
  dragStarted.value = true
  const others = [
    ...document.querySelectorAll<HTMLElement>('.model-record[data-model-id]'),
  ].filter((el) => el.dataset.modelId !== dragId.value)
  const index = others.findIndex((el) => {
    const rect = el.getBoundingClientRect()
    return e.clientY < rect.top + rect.height / 2
  })
  const at = index < 0 ? others.length : index
  candidateOrder = moveId(modelOrder.value, dragId.value, at)
  dropBefore.value = others[at]?.dataset.modelId ?? null
  e.preventDefault()
}
function dragFinish(e: PointerEvent) {
  if (e.pointerId !== activePointer) return
  if (dragStarted.value && !locked.value) {
    modelOrder.value = [...candidateOrder]
    success.value = '顺序已调整，点击保存后写入文件'
  }
  cleanupDrag()
}
function dragCancel(e: PointerEvent) {
  if (e.pointerId === activePointer) cleanupDrag()
}
function handleOrderKey(e: KeyboardEvent, id: string) {
  if (
    locked.value ||
    search.value.trim() ||
    !['ArrowUp', 'ArrowDown'].includes(e.key)
  )
    return
  e.preventDefault()
  const index = modelOrder.value.indexOf(id)
  modelOrder.value = moveId(
    modelOrder.value,
    id,
    index + (e.key === 'ArrowUp' ? -1 : 1),
  )
  success.value = '顺序已调整，点击保存后写入文件'
  void nextTick(() => {
    for (const button of document.querySelectorAll<HTMLButtonElement>(
      '.drag-handle',
    ))
      if (button.getAttribute('aria-label') === `拖动排序 ${id}`) button.focus()
  })
}
onBeforeUnmount(cleanupDrag)
onMounted(load)
</script>
<template>
  <div class="capabilities-page">
    <div v-if="props.demo" class="notice warning">
      <strong>本地演示模式</strong> · 模拟数据，不访问或修改 US 服务器。
    </div>
    <div class="toolbar">
      <div class="search-tools">
        <ZInput
          v-model="search"
          aria-label="搜索模型"
          placeholder="搜索模型 ID 或显示名称"
          ><template #prefix><Search :size="16" /></template></ZInput
        ><ZTag>{{ count }} 个模型</ZTag>
      </div>
      <div class="actions">
        <HelpTip
          label="插件职责"
          text="只维护独立的模型基础 JSON。不会修改账号白名单、路由、CPA 或本地 Codex 设置。这里的能力值是人工声明，不代表上游已验证支持。"
        /><ZButton :disabled="locked" :loading="busy" @click="reload"
          ><RefreshCw :size="15" />重新加载</ZButton
        ><ZButton :disabled="!snapshot || locked" @click="openImport"
          ><Upload :size="15" />导入 JSON</ZButton
        ><ZButton :disabled="!snapshot || locked" @click="showJson()"
          ><Download :size="15" />导出 JSON</ZButton
        ><ZButton
          type="primary"
          :disabled="!snapshot || locked"
          @click="startEdit(null)"
          ><Plus :size="15" />新增模型</ZButton
        >
      </div>
    </div>
    <div v-if="error && !previewOpen" class="notice error" role="alert">
      {{ error }}
    </div>
    <div v-if="success" class="notice" role="status">{{ success }}</div>
    <div class="model-records" v-if="rows.length">
      <div
        v-for="[id, m] in rows"
        :key="id"
        class="model-record"
        :data-model-id="id"
        :class="{
          dragging: dragId === id,
          'drop-before': dropBefore === id,
          'drop-after':
            dropBefore === null &&
            dragStarted &&
            id === modelOrder[modelOrder.length - 1],
        }"
      >
        <div class="model-header">
          <button
            type="button"
            class="drag-handle"
            :disabled="locked || !!search.trim()"
            :aria-label="`拖动排序 ${id}`"
            :title="
              search.trim()
                ? '清空搜索后可排序'
                : '拖动调整顺序；也可使用上下方向键'
            "
            @pointerdown="startDrag($event, id)"
            @keydown="handleOrderKey($event, id)"
          >
            <GripVertical :size="17" />
          </button>
          <button
            class="expand-button"
            :aria-label="`${expanded.includes(id) ? '收起' : '展开'}模型 ${id}`"
            :aria-expanded="expanded.includes(id)"
            @click="toggleExpanded(id)"
          >
            <ChevronDown v-if="expanded.includes(id)" :size="17" /><ChevronRight
              v-else
              :size="17"
            />
          </button>
          <div class="model-identity">
            <div class="avatar">
              {{ Array.from(m.display_name)[0]?.toUpperCase() }}
            </div>
            <div class="identity-text">
              <strong :title="m.display_name">{{ m.display_name }}</strong
              ><span :title="id">{{ id }}</span>
            </div>
          </div>
          <div class="model-efforts">
            <small>思考等级</small
            ><span :title="m.reasoning_levels.join(' · ')">{{
              m.reasoning_levels.join(' · ') || '未声明'
            }}</span>
          </div>
          <div class="token-cell">
            <small>上下文</small
            ><strong>{{ m.context_window.toLocaleString() }}</strong>
          </div>
          <div class="token-cell">
            <small>最大输出</small
            ><strong>{{
              m.max_output_tokens?.toLocaleString() ?? '未声明'
            }}</strong>
          </div>
          <div class="input-types">
            <ZTag v-for="type in m.input_modalities" :key="type" size="small">{{
              modalityLabels[type]
            }}</ZTag>
          </div>
          <div class="record-actions">
            <ZIconButton
              :disabled="locked"
              :aria-label="`编辑模型 ${id}`"
              :title="'编辑模型'"
              @click="startEdit(id)"
              ><Pencil :size="17" /></ZIconButton
            ><ZIconButton
              :disabled="locked"
              :aria-label="`删除模型 ${id}`"
              title="删除模型"
              class="danger-button"
              @click="askDelete(id)"
              ><Trash2 :size="17"
            /></ZIconButton>
            <div class="more-wrap">
              <ZIconButton
                :disabled="locked"
                :aria-label="`更多操作 ${id}`"
                title="更多"
                @click="moreId = moreId === id ? null : id"
                ><MoreHorizontal :size="18"
              /></ZIconButton>
              <div v-if="moreId === id" class="more-menu">
                <button @click="showJson(id)">
                  <FileJson :size="15" />查看 JSON</button
                ><button @click="startEdit(id, true)">
                  <Plus :size="15" />复制为新模型</button
                ><button @click="moreDetails(id)">
                  {{ expanded.includes(id) ? '收起' : '展开' }}详情
                </button>
              </div>
            </div>
          </div>
        </div>
        <div v-if="expanded.includes(id)" class="model-details">
          <div>
            <h3>模型基础数据</h3>
            <div class="detail-grid">
              <span>显示名称</span><strong>{{ m.display_name }}</strong
              ><span>模型 ID</span><code>{{ id }}</code
              ><span>支持思考等级</span
              ><strong>{{ m.reasoning_levels.join(' / ') || '未声明' }}</strong
              ><span>上下文 / 输出</span
              ><strong
                >{{ m.context_window.toLocaleString() }} /
                {{ m.max_output_tokens?.toLocaleString() ?? '未声明' }}
                tokens</strong
              ><span>支持输入</span
              ><strong>{{ inputSummary(m.input_modalities) }}</strong>
            </div>
          </div>
          <div>
            <h3>JSON 预览</h3>
            <pre>{{ format({ [id]: m }) }}</pre>
          </div>
        </div>
      </div>
    </div>
    <ZEmpty
      v-else-if="snapshot"
      :title="count ? '没有匹配的模型' : '还没有模型配置'"
      description="新增模型或导入通用 JSON 配置"
    />
    <div class="save-bar" v-if="snapshot">
      <div class="file-status">
        <FileJson :size="17" /><span>{{
          dirty ? '有未保存的修改' : '与文件一致'
        }}</span
        ><HelpTip
          label="保存文件"
          :text="`维护文件：${snapshot.filePath}。保存前预览新增、修改和删除；写入采用版本检查、旧文件备份和原子替换。`"
        /><ZTag v-if="dirty && summary"
          >新增 {{ summary.added.length }} · 修改 {{ summary.changed.length }} ·
          删除 {{ summary.removed.length
          }}{{ summary.orderChanged ? ' · 顺序已调整' : '' }}</ZTag
        >
      </div>
      <ZButton
        type="primary"
        :disabled="!dirty || locked"
        :loading="busy"
        @click="preview"
        ><Save :size="16" />保存</ZButton
      >
    </div>
    <ZDialog
      v-model="editOpen"
      :title="editId ? '编辑模型' : '新增模型'"
      width="43rem"
      :before-close="() => !busy"
      ><div class="editor-form">
        <div class="form-columns">
          <ZFormItem label="模型 ID"
            ><ZInput
              v-model="formId"
              aria-label="模型 ID"
              placeholder="例如：gpt-6-astra" /></ZFormItem
          ><ZFormItem label="显示名称"
            ><ZInput
              v-model="formName"
              aria-label="显示名称"
              placeholder="例如：GPT-6 Astra" /></ZFormItem
          ><ZFormItem label="上下文 token 上限"
            ><ZInput
              v-model="formContext"
              aria-label="上下文 token 上限"
              inputmode="numeric"
              placeholder="正整数" /></ZFormItem
          ><ZFormItem label="最大输出 tokens"
            ><ZInput
              v-model="formOutput"
              aria-label="最大输出 tokens"
              inputmode="numeric"
              placeholder="未知时留空"
          /></ZFormItem>
        </div>
        <div class="field">
          <div class="field-heading">
            支持的思考等级<HelpTip
              label="思考等级"
              text="仅声明支持范围，不设置默认等级。ultra、默认思考等客户端策略不写入基础文件。"
            />
          </div>
          <div class="checkbox-grid">
            <ZCheckbox
              v-for="level in efforts"
              :key="level"
              :model-value="formLevels.includes(level)"
              :label="level"
              @update:model-value="
                formLevels = toggle(formLevels, level, $event)
              "
            />
          </div>
        </div>
        <div class="field">
          <div class="field-heading">
            支持输入类型<HelpTip
              label="输入类型"
              text="图片输入不等于图片生成。至少选择一种输入类型。"
            />
          </div>
          <div class="checkbox-grid">
            <ZCheckbox
              v-for="type in modalities"
              :key="type"
              :model-value="formInputs.includes(type)"
              :label="modalityLabels[type]"
              @update:model-value="
                formInputs = toggle(formInputs, type, $event)
              "
            />
          </div>
        </div>
        <div v-if="formError" class="notice error" role="alert">
          {{ formError }}
        </div>
      </div>
      <template #footer
        ><ZButton @click="editOpen = false">取消</ZButton
        ><ZButton type="primary" @click="applyEdit">确定</ZButton></template
      ></ZDialog
    >
    <ZDialog
      v-model="deleteOpen"
      title="删除模型"
      width="32rem"
      description="先从草稿移除；保存并确认后才修改文件。不会删除 CPR 上游账号或改变路由规则。"
      ><p>确认移除“{{ deleteId }}”？</p>
      <template #footer
        ><ZButton @click="deleteOpen = false">取消</ZButton
        ><ZButton type="danger" @click="remove">从草稿移除</ZButton></template
      ></ZDialog
    >
    <ZDialog
      v-model="importOpen"
      title="导入通用 JSON"
      width="46rem"
      description="支持模型 ID 为键的五字段配置，不接受 Codex 目录或客户端策略字段。"
      ><div class="editor-form">
        <div class="actions">
          <ZSegmented
            v-model="importMode"
            aria-label="导入方式"
            :options="[
              { label: '合并，同 ID 更新', value: 'merge' },
              { label: '替换全部', value: 'replace' },
            ]"
          /><ZButton @click="inputFile?.click()"
            ><Upload :size="15" />选择文件</ZButton
          >
        </div>
        <input
          ref="inputFile"
          type="file"
          accept=".json,application/json"
          class="sr-only"
          aria-label="选择导入 JSON 文件"
          @change="importFile"
        /><ZTextarea
          v-model="importText"
          aria-label="导入 JSON 内容"
          :rows="13"
          resize="none"
          placeholder='{"model-id":{"display_name":"Name","reasoning_levels":["high"],"context_window":1000000,"max_output_tokens":128000,"input_modalities":["text"]}}'
        />
        <div v-if="importError" class="notice error" role="alert">
          {{ importError }}
        </div>
      </div>
      <template #footer
        ><ZButton @click="importOpen = false">取消</ZButton
        ><ZButton type="primary" @click="importDraft"
          >导入草稿</ZButton
        ></template
      ></ZDialog
    >
    <ZDialog
      v-model="previewOpen"
      title="保存模型配置"
      width="45rem"
      :before-close="() => !busy"
      description="确认后只写入独立 JSON 文件；不会改变账号权限或启用 Codex 配置。"
      ><div v-if="prepared" class="editor-form">
        <div v-if="error" class="notice error" role="alert">
          {{ error }}
          <p v-if="saveFailed">保存结果需要重新加载确认，不自动重试或回滚。</p>
        </div>
        <div class="diff-grid">
          <div
            v-for="kind in ['added', 'changed', 'removed'] as const"
            :key="kind"
          >
            <h3>
              {{ { added: '新增', changed: '修改', removed: '删除' }[kind] }}
              {{ prepared.diff[kind].length }}
            </h3>
            <ul>
              <li v-for="id in prepared.diff[kind]" :key="id" :title="id">
                {{ id }}
              </li>
            </ul>
          </div>
        </div>
        <div v-if="!Object.keys(prepared.models).length" class="notice warning">
          此次保存将清空全部模型配置。
        </div>
        <div v-if="prepared.diff.orderChanged" class="notice">
          模型顺序已调整，将按当前顺序写入 JSON 文件。
        </div>
        <p class="hint">
          写入前再次核对文件版本，原文件会先备份。其他页面的较新修改不会被覆盖。
        </p>
      </div>
      <template #footer
        ><ZButton :disabled="busy" @click="previewOpen = false">取消</ZButton
        ><ZButton
          type="primary"
          :disabled="saveFailed"
          :loading="busy"
          @click="confirm"
          >确认保存</ZButton
        ></template
      ></ZDialog
    >
    <ZDialog
      v-model="discardOpen"
      title="放弃未保存的修改？"
      width="32rem"
      description="重新加载会丢弃当前草稿，服务器文件保持不变。"
      ><template #footer
        ><ZButton @click="discardOpen = false">继续编辑</ZButton
        ><ZButton type="primary" @click="discard"
          >放弃并重新加载</ZButton
        ></template
      ></ZDialog
    >
    <div
      v-if="dragStarted"
      class="drag-label"
      :style="{ left: `${dragX + 12}px`, top: `${dragY + 12}px` }"
    >
      <GripVertical :size="16" />{{ draft[dragId!]?.display_name }}
    </div>
    <JsonExportDialog />
  </div>
</template>
