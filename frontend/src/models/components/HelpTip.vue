<script setup lang="ts">
import { computed } from 'vue'
import { ZPopover } from '@codex-proxy/ui'
import { CircleHelp } from '@lucide/vue'
const props = defineProps<{ label: string; text: string }>()
const paragraphs = computed(() =>
  props.text
    .split(/(?<=。)|\n/u)
    .map((p) => p.trim())
    .filter(Boolean),
)
</script>
<template>
  <ZPopover
    trigger="hover-click"
    placement="top"
    role="tooltip"
    :hover-delay="180"
    :offset="8"
  >
    <template #reference="{ id, open }">
      <button
        type="button"
        class="help-icon"
        :aria-label="`${label}说明`"
        :aria-describedby="open ? id : undefined"
      >
        <CircleHelp :size="16" />
      </button>
    </template>
    <div class="help-content">
      <p v-for="(paragraph, index) in paragraphs" :key="index">
        {{ paragraph }}
      </p>
    </div>
  </ZPopover>
</template>
