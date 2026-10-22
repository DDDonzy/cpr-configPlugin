import { ref } from 'vue'
import type {
  ApplyResult,
  GroupState,
  ModelGroup,
  PolicyDiff,
  PolicyRequest,
  Snapshot,
  UpstreamsResponse,
} from './domain'
export type { PluginHost } from '../host'
import type { PluginHost } from '../host'
export async function request<T>(path: string, data?: unknown): Promise<T> {
  const host = window.codexProxyPlugin
  if (!host || host.version !== 2)
    throw new Error('请从 CPR 的“扩展页 → 模型路由分组”打开此页面')
  const reply = await host.request({
    method: data === undefined ? 'GET' : 'POST',
    path,
    ...(data === undefined
      ? {}
      : { contentType: 'application/json', body: JSON.stringify(data) }),
  })
  if (reply.contentType.split(';')[0]?.trim() !== 'application/json')
    throw new Error('插件响应不是 JSON')
  let value: unknown
  try {
    value = JSON.parse(new TextDecoder().decode(reply.body))
  } catch {
    throw new Error('插件响应格式无效')
  }
  if (reply.status < 200 || reply.status >= 300) {
    const error =
      value && typeof value === 'object'
        ? (value as { error?: { message?: string } }).error
        : undefined
    throw new Error(error?.message ?? `插件操作失败（HTTP ${reply.status}）`)
  }
  return value as T
}
export const api = {
  snapshot: () => request<Snapshot>('api/snapshot'),
  upstreams: () => request<UpstreamsResponse>('api/upstreams'),
  catalog: (accountId: string, refresh = false) =>
    request<{ models: string[]; accountId: string; warnings?: string[] }>(
      'api/catalog',
      {
        accountId,
        refresh,
      },
    ),
  globalCatalog: () =>
    request<{ models: string[]; warnings: string[] }>('api/catalog-all', {
      refresh: true,
    }),
  saveGroups: (expectedVersion: number | null, groups: ModelGroup[]) =>
    request<GroupState>('api/groups', { expectedVersion, groups }),
  preview: (value: PolicyRequest) =>
    request<{ diff: PolicyDiff; groupVersion: number | null }>(
      'api/preview',
      value,
    ),
  apply: (value: PolicyRequest) => request<ApplyResult>('api/apply', value),
}
export const exportDocument = ref<{ name: string; text: string } | null>(null)
export function downloadJson(name: string, value: unknown): void {
  const text = JSON.stringify(value, null, 2)
  if (text === undefined) throw new Error('没有可导出的 JSON 数据')
  exportDocument.value = { name, text }
}
