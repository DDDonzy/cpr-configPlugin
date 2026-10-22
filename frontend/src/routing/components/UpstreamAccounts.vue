<script setup lang="ts">
import { computed, ref } from 'vue'
import { ZButton, ZCard, ZEmpty, ZInput, ZSegmented, ZTag } from '@codex-proxy/ui'
import { PencilLine } from '@lucide/vue'
import HelpTip from './HelpTip.vue'
import { modeLabels, type Account, type AccountUpstream, type Binding } from '../domain'
import {
  filterAccount,
  initial,
  isApiKey,
  isOAuth,
  kindLabel,
  linkedGroupCount,
  matchesAccount,
  nativeName,
  planLabel,
  relativeTime,
  safeColor,
  shortId,
  statusView,
  upstreamView,
  type AccountFilter,
} from '../accountPresentation'
const props = defineProps<{
  accounts: readonly Account[]
  labels: Map<string, string>
  upstreams: Record<string, AccountUpstream> | null
  upstreamsLoading: boolean
  upstreamWarnings: readonly string[]
  bindings: Record<string, Binding>
  selectedId: string
}>()
const emit = defineEmits<{ select: [string] }>()
const search = ref('')
const filter = ref<string>('all')
const filters = [
  { label: '全部', value: 'all' },
  { label: 'OAuth', value: 'oauth' },
  { label: 'API Key', value: 'api_key' },
  { label: '已停用', value: 'disabled' },
]
const counts = computed(() => ({
  oauth: props.accounts.filter(isOAuth).length,
  apiKey: props.accounts.filter(isApiKey).length,
  disabled: props.accounts.filter((a) => !a.enabled).length,
}))
const label = (a: Account) => props.labels.get(a.id) ?? a.name
const rows = computed(() =>
  props.accounts
    .filter(
      (a) =>
        filterAccount(a, filter.value as AccountFilter) &&
        matchesAccount(a, label(a), props.upstreams?.[a.id], search.value),
    )
    .map((a) => ({
      account: a,
      label: label(a),
      name: nativeName(a),
      kind: kindLabel(a),
      plan: planLabel(a),
      source: upstreamView(a, props.upstreams?.[a.id], props.upstreamsLoading),
      status: statusView(a),
      lastUsed: relativeTime(a.identity?.lastUsedAt),
      groups: a.identity?.groups ?? [],
      rule: `${modeLabels[a.modelAccess.mode]} · ${a.modelAccess.models.length} 个模型`,
      linked: linkedGroupCount(props.bindings, a.id),
    })),
)
</script>
<template>
  <ZCard class="upstream-page">
    <template #title
      ><span class="section-title"
        >上游账号<HelpTip
          label="上游账号"
          text="直接列出 CPR 原生账号：OAuth 账号显示登录邮箱与套餐，API Key 账号显示实际上游地址（只显示协议、主机和端口，不显示密钥或路径）。点击“编辑模型规则”进入该账号的白名单/黑名单设置。" /></span
    ></template>
    <div class="upstream-toolbar">
      <ZInput
        v-model="search"
        class="upstream-search"
        clearable
        aria-label="搜索上游账号"
        placeholder="搜索邮箱、名称、上游地址、CPR 分组或备注"
      />
      <ZSegmented v-model="filter" aria-label="账号类型筛选" :options="filters" />
    </div>
    <p class="upstream-summary">
      共 {{ accounts.length }} 个上游账号 · OAuth {{ counts.oauth }} · API Key
      {{ counts.apiKey }}<template v-if="counts.disabled">
        · 已停用 {{ counts.disabled }}</template
      >
    </p>
    <div
      v-for="warning in upstreamWarnings"
      :key="warning"
      class="notice warning"
      role="status"
    >
      {{ warning }}
    </div>
    <ul v-if="rows.length" class="upstream-list" aria-label="上游账号列表">
      <li v-for="row in rows" :key="row.account.id">
        <article
          class="upstream-row"
          :class="{
            selected: row.account.id === selectedId,
            inactive: !row.account.enabled,
          }"
        >
          <span class="upstream-avatar" aria-hidden="true">{{ initial(row.label) }}</span>
          <div class="upstream-identity">
            <strong class="upstream-title" :title="row.label">{{ row.label }}</strong>
            <span class="upstream-sub">
              <span v-if="row.name">CPR 名称：{{ row.name }}</span>
              <code :title="row.account.id">{{ shortId(row.account.id) }}</code>
            </span>
            <span v-if="row.account.identity?.notes" class="upstream-notes" :title="row.account.identity.notes">备注：{{ row.account.identity.notes }}</span>
          </div>
          <div class="upstream-source">
            <span class="upstream-tags"
              ><ZTag size="small" :type="isOAuth(row.account) ? 'primary' : 'info'">{{ row.kind }}</ZTag
              ><ZTag v-if="row.plan" size="small">{{ row.plan }}</ZTag></span
            >
            <span class="upstream-host" :class="`tone-${row.source.tone}`" :title="row.source.detail ?? row.source.text">{{ row.source.text }}</span>
            <span v-if="row.source.detail" class="upstream-detail">{{ row.source.detail }}</span>
          </div>
          <div class="upstream-groups" aria-label="CPR 账号分组">
            <span v-for="group in row.groups" :key="group.id" class="cpr-group" :class="{ off: !group.enabled }"
              ><i :style="{ background: safeColor(group.color) }" aria-hidden="true"></i>{{ group.name }}</span
            >
            <span v-if="!row.groups.length" class="muted">未加入 CPR 分组</span>
          </div>
          <div class="upstream-state">
            <ZTag size="small" :type="row.status.type">{{ row.status.text }}</ZTag>
            <span class="muted">{{ row.lastUsed ? `最近使用 ${row.lastUsed}` : '暂无使用记录' }}</span>
            <span class="muted">{{ row.rule }}<template v-if="row.linked"> · 关联 {{ row.linked }} 个分组</template></span>
          </div>
          <div class="upstream-actions">
            <ZButton
              size="small"
              :type="row.account.id === selectedId ? 'primary' : 'default'"
              :aria-label="`编辑 ${row.label} 的模型规则`"
              @click="emit('select', row.account.id)"
              ><PencilLine :size="14" />编辑模型规则</ZButton
            >
          </div>
        </article>
      </li>
    </ul>
    <ZEmpty
      v-else
      class="empty-body"
      :title="accounts.length ? '没有匹配的上游账号' : '暂无上游账号'"
      :description="accounts.length ? '换个关键词或筛选条件试试' : '请先在 CPR 原生账号页面添加账号'"
    />
  </ZCard>
</template>
