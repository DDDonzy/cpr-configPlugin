import { ref } from 'vue'
import type { ExportBundle } from './codexExport'
import type { Catalog, Diff, Snapshot } from './domain'
export type { PluginHost } from '../host'
import type { PluginHost } from '../host'
export async function request<T>(path: string, data?: unknown): Promise<T> {
  const host = window.codexProxyPlugin
  if (!host || host.version !== 2)
    throw new Error('请从 CPR 的插件扩展页打开模型基础配置')
  const reply = await host.request({
    method: data === undefined ? 'GET' : 'POST',
    path: `models/${path}`,
    ...(data === undefined
      ? {}
      : { contentType: 'application/json', body: JSON.stringify(data) }),
  })
  if (reply.contentType.split(';')[0]?.trim() !== 'application/json')
    throw new Error('插件响应不是 JSON')
  const value = JSON.parse(new TextDecoder().decode(reply.body)) as {
    error?: { message?: string }
  }
  if (reply.status < 200 || reply.status >= 300)
    throw new Error(value.error?.message ?? `操作失败（HTTP ${reply.status}）`)
  return value as T
}
export const api = {
  initialize: () => request<Snapshot>('api/initialize', { confirm: true }),
  snapshot: () => request<Snapshot>('api/snapshot'),
  preview: (expectedVersion: string, models: Catalog, modelOrder: string[]) =>
    request<{ diff: Diff; version: string }>('api/preview', {
      expectedVersion,
      models,
      modelOrder,
    }),
  save: (expectedVersion: string, models: Catalog, modelOrder: string[]) =>
    request<{ snapshot: Snapshot; diff: Diff }>('api/save', {
      expectedVersion,
      models,
      modelOrder,
    }),
}
export const exportDocument = ref<ExportBundle | null>(null)
