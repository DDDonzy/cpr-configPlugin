<script setup lang="ts">
import { computed, nextTick, ref, watch, watchEffect } from 'vue'
import {
  ZButton,
  ZCard,
  ZCheckbox,
  ZDialog,
  ZSegmented,
  ZTag,
} from '@codex-proxy/ui'
import { Download, Save } from '@lucide/vue'
import { api, downloadJson } from '../api'
import HelpTip from './HelpTip.vue'
import ModelPreview from './ModelPreview.vue'
import {
  isOAuth,
  kindLabel,
  nativeName,
  planLabel,
  safeFileName,
  statusView,
  type UpstreamView,
} from '../accountPresentation'
import {
  expandGroups,
  modeLabels,
  policy,
  samePolicy,
  type Account,
  type ApplyResult,
  type GroupState,
  type Mode,
  type PolicyDiff,
  type PolicyRequest,
} from '../domain'
const props = defineProps<{
  account: Account
  label: string
  source: UpstreamView | null
  state: GroupState
  refreshKey: number
  apply: (request: PolicyRequest) => Promise<ApplyResult>
}>()
const emit = defineEmits<{ dirty: [boolean] }>()
const plan = computed(() => planLabel(props.account))
const status = computed(() => statusView(props.account))
const name = computed(() => nativeName(props.account))
const cprGroups = computed(() =>
  (props.account.identity?.groups ?? []).map((g) => g.name).join('、'),
)
const mode = ref<Mode>('all'),
  groupIds = ref<string[]>([]),
  message = ref(''),
  notice = ref(''),
  busy = ref(false)
const previewOpen = ref(false),
  previewDiff = ref<PolicyDiff | null>(null),
  prepared = ref<PolicyRequest | null>(null),
  applyFailed = ref(false)
