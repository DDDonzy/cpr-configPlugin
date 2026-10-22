import type { Account, AccountUpstream, Binding } from './domain'

const providers: Record<string, string> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  google: 'Google',
  xai: 'xAI',
  deepseek: 'DeepSeek',
}
export function providerName(provider: string): string {
  return providers[provider.toLowerCase()] ?? provider
}
export const isOAuth = (a: Account) => a.identity?.authenticationKind === 'oauth'
export const isApiKey = (a: Account) =>
  a.identity?.authenticationKind === 'api_key'

/** 与 CPR 原生账号页一致：API Key 账号用名称，OAuth 账号用邮箱 */
export function identityLabel(a: Account): string {
  const name = a.name.trim()
  if (isApiKey(a) && name) return name
  return a.identity?.email?.trim() || a.identity?.label?.trim() || name || a.id
}
export function shortId(id: string): string {
  return id.length > 12 ? `…${id.slice(-6)}` : id
}
/** 标签重复时追加账号 ID 尾号，保证列表中每一项都能区分 */
export function accountLabels(accounts: readonly Account[]): Map<string, string> {
  const counts = new Map<string, number>()
  for (const a of accounts) {
    const label = identityLabel(a)
    counts.set(label, (counts.get(label) ?? 0) + 1)
  }
  return new Map(
    accounts.map((a) => {
      const label = identityLabel(a)
      const duplicated = (counts.get(label) ?? 0) > 1
      return [a.id, duplicated ? `${label}（${shortId(a.id)}）` : label]
    }),
  )
}
/** CPR 中填写的账号名称；与身份标签相同时不重复显示 */
export function nativeName(a: Account): string | undefined {
  const name = a.name.trim()
  return name && name !== identityLabel(a) ? name : undefined
}
export function kindLabel(a: Account): string {
  if (isOAuth(a)) return `${providerName(a.provider)} OAuth`
  if (isApiKey(a)) return 'API Key'
  return providerName(a.provider)
}
export function planLabel(a: Account): string | undefined {
  const id = a.identity
  if (!isOAuth(a) || !id?.planType) return undefined
  return id.planTypeDisplay || id.planType
}
export type UpstreamTone = 'official' | 'local' | 'relay' | 'pending' | 'unknown'
export interface UpstreamView {
  text: string
  detail?: string
  tone: UpstreamTone
}
export function upstreamView(
  a: Account,
  upstream: AccountUpstream | undefined,
  loading: boolean,
): UpstreamView {
  if (isOAuth(a))
    return {
      text: `${providerName(a.provider)} 官方`,
      detail: 'OAuth 授权登录',
      tone: 'official',
    }
  if (!isApiKey(a))
    return { text: providerName(a.provider), detail: '认证方式未知', tone: 'unknown' }
  if (upstream?.status === 'ok' && upstream.address) {
    const { host, url, loopback } = upstream.address
    return {
      text: host,
      detail: loopback ? `本机服务 · ${url}` : url,
      tone: loopback ? 'local' : 'relay',
    }
  }
  if (!upstream && loading)
    return { text: '正在读取上游地址…', tone: 'pending' }
  if (upstream?.status === 'timeout')
    return { text: '上游地址读取超时', detail: '重新加载后重试', tone: 'unknown' }
  return {
    text: '上游地址不可显示',
    detail: '原生详情不可读或地址不规范',
    tone: 'unknown',
  }
}
const statusTexts: Record<string, string> = {
  normal: '正常',
  quota_exhausted: '额度用尽',
  rate_limited: '限流中',
  disabled: '已停用',
  error: '异常',
}
export type TagTone = 'default' | 'success' | 'warning' | 'danger'
export function statusView(a: Account): { text: string; type: TagTone } {
  if (!a.enabled) return { text: '已停用', type: 'default' }
  const text = statusTexts[a.status] ?? a.status
  if (a.status === 'normal') return { text, type: 'success' }
  if (a.status === 'error') return { text, type: 'danger' }
  return { text, type: a.status === 'disabled' ? 'default' : 'warning' }
}
const pad = (n: number) => String(n).padStart(2, '0')
export function relativeTime(
  iso: string | undefined,
  now = Date.now(),
): string | undefined {
  if (!iso) return undefined
  const t = Date.parse(iso)
  if (!Number.isFinite(t)) return undefined
  const minutes = Math.floor(Math.max(0, now - t) / 60000)
  if (minutes < 1) return '刚刚'
  if (minutes < 60) return `${minutes} 分钟前`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} 小时前`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days} 天前`
  const d = new Date(t)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}
export type AccountFilter = 'all' | 'oauth' | 'api_key' | 'disabled'
export function filterAccount(a: Account, filter: AccountFilter): boolean {
  if (filter === 'disabled') return !a.enabled
  if (filter === 'oauth') return isOAuth(a)
  if (filter === 'api_key') return isApiKey(a)
  return true
}
export function matchesAccount(
  a: Account,
  label: string,
  upstream: AccountUpstream | undefined,
  query: string,
): boolean {
  const q = query.trim().toLowerCase()
  if (!q) return true
  const id = a.identity
  return [
    label,
    a.name,
    a.id,
    a.provider,
    kindLabel(a),
    id?.email,
    id?.label,
    id?.notes,
    id?.planType,
    id?.planTypeDisplay,
    upstream?.address?.host,
    ...(id?.groups ?? []).map((g) => g.name),
  ].some((value) => !!value && value.toLowerCase().includes(q))
}
/** 只允许 #RRGGBB / #RRGGBBAA，避免把任意字符串写入内联样式 */
export function safeColor(color: string | undefined): string | undefined {
  return color && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(color) ? color : undefined
}
export function initial(label: string): string {
  return Array.from(label.trim().normalize('NFC'))[0]?.toUpperCase() || '?'
}
export function safeFileName(label: string): string {
  const name = label.replace(/[\\/:*?"<>|\p{Cc}]+/gu, '_').trim().slice(0, 80)
  return name || 'account'
}
/** 侧栏副标题：OAuth 显示套餐和 CPR 名称，API Key 显示上游主机 */
export function compactSource(a: Account, view: UpstreamView): string {
  const name = nativeName(a)
  if (isOAuth(a))
    return [planLabel(a) ?? kindLabel(a), name].filter(Boolean).join(' · ')
  if (isApiKey(a)) return `API Key · ${view.text}`
  return [kindLabel(a), name].filter(Boolean).join(' · ')
}
export function linkedGroupCount(
  bindings: Record<string, Binding>,
  accountId: string,
): number {
  return bindings[accountId]?.groupIds.length ?? 0
}
