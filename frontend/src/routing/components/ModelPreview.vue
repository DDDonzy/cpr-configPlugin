<script setup lang="ts">
import { ZScrollbar, ZTag } from '@codex-proxy/ui'
import { Circle, Ban } from '@lucide/vue'
import type { Mode } from '../domain'
defineProps<{
  models: readonly string[]
  groupModels?: ReadonlySet<string>
  mode?: Mode
}>()
</script>
<template>
  <ZScrollbar
    max-height="12rem"
    class="model-scroll model-preview-scroll"
    always-visible
  >
    <ul class="model-preview-grid" aria-label="模型预览">
      <li v-for="id in models" :key="id" class="catalog-row model-preview-item">
        <Circle
          v-if="mode === 'allowlist'"
          class="model-policy-icon"
          :size="17"
          role="img"
          aria-label="白名单"
        />
        <Ban
          v-else-if="mode === 'denylist'"
          class="model-policy-icon"
          :size="17"
          role="img"
          aria-label="黑名单"
        />
        <span class="model-preview-name" :title="id">{{ id }}</span
        ><ZTag v-if="groupModels?.has(id)" size="small">分组</ZTag>
      </li>
    </ul>
  </ZScrollbar>
</template>
