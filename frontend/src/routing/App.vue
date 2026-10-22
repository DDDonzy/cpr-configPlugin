<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  ZButton,
  ZCard,
  ZDialog,
  ZEmpty,
  ZInput,
  ZSegmented,
  ZTag,
  ZTextarea,
} from '@codex-proxy/ui'
import { Check, Download, Plus, RefreshCw, Upload } from '@lucide/vue'
import AccountEditor from './components/AccountEditor.vue'
import GroupEditor from './components/GroupEditor.vue'
import HelpTip from './components/HelpTip.vue'
import GroupRow from './components/GroupRow.vue'
import UpstreamAccounts from './components/UpstreamAccounts.vue'
import JsonExportDialog from './components/JsonExportDialog.vue'
import { api, downloadJson } from './api'
import {
  groupId,
  modeLabels,
  normalizeModels,
  validateGroups,
  type ApplyResult,
  type ModelGroup,
  type PolicyRequest,
  type AccountUpstream,
  type Snapshot,
} from './domain'
import {
  accountLabels,
  compactSource,
  matchesAccount,
  upstreamView,
} from './accountPresentation'
const props = defineProps<{ demo: boolean }>()
const snapshot = ref<Snapshot | null>(null),
  loading = ref(false),
  error = ref(''),
  success = ref(''),
  page = ref('upstreams'),
  selected = ref(''),
  accountSearch = ref(''),
  dirty = ref(false),
  refreshKey = ref(0)
const groupDialog = ref(false),
  editGroup = ref<ModelGroup | null>(null),
  seed = ref<string[]>([])
const deleteDialog = ref(false),
  deleteGroup = ref<ModelGroup | null>(null),
  groupBusy = ref(false)
const discardDialog = ref(false),
  pending = ref<(() => void) | null>(null)
const importDialog = ref(false),
  importText = ref(''),
  importError = ref('')
const upstreams = ref<Record<string, AccountUpstream> | null>(null),
  upstreamsLoading = ref(false),
  upstreamWarnings = ref<string[]>([])
