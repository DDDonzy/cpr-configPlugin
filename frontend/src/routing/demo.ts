import type { PluginHost } from './api'
import {
  applyResolvedTheme,
  resolveTheme,
  DEFAULT_THEME_COLOR,
  DEFAULT_CUSTOM_THEME_COLOR,
} from '@codex-proxy/ui/theme'
import {
  difference,
  policy,
  samePolicy,
  validateGroups,
  type Account,
  type GroupState,
  type PolicyRequest,
  type Snapshot,
} from './domain'

const fixtures = (): Snapshot => ({
  accounts: [
    {
      id: 'acct_demo_gpt',
      name: 'GPT',
      provider: 'openai',
      enabled: true,
      status: 'normal',
      modelAccess: { mode: 'denylist', models: ['claude-opus-5.5'] },
    },
    {
      id: 'acct_demo_claude',
      name: 'CLAUDE',
      provider: 'openai',
      enabled: true,
      status: 'normal',
      modelAccess: { mode: 'all', models: [] },
    },
  ],
  state: {
    version: 1,
    value: {
      groups: [
        {
          id: 'group_claude',
          name: 'Claude 模型',
          models: [
            'claude-opus-5.5',
            'claude-opus-5',
            'claude-fable-5-1',
            'claude-fable-5',
          ],
        },
        {
          id: 'group_images',
          name: '图片模型',
          models: ['gpt-image-2', 'gpt-image-1.5'],
        },
      ],
      bindings: {},
    },
  },
  limits: { modelsPerAccount: 256, groups: 32 },
  pluginVersion: '0.1.0',
})
export function installDemo(): boolean {
  if (window.codexProxyPlugin)
    return (
      typeof (window as unknown as { routingGroupsDemoTheme?: unknown })
        .routingGroupsDemoTheme === 'function'
    )
  let snapshot = fixtures()
  const catalogue = [
    'claude-opus-5.5',
    'claude-opus-5',
    'claude-fable-5-1',
    'claude-fable-5',
    'gpt-6-astra',
    'gpt-6-luna',
    'gpt-6.1-sol',
    'gpt-image-2',
    'gpt-image-1.5',
    'vendor-custom-image-model',
    ...Array.from({ length: 300 }, (_, i) => `demo-model-${i.toString(36)}`),
  ]
  let theme: 'light' | 'dark' = 'dark'
  function applyTheme() {
    applyResolvedTheme(
      document.documentElement,
      resolveTheme(theme, DEFAULT_THEME_COLOR, DEFAULT_CUSTOM_THEME_COLOR),
    )
  }
  applyTheme()
  const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T
  const reply = (status: number, value: unknown) => ({
    status,
    contentType: 'application/json',
    body: new TextEncoder().encode(JSON.stringify(value)).buffer as ArrayBuffer,
  })
  const host: PluginHost = {
    version: 2,
    get theme() {
      return theme
    },
    async request(input) {
      await new Promise((resolve) => setTimeout(resolve, 70))
      try {
        const body = input.body ? JSON.parse(input.body) : null
        if (input.path === 'api/snapshot') return reply(200, clone(snapshot))
        if (input.path === 'api/catalog-all')
          return reply(200, { models: catalogue, warnings: [] })
        if (input.path === 'api/catalog')
          return reply(200, { accountId: body.accountId, models: catalogue })
        if (input.path === 'api/groups') {
          if (body.expectedVersion !== snapshot.state.version)
            return reply(409, {
              error: { message: '分组版本已变化，请重新加载' },
            })
          validateGroups(body.groups)
          snapshot.state = {
            version: (snapshot.state.version ?? 0) + 1,
            value: { ...snapshot.state.value, groups: body.groups },
          }
          return reply(200, clone(snapshot.state))
        }
        if (input.path === 'api/preview' || input.path === 'api/apply') {
          const req = body as PolicyRequest
          const account = snapshot.accounts.find((a) => a.id === req.accountId)
          if (!account) return reply(400, { error: { message: '账号不存在' } })
          if (!samePolicy(account.modelAccess, req.before))
            return reply(409, {
              error: { message: '原生模型权限已被其他页面修改，请重新加载' },
            })
          if (req.expectedGroupVersion !== snapshot.state.version)
            return reply(409, {
              error: { message: '分组版本已变化，请重新加载' },
            })
          const diff = difference(account, req.after)
          if (input.path === 'api/preview')
            return reply(200, { diff, groupVersion: snapshot.state.version })
          policy(req.after.mode, req.after.models, true)
          account.modelAccess = clone(req.after)
          snapshot.state.version = (snapshot.state.version ?? 0) + 1
          snapshot.state.value.bindings[req.accountId] = {
            groupIds: req.groupIds,
            manualModels: req.manualModels,
            appliedPolicy: clone(req.after),
          }
          return reply(200, {
            account: clone(account),
            state: clone(snapshot.state),
            diff,
            warnings: [],
            nativeApplied: true,
          })
        }
        return reply(404, { error: { message: '未找到演示接口' } })
      } catch (e) {
        return reply(400, {
          error: { message: e instanceof Error ? e.message : '演示数据无效' },
        })
      }
    },
  }
  Object.defineProperty(window, 'codexProxyPlugin', {
    value: host,
    configurable: false,
  })
  Object.defineProperty(window, 'routingGroupsDemoTheme', {
    value: () => {
      theme = theme === 'dark' ? 'light' : 'dark'
      applyTheme()
      window.dispatchEvent(
        new CustomEvent('codex-proxy-themechange', { detail: theme }),
      )
    },
  })
  return true
}
