import {
  applyResolvedTheme,
  resolveTheme,
  DEFAULT_THEME_COLOR,
  DEFAULT_CUSTOM_THEME_COLOR,
} from '@codex-proxy/ui/theme'
import type { PluginHost } from './api'
import { clone, changes, validate, type Snapshot } from './domain'
export function installDemo(): boolean {
  if (window.codexProxyPlugin) return false
  const root = document.documentElement
  applyResolvedTheme(
    root,
    resolveTheme('dark', DEFAULT_THEME_COLOR, DEFAULT_CUSTOM_THEME_COLOR),
  )
  document.body.style.background = '#0b1220'
  let snapshot: Snapshot = {
    version: 'a'.repeat(64),
    pluginVersion: '0.3.1',
    filePath:
      '/app/.runtime/data/model-capabilities/models.common.json（本地模拟）',
    modelOrder: ['demo-gpt', 'demo-claude'],
    models: {
      'demo-gpt': {
        display_name: 'Demo GPT',
        reasoning_levels: ['low', 'high', 'max'],
        context_window: 1050000,
        max_output_tokens: 128000,
        input_modalities: ['text', 'image'],
      },
      'demo-claude': {
        display_name: 'Demo Claude',
        reasoning_levels: ['low', 'medium', 'high'],
        context_window: 1000000,
        max_output_tokens: 256000,
        input_modalities: ['text', 'image'],
      },
    },
  }
  const reply = (status: number, value: unknown) => ({
    status,
    contentType: 'application/json',
    body: new TextEncoder().encode(JSON.stringify(value)).buffer as ArrayBuffer,
  })
  const host: PluginHost = {
    version: 2,
    theme: 'dark',
    async request(input) {
      try {
        if (input.path === 'api/snapshot') return reply(200, clone(snapshot))
        const body = JSON.parse(input.body ?? '{}')
        validate(body.models)
        if (body.expectedVersion !== snapshot.version)
          return reply(409, {
            error: { message: '文件版本已变化，请重新加载' },
          })
        const diff = changes(
          snapshot.models,
          body.models,
          snapshot.modelOrder,
          body.modelOrder,
        )
        if (input.path === 'api/preview')
          return reply(200, { version: snapshot.version, diff })
        if (input.path === 'api/save') {
          snapshot = {
            ...snapshot,
            version: crypto.randomUUID().replaceAll('-', '').repeat(2),
            models: clone(body.models),
            modelOrder: [...body.modelOrder],
          }
          return reply(200, { snapshot: clone(snapshot), diff })
        }
        return reply(404, { error: { message: '未知模拟接口' } })
      } catch (e) {
        return reply(400, { error: { message: (e as Error).message } })
      }
    },
  }
  Object.defineProperty(window, 'codexProxyPlugin', { value: host })
  return true
}