let upstreamSeq = 0
const tabs = [
  { label: '上游账号', value: 'upstreams' },
  { label: '账号模型规则', value: 'accounts' },
  { label: '模型分组', value: 'groups' },
]
const account = computed(
  () => snapshot.value?.accounts.find((a) => a.id === selected.value) ?? null,
)
const labels = computed(() => accountLabels(snapshot.value?.accounts ?? []))
const visibleAccounts = computed(
  () =>
    snapshot.value?.accounts.filter((a) =>
      matchesAccount(
        a,
        labels.value.get(a.id) ?? a.name,
        upstreams.value?.[a.id],
        accountSearch.value,
      ),
    ) ?? [],
)
function sourceOf(id: string) {
  const a = snapshot.value?.accounts.find((x) => x.id === id)
  return a ? upstreamView(a, upstreams.value?.[id], upstreamsLoading.value) : null
}
async function loadUpstreams() {
  const seq = ++upstreamSeq
  upstreamsLoading.value = true
  try {
    const value = await api.upstreams()
    if (seq !== upstreamSeq) return
    upstreams.value = value.upstreams
    upstreamWarnings.value = value.warnings
  } catch (e) {
    if (seq !== upstreamSeq) return
    upstreams.value = {}
    upstreamWarnings.value = [`上游地址暂不可读：${(e as Error).message}`]
  } finally {
    if (seq === upstreamSeq) upstreamsLoading.value = false
  }
}
function sidebarSource(id: string) {
  const a = snapshot.value?.accounts.find((x) => x.id === id)
  const view = sourceOf(id)
  return a && view ? compactSource(a, view) : ''
}
function openRules(id: string) {
  switchTo(() => {
    selected.value = id
    page.value = 'accounts'
  })
}
async function load() {
  loading.value = true
  error.value = ''
  try {
    const value = await api.snapshot()
    snapshot.value = value
    if (!value.accounts.some((a) => a.id === selected.value))
      selected.value = value.accounts[0]?.id ?? ''
    refreshKey.value++
    dirty.value = false
    void loadUpstreams()
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    loading.value = false
  }
}
function switchTo(action: () => void) {
  if (dirty.value) {
    pending.value = action
    discardDialog.value = true
  } else action()
}
function discard() {
  discardDialog.value = false
  dirty.value = false
  pending.value?.()
  pending.value = null
}
function createGroup(models: string[] = []) {
  editGroup.value = null
  seed.value = [...models]
  groupDialog.value = true
}
async function saveGroup(group: ModelGroup) {
  if (!snapshot.value) return
  const groups = [
    ...snapshot.value.state.value.groups.filter((g) => g.id !== group.id),
    group,
  ]
  validateGroups(groups)
  snapshot.value.state = await api.saveGroups(
    snapshot.value.state.version,
    groups,
  )
  success.value = '分组已保存；账号规则未自动改变，请预览后应用'
  error.value = ''
}
async function removeGroup() {
  if (!snapshot.value || !deleteGroup.value) return
  groupBusy.value = true
  try {
    snapshot.value.state = await api.saveGroups(
      snapshot.value.state.version,
      snapshot.value.state.value.groups.filter(
        (g) => g.id !== deleteGroup.value?.id,
      ),
    )
    deleteDialog.value = false
    success.value = '分组已删除，已生效的原生模型名单保持不变'
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    groupBusy.value = false
  }
}
async function apply(value: PolicyRequest): Promise<ApplyResult> {
  const result = await api.apply(value)
  if (snapshot.value) {
    snapshot.value.accounts = snapshot.value.accounts.map((a) =>
      a.id === result.account.id ? result.account : a,
    )
    snapshot.value.state = result.state
  }
  dirty.value = false
  error.value = ''
  success.value = result.warnings.length
    ? result.warnings.join('；')
    : result.diff.modeChanged ||
        result.diff.added.length ||
        result.diff.removed.length
      ? '原生模型权限已应用并回读确认'
      : '原生名单未改变，分组关联已保存'
  return result
}
function exportConfig() {
  if (!snapshot.value) return
  downloadJson('model-routing-groups.json', {
    formatVersion: 1,
    groups: snapshot.value.state.value.groups,
    accounts: snapshot.value.accounts.map((a) => ({
      id: a.id,
      name: a.name,
      modelAccess: a.modelAccess,
    })),
  })
}
function importGroups(): ModelGroup[] {
  let parsed: unknown
  try {
    parsed = JSON.parse(importText.value)
  } catch {
    throw new Error('分组 JSON 格式无效')
  }
  const raw = Array.isArray(parsed)
    ? parsed
    : (parsed as { groups?: unknown })?.groups
  if (!Array.isArray(raw))
    throw new Error('请粘贴分组数组，或包含 groups 数组的导出 JSON')
  const groups = raw.map((g) => {
    if (
      !g ||
      typeof g !== 'object' ||
      typeof g.name !== 'string' ||
      !Array.isArray(g.models) ||
      g.models.some((x: unknown) => typeof x !== 'string')
    )
      throw new Error('分组条目需包含 name 和 models 字符串数组')
    return {
      id: typeof g.id === 'string' ? g.id : groupId(),
      name: g.name.trim(),
      models: normalizeModels(g.models),
    }
  })
  validateGroups(groups)
  return groups
}
async function confirmImport() {
  if (!snapshot.value) return
  groupBusy.value = true
  importError.value = ''
  try {
    const incoming = importGroups()
    const merged = [
      ...snapshot.value.state.value.groups.filter(
        (g) => !incoming.some((i) => i.id === g.id),
      ),
      ...incoming,
    ]
    validateGroups(merged)
    snapshot.value.state = await api.saveGroups(
      snapshot.value.state.version,
      merged,
    )
    importDialog.value = false
    success.value = '模型分组已导入，账号原生规则未自动修改'
  } catch (e) {
    importError.value = (e as Error).message
  } finally {
    groupBusy.value = false
  }
}
function toggleDemoTheme() {
  ;(
    window as unknown as { routingGroupsDemoTheme?: () => void }
  ).routingGroupsDemoTheme?.()
}
function openImportGroups() {
  importText.value = ''
  importError.value = ''
  importDialog.value = true
}
function openGroupEditor(group: ModelGroup) {
  editGroup.value = group
  seed.value = []
  groupDialog.value = true
}
function openDelete(group: ModelGroup) {
  deleteGroup.value = group
  deleteDialog.value = true
}
function closeDiscard() {
  discardDialog.value = false
  pending.value = null
}
onMounted(load)
</script>
<template>
  <div class="routing-page">
    <div v-if="demo" class="notice warning">
      <strong>本地演示模式</strong> ·
      当前为模拟账号和模型目录，不会访问或修改真实 CPR。<ZButton
        size="small"
        variant="text"
        @click="toggleDemoTheme"
        >切换演示主题</ZButton
      >
    </div>
    <div class="toolbar">
      <ZSegmented
        :model-value="page"
        aria-label="管理视图"
        :options="tabs"
        @update:model-value="
          switchTo(() => {
            page = $event as string
          })
        "
      />
      <div class="toolbar-actions">
        <HelpTip
          label="插件职责"
          text="规则由 CPR 原生 modelAccess 执行。插件只管理名单，不接管调度或 CPA 内部选号。停用插件不会撤销已写入的名单。"
        />
        <ZButton :loading="loading" @click="switchTo(() => void load())"
          ><RefreshCw :size="15" />重新加载</ZButton
        ><ZButton :disabled="!snapshot" @click="exportConfig"
          ><Download :size="15" />导出配置</ZButton
        >
      </div>
    </div>
    <div v-if="error" class="notice error" role="alert">{{ error }}</div>
    <div v-if="success" class="notice" role="status">{{ success }}</div>
    <template v-if="snapshot">
      <UpstreamAccounts
        v-if="page === 'upstreams'"
        :accounts="snapshot.accounts"
        :labels="labels"
        :upstreams="upstreams"
        :upstreams-loading="upstreamsLoading"
        :upstream-warnings="upstreamWarnings"
        :bindings="snapshot.state.value.bindings"
        :selected-id="selected"
        @select="openRules"
      />
      <div v-else-if="page === 'accounts'" class="account-layout">
        <ZCard class="account-sidebar" title="上游账号"
          ><div class="account-list">
            <ZInput
              v-model="accountSearch"
              aria-label="搜索上游账号"
              placeholder="搜索邮箱、名称或上游地址"
            /><button
              v-for="item in visibleAccounts"
              :key="item.id"
              type="button"
              class="account-choice"
              :class="{ selected: item.id === selected }"
              :aria-pressed="item.id === selected"
              :aria-label="`选择账号 ${labels.get(item.id) ?? item.name}`"
              @click="
                switchTo(() => {
                  selected = item.id
                })
              "
            >
              <span class="account-name"
                ><strong :title="labels.get(item.id)">{{
                  labels.get(item.id) ?? item.name
                }}</strong
                ><small class="account-source" :title="sidebarSource(item.id)">{{
                  sidebarSource(item.id)
                }}</small
                ><small
                  >{{ modeLabels[item.modelAccess.mode] }} ·
                  {{ item.modelAccess.models.length }} 个</small
                ></span
              ><span class="account-indicators"
                ><Check
                  v-if="item.id === selected"
                  :size="17"
                  aria-hidden="true"
                /><ZTag size="small">{{
                  item.enabled ? '启用' : '停用'
                }}</ZTag></span
              >
            </button>
            <p v-if="!visibleAccounts.length" class="hint">没有匹配账号</p>
          </div></ZCard
        >
        <AccountEditor
          v-if="account"
          :key="account.id"
          :account="account"
          :label="labels.get(account.id) ?? account.name"
          :source="sourceOf(account.id)"
          :state="snapshot.state"
          :refresh-key="refreshKey"
          :apply="apply"
          @dirty="dirty = $event"
        />
        <ZEmpty
          v-else
          class="empty-body"
          title="暂无上游账号"
          description="请先在 CPR 原生账号页面添加账号"
        />
      </div>
      <div v-else class="editor">
        <div class="toolbar groups-toolbar">
          <div class="toolbar-actions">
            <ZButton
              :disabled="snapshot.state.value.groups.length >= 32"
              @click="createGroup()"
              ><Plus :size="15" />新建分组</ZButton
            ><ZButton @click="openImportGroups"
              ><Upload :size="15" />批量导入分组</ZButton
            >
          </div>
        </div>
        <div
          v-if="snapshot.state.value.groups.length"
          class="group-record-list"
        >
          <GroupRow
            v-for="group in snapshot.state.value.groups"
            :key="group.id"
            :group="group"
            :accounts="snapshot.accounts"
            :labels="labels"
            :bindings="snapshot.state.value.bindings"
            @edit="openGroupEditor"
            @delete="openDelete"
            @export="
              downloadJson(`${$event.name}.json`, {
                formatVersion: 1,
                groups: [$event],
              })
            "
          />
        </div>
        <ZEmpty
          v-else
          title="还没有模型分组"
          description="批量粘贴模型 ID，创建可复用的白名单或黑名单模板"
        />
      </div>
    </template>
    <JsonExportDialog />
    <GroupEditor
      v-model="groupDialog"
      :group="editGroup"
      :seed-models="seed"
      :save="saveGroup"
    />
    <ZDialog
      v-model="deleteDialog"
      title="删除模型分组"
      description="只删除插件中的模板，不改变已经生效的原生模型权限"
      width="32rem"
      ><p>确认删除“{{ deleteGroup?.name }}”？</p>
      <template #footer
        ><ZButton :disabled="groupBusy" @click="deleteDialog = false"
          >取消</ZButton
        ><ZButton type="danger" :loading="groupBusy" @click="removeGroup"
          >确认删除分组</ZButton
        ></template
      ></ZDialog
    >
    <ZDialog
      v-model="discardDialog"
      title="放弃未应用的草稿？"
      description="服务器上的已生效规则不会改变"
      width="32rem"
      ><p>切换或重新加载后，这个账号的未应用编辑会丢失。</p>
      <template #footer
        ><ZButton @click="closeDiscard">继续编辑</ZButton
        ><ZButton type="primary" @click="discard"
          >放弃草稿并继续</ZButton
        ></template
      ></ZDialog
    >
    <ZDialog
      v-model="importDialog"
      title="批量导入模型分组"
      description="按分组 ID 合并；同 ID 将替换模板内容，不自动修改账号权限"
      width="44rem"
      ><div class="editor">
        <ZTextarea
          v-model="importText"
          aria-label="导入分组 JSON"
          placeholder='[{"name":"Claude 模型","models":["claude-opus-5.5"]}]'
          :rows="10"
        />
        <div v-if="importError" class="notice error" role="alert">
          {{ importError }}
        </div>
      </div>
      <template #footer
        ><ZButton :disabled="groupBusy" @click="importDialog = false"
          >取消</ZButton
        ><ZButton type="primary" :loading="groupBusy" @click="confirmImport"
          >确认导入分组</ZButton
        ></template
      ></ZDialog
    >
  </div>
</template>
