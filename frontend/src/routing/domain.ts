export type Mode = 'all' | 'allowlist' | 'denylist'
export interface ModelAccess {
  mode: Mode
  models: string[]
}
export interface ModelGroup {
  id: string
  name: string
  models: string[]
}
export interface Binding {
  groupIds: string[]
  manualModels: string[]
  appliedPolicy: ModelAccess
}
export interface GroupDocument {
  groups: ModelGroup[]
  bindings: Record<string, Binding>
}
export interface GroupState {
  version: number | null
  value: GroupDocument
}
export interface AccountGroupRef {
  id: string
  name: string
  color?: string
  enabled: boolean
}
/** 插件后端按允许清单投影的账号身份，不含令牌或密钥 */
export interface AccountIdentity {
  email?: string
  label?: string
  notes?: string
  authenticationKind?: string
  planType?: string
  planTypeDisplay?: string
  groups?: AccountGroupRef[]
  lastUsedAt?: string
}
export interface Account {
  id: string
  name: string
  provider: string
  enabled: boolean
  status: string
  modelAccess: ModelAccess
  identity?: AccountIdentity
}
export interface UpstreamAddress {
  url: string
  host: string
  loopback: boolean
}
export interface AccountUpstream {
  kind: 'oauth' | 'api_key' | 'unknown'
  status: 'ok' | 'unavailable' | 'timeout'
  address?: UpstreamAddress
  transport?: 'http' | 'prefer_websocket' | null
}
export interface UpstreamsResponse {
  upstreams: Record<string, AccountUpstream>
  warnings: string[]
}
export interface Snapshot {
  accounts: Account[]
  state: GroupState
  limits: { modelsPerAccount: number; groups: number }
  pluginVersion: string
}
export interface PolicyRequest {
  accountId: string
  before: ModelAccess
  after: ModelAccess
  groupIds: string[]
  manualModels: string[]
  expectedGroupVersion: number | null
}
export interface PolicyDiff {
  accountId: string
  accountName: string
  before: ModelAccess
  after: ModelAccess
  added: string[]
  removed: string[]
  modeChanged: boolean
}
export interface ApplyResult {
  account: Account
  state: GroupState
  diff: PolicyDiff
  warnings: string[]
  nativeApplied: boolean
}
export const modeLabels: Record<Mode, string> = {
  all: '不限制',
  allowlist: '白名单',
  denylist: '黑名单',
}
export function validModelId(id: string): boolean {
  return (
    id.length > 0 &&
    id.trim() === id &&
    new TextEncoder().encode(id).length <= 256 &&
    !id.startsWith('__') &&
    !id.includes('*') &&
    !/\p{Cc}/u.test(id)
  )
}
export function normalizeModels(
  input: readonly string[],
  allowEmpty = true,
  enforceLimit = false,
): string[] {
  const models = [...new Set(input)].sort()
  if (enforceLimit && models.length > 256)
    throw new Error('最终名单最多 256 个精确模型 ID')
  if (!allowEmpty && models.length === 0)
    throw new Error('白名单或黑名单至少需要一个模型')
  const invalid = models.findIndex((id) => !validModelId(id))
  if (invalid >= 0)
    throw new Error(
      `第 ${invalid + 1} 个模型 ID 无效：字符格式或长度无效，不支持通配符或保留前缀`,
    )
  return models
}
export function parseModels(text: string): string[] {
  const raw = text.replace(/^\uFEFF/, '').trim()
  if (!raw) return []
  let input: unknown
  if (raw.startsWith('[') || raw.startsWith('{')) {
    try {
      input = JSON.parse(raw)
    } catch {
      throw new Error('名单 JSON 格式无效')
    }
    if (!Array.isArray(input)) {
      if (
        !input ||
        typeof input !== 'object' ||
        !Array.isArray((input as { models?: unknown }).models)
      )
        throw new Error('JSON 需为字符串数组，或包含 models 数组')
      input = (input as { models: unknown[] }).models
    }
  } else
    input = raw
      .split(/[\r\n,;，；]+/)
      .map((x) => x.trim())
      .filter(Boolean)
  if (!Array.isArray(input) || input.some((id) => typeof id !== 'string'))
    throw new Error('模型名单只能包含字符串 ID')
  return normalizeModels(input as string[])
}
export function policy(
  mode: Mode,
  models: readonly string[],
  enforceLimit = false,
): ModelAccess {
  return {
    mode,
    models: mode === 'all' ? [] : normalizeModels(models, false, enforceLimit),
  }
}
export function samePolicy(a: ModelAccess, b: ModelAccess): boolean {
  return (
    a.mode === b.mode &&
    JSON.stringify([...new Set(a.models)].sort()) ===
      JSON.stringify([...new Set(b.models)].sort())
  )
}
export function expandGroups(
  manual: readonly string[],
  selected: readonly string[],
  groups: readonly ModelGroup[],
): string[] {
  const all = [...manual]
  for (const id of selected) {
    const group = groups.find((g) => g.id === id)
    if (!group) throw new Error('所选分组已删除，请重新加载')
    all.push(...group.models)
  }
  return normalizeModels(all)
}
export function difference(account: Account, after: ModelAccess): PolicyDiff {
  return {
    accountId: account.id,
    accountName: account.name,
    before: account.modelAccess,
    after,
    added: after.models.filter(
      (id) => !account.modelAccess.models.includes(id),
    ),
    removed: account.modelAccess.models.filter(
      (id) => !after.models.includes(id),
    ),
    modeChanged: account.modelAccess.mode !== after.mode,
  }
}
export function groupId(): string {
  return [...crypto.getRandomValues(new Uint8Array(12))]
    .map((x) => x.toString(16).padStart(2, '0'))
    .join('')
}
export function validateGroups(groups: readonly ModelGroup[]): void {
  if (groups.length > 32) throw new Error('最多保存 32 个模型分组')
  const ids = new Set<string>()
  for (const group of groups) {
    if (!/^[a-zA-Z0-9_-]{1,128}$/.test(group.id) || ids.has(group.id))
      throw new Error('分组 ID 无效或重复')
    ids.add(group.id)
    if (
      !group.name.trim() ||
      new TextEncoder().encode(group.name).length > 192 ||
      /\p{Cc}/u.test(group.name)
    )
      throw new Error('分组名称为空或过长')
    normalizeModels(group.models, true, true)
  }
}
