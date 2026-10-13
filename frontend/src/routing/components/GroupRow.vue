<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { ZIconButton, ZPopover, ZMenuItem, ZTag } from '@codex-proxy/ui'
import { ChevronRight, Pencil, Trash2, Ellipsis, Download } from '@lucide/vue'
import ModelPreview from './ModelPreview.vue'
import HelpTip from './HelpTip.vue'
import {
  modeLabels,
  type Account,
  type Binding,
  type ModelGroup,
} from '../domain'
import {
  groupInitial,
  groupModelSummary,
  groupAssociations,
} from '../groupPresentation'
const props = defineProps<{
  group: ModelGroup
  accounts: readonly Account[]
  bindings: Record<string, Binding>
}>()
const emit = defineEmits<{
  edit: [ModelGroup]
  delete: [ModelGroup]
  export: [ModelGroup]
}>()
const expanded = ref(false),
  moreOpen = ref(false)
const detailsId = `group-details-${useId()}`
const associations = computed(() =>
  groupAssociations(props.group.id, props.bindings, props.accounts),
)
function toggle() {
  expanded.value = !expanded.value
}
function exportGroup() {
  moreOpen.value = false
  emit('export', props.group)
}
function toggleFromMenu() {
  moreOpen.value = false
  toggle()
}
</script>
<template>
  <article class="group-record" :class="{ 'is-expanded': expanded }">
    <div class="group-record-header">
      <button
        type="button"
        class="group-row-toggle"
        :aria-label="`${expanded ? '收起' : '展开'}分组 ${group.name}`"
        :aria-expanded="expanded"
        :aria-controls="detailsId"
        @click="toggle"
      >
        <ChevronRight
          :size="16"
          class="group-chevron"
          :class="{ rotated: expanded }"
          aria-hidden="true"
        />
        <span class="group-avatar" aria-hidden="true">{{
          groupInitial(group.name)
        }}</span>
        <span class="group-row-name">{{ group.name }}</span>
        <span class="group-row-summary">
          <span class="group-row-facts"
            >{{ group.models.length }} 个模型<span class="summary-separator"
              >·</span
            >{{ associations.length }} 个账号关联</span
          >
          <span class="group-row-excerpt">{{
            groupModelSummary(group.models)
          }}</span>
        </span>
      </button>
      <div class="group-row-actions">
        <ZIconButton
          type="primary"
          :aria-label="`编辑分组 ${group.name}`"
          @click="emit('edit', group)"
          ><Pencil :size="17"
        /></ZIconButton>
        <ZIconButton
          type="danger"
          :aria-label="`删除分组 ${group.name}`"
          @click="emit('delete', group)"
          ><Trash2 :size="17"
        /></ZIconButton>
        <ZPopover v-model="moreOpen" trigger="click" placement="bottom-end">
          <template #reference="{ id }"
            ><ZIconButton
              :aria-label="`更多操作 ${group.name}`"
              aria-haspopup="menu"
              :aria-expanded="moreOpen"
              :aria-controls="moreOpen ? id : undefined"
              ><Ellipsis :size="18" /></ZIconButton
          ></template>
          <div
            class="group-more-menu"
            role="menu"
            :aria-label="`${group.name}的更多操作`"
          >
            <ZMenuItem role="menuitem" @click="exportGroup"
              ><template #icon><Download :size="16" /></template
              >导出分组</ZMenuItem
            >
            <ZMenuItem role="menuitem" @click="toggleFromMenu"
              ><template #icon><ChevronRight :size="16" /></template
              >{{ expanded ? '收起详情' : '展开详情' }}</ZMenuItem
            >
          </div>
        </ZPopover>
      </div>
    </div>
    <section
      v-if="expanded"
      :id="detailsId"
      class="group-record-details"
      :aria-label="`${group.name}详情`"
    >
      <div class="group-detail-panel">
        <div class="group-detail-heading">
          <h3>模型名单</h3>
          <ZTag size="small">{{ group.models.length }} 个模型</ZTag>
        </div>
        <ModelPreview v-if="group.models.length" :models="group.models" />
        <p v-else class="group-detail-empty">
          尚未添加模型，点击编辑完善这个分组。
        </p>
      </div>
      <div class="group-detail-panel">
        <div class="group-detail-heading">
          <h3>关联账号</h3>
          <HelpTip
            label="关联账号"
            text="这里展示保存过的分组引用记录，不代表规则会自动同步。修改分组后，仍需在账号模型规则中预览并确认应用。"
          />
        </div>
        <ul v-if="associations.length" class="group-associated-list">
          <li v-for="account in associations" :key="account.accountId">
            <span>{{ account.name }}</span
            ><span class="hint">{{
              account.missing
                ? '账号不存在'
                : `保存时：${modeLabels[account.savedMode]}`
            }}</span>
          </li>
        </ul>
        <p v-else class="group-detail-empty">尚未关联账号</p>
      </div>
      <div class="group-detail-id">
        分组标识 <code>{{ group.id }}</code>
      </div>
    </section>
  </article>
</template>