const loadedGroupIds = ref<string[]>([])
const legacy = ref(false)
const groupsTouched = ref(false)
const modes = [
  { label: '不限制', value: 'all' },
  { label: '白名单', value: 'allowlist' },
  { label: '黑名单', value: 'denylist' },
]
function reset() {
  mode.value = props.account.modelAccess.mode
  groupIds.value = []
  loadedGroupIds.value = []
  groupsTouched.value = false
  notice.value = ''
  message.value = ''
  const binding = props.state.value.bindings[props.account.id]
  legacy.value =
    mode.value !== 'all' &&
    (!binding ||
      !samePolicy(binding.appliedPolicy, props.account.modelAccess) ||
      binding.manualModels.length > 0 ||
      binding.groupIds.length === 0 ||
      binding.groupIds.some(
        (id) => !props.state.value.groups.some((g) => g.id === id),
      ))
  if (legacy.value) {
    notice.value =
      '当前原生名单尚未完整关联分组，暂按已生效名单只读展示。可导出有效规则，在“模型分组”页创建对应分组，再勾选分组并保存确认；现有规则不会自动改变。'
  } else if (binding && mode.value !== 'all') {
    groupIds.value = [...binding.groupIds]
    loadedGroupIds.value = [...binding.groupIds].sort()
  }
}
watch(
  () =>
    `${props.account.id}:${JSON.stringify(props.account.modelAccess)}:${props.refreshKey}`,
  reset,
  { immediate: true },
)
const showingLegacy = computed(
  () => legacy.value && !groupsTouched.value && mode.value !== 'all',
)
const resolved = computed(() => {
  if (mode.value === 'all') return { models: [] as string[], error: '' }
  if (showingLegacy.value)
    return { models: [...props.account.modelAccess.models], error: '' }
  try {
    return {
      models: expandGroups([], groupIds.value, props.state.value.groups),
      error: '',
    }
  } catch (e) {
    return {
      models: [] as string[],
      error: e instanceof Error ? e.message : '名单展开失败',
    }
  }
})
const draft = computed(() => {
  try {
    return policy(mode.value, resolved.value.models)
  } catch {
    return null
  }
})
const associationChanged = computed(
  () =>
    mode.value !== 'all' &&
    JSON.stringify([...groupIds.value].sort()) !==
      JSON.stringify(loadedGroupIds.value),
)
const changed = computed(
  () =>
    !!resolved.value.error ||
    associationChanged.value ||
    !draft.value ||
    !samePolicy(draft.value, props.account.modelAccess),
)
watchEffect(() => emit('dirty', changed.value))
function selectGroup(id: string, value: boolean) {
  groupsTouched.value = true
  groupIds.value = value
    ? [...new Set([...groupIds.value, id])]
    : groupIds.value.filter((x) => x !== id)
}
function request(): PolicyRequest {
  if (showingLegacy.value) throw new Error('请先选择模型分组')
  if (resolved.value.error) throw new Error(resolved.value.error)
  return {
    accountId: props.account.id,
    before: {
      mode: props.account.modelAccess.mode,
      models: [...props.account.modelAccess.models],
    },
    after: policy(mode.value, resolved.value.models),
    groupIds: mode.value === 'all' ? [] : [...groupIds.value],
    manualModels: [],
    expectedGroupVersion: props.state.version,
  }
}
async function preview() {
  busy.value = true
  message.value = ''
  applyFailed.value = false
  try {
    const value = request()
    const result = await api.preview(value)
    prepared.value = value
    previewDiff.value = result.diff
    previewOpen.value = true
  } catch (e) {
    message.value = (e as Error).message
  } finally {
    busy.value = false
  }
}
async function confirm() {
  if (!prepared.value) return
  try {
    policy(prepared.value.after.mode, prepared.value.after.models, true)
  } catch (e) {
    message.value = (e as Error).message
    return
  }
  busy.value = true
  message.value = ''
  try {
    await props.apply(prepared.value)
    await nextTick()
    reset()
    previewOpen.value = false
    prepared.value = null
  } catch (e) {
    message.value = (e as Error).message
    applyFailed.value = true
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <div class="editor">
    <ZCard>
      <div class="editor-head">
        <div class="editor-identity">
          <h2 class="editor-title" :title="label">{{ label }}</h2>
          <div class="status-line">
            <ZTag :type="isOAuth(account) ? 'primary' : 'info'">{{
              kindLabel(account)
            }}</ZTag
            ><ZTag v-if="plan">{{ plan }}</ZTag
            ><ZTag :type="status.type">{{ status.text }}</ZTag
            ><span
              >原生规则：{{ modeLabels[account.modelAccess.mode] }} ·
              {{ account.modelAccess.models.length }} 个模型</span
            >
          </div>
          <dl class="identity-facts">
            <div>
              <dt>上游</dt>
              <dd :title="source?.detail ?? source?.text">
                {{ source?.text ?? '—'
                }}<span v-if="source?.detail" class="muted">
                  · {{ source.detail }}</span
                >
              </dd>
            </div>
            <div v-if="name"><dt>CPR 名称</dt><dd>{{ name }}</dd></div>
            <div v-if="cprGroups"><dt>CPR 分组</dt><dd>{{ cprGroups }}</dd></div>
            <div>
              <dt>账号 ID</dt>
              <dd><code>{{ account.id }}</code></dd>
            </div>
          </dl>
        </div>
        <ZButton
          size="small"
          @click="
            downloadJson(
              `${safeFileName(label)}-model-access.json`,
              account.modelAccess,
            )
          "
          ><Download :size="14" />导出有效规则</ZButton
        >
      </div>
      <div class="field" style="margin-top: 18px">
        <div class="field-heading">
          <label class="field-label">模型限制模式</label
          ><HelpTip
            label="模型限制模式"
            text="不限制：清除账号的模型限制。白名单：仅允许最终名单中的精确模型 ID。黑名单：排除最终名单中的精确模型 ID。实际可用性仍由供应商能力、CPR 权限和调度决定。"
          />
        </div>
        <ZSegmented
          v-model="mode"
          aria-label="模型限制模式"
          :options="modes"
          :disabled="busy"
        />
      </div>
    </ZCard>
    <div v-if="notice && showingLegacy" class="notice warning">
      {{ notice }}
    </div>
    <template v-if="mode !== 'all'">
      <ZCard
        ><template #title
          ><span class="section-title"
            >引用模型分组<HelpTip
              label="引用模型分组"
              text="将分组展开为精确模型名单。修改模板不会自动改变已生效规则，需要重新预览并应用。" /></span
        ></template>
        <div v-if="state.value.groups.length" class="model-groups">
          <div
            v-for="group in state.value.groups"
            :key="group.id"
            class="group-choice"
          >
            <ZCheckbox
              :model-value="groupIds.includes(group.id)"
              :label="`${group.name}（${group.models.length}）`"
              :disabled="busy"
              @update:model-value="selectGroup(group.id, $event)"
            />
          </div>
        </div>
        <p v-else class="hint">还没有分组，可以在“模型分组”页批量创建。</p>
      </ZCard>
      <ZCard
        ><template #title
          ><span class="section-title"
            >最终模型名单<HelpTip
              label="最终模型名单"
              text="所选分组的模型合并去重后，作为账号原生白名单或黑名单。此处仅作预览，请通过分组修改，预览并确认应用后才生效。未关联分组的旧规则保持只读，须先迁移到分组。" /></span
        ></template>
        <div class="row">
          <ZTag
            >{{ modeLabels[mode] }} · {{ resolved.models.length }} 个模型</ZTag
          >
        </div>
        <ModelPreview :models="resolved.models" :mode="mode" />
        <p v-if="resolved.error" class="hint" role="alert">
          {{ resolved.error }}
        </p>
        <p v-else-if="!resolved.models.length" class="hint">
          白名单和黑名单至少需要一个模型。
        </p>
      </ZCard>
    </template>
    <div v-if="message && !previewOpen" class="notice error" role="alert">
      {{ message }}
    </div>
    <div class="action-footer">
      <div class="row">
        <ZButton :disabled="busy" @click="reset">恢复已生效规则</ZButton
        ><ZButton
          type="primary"
          :loading="busy"
          :disabled="!changed || !draft || !!resolved.error || showingLegacy"
          @click="preview"
          ><Save :size="15" />保存</ZButton
        >
      </div>
    </div>
    <ZDialog
      v-model="previewOpen"
      title="确认原生模型权限变更"
      :description="
        previewDiff
          ? `目标账号：${label}，${samePolicy(previewDiff.before, previewDiff.after) ? '原生名单不变，仅保存分组关联' : '确认后写入原生模型权限'}`
          : ''
      "
      width="44rem"
    >
      <div v-if="previewDiff" class="editor">
        <div v-if="message" class="notice error" role="alert">
          {{ message }}
          <p v-if="applyFailed">
            先关闭并重新加载，确认原生值后再操作，不自动重复写入。
          </p>
        </div>
        <div class="row">
          <ZTag
            >{{ modeLabels[previewDiff.before.mode] }} →
            {{ modeLabels[previewDiff.after.mode] }}</ZTag
          ><span>最终 {{ previewDiff.after.models.length }} 个模型</span>
        </div>
        <div v-if="previewDiff.after.mode === 'all'" class="notice warning">
          将清除该账号的模型限制。此操作不是把账号停用。
        </div>
        <div class="diff-grid">
          <div>
            <p class="diff-heading">新增 {{ previewDiff.added.length }}</p>
            <ul class="diff-list">
              <li v-for="id in previewDiff.added" :key="id">{{ id }}</li>
            </ul>
          </div>
          <div>
            <p class="diff-heading">移除 {{ previewDiff.removed.length }}</p>
            <ul class="diff-list">
              <li v-for="id in previewDiff.removed" :key="id">{{ id }}</li>
            </ul>
          </div>
        </div>
        <p class="hint">
          应用前会再次读取原生规则核对。原生接口没有跨页面原子版本条件，请勿同时在其他页面编辑这个账号。
        </p>
      </div>
      <template #footer
        ><ZButton :disabled="busy" @click="previewOpen = false">取消</ZButton
        ><ZButton
          type="primary"
          :loading="busy"
          :disabled="applyFailed"
          @click="confirm"
          >确认应用</ZButton
        ></template
      >
    </ZDialog>
  </div>
</template>
