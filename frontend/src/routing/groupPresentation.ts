import type { Account, Binding, Mode } from './domain'

export function groupInitial(name: string): string {
  return Array.from(name.trim().normalize('NFC'))[0]?.toUpperCase() || '?'
}
export function groupModelSummary(models: readonly string[]): string {
  if (!models.length) return '尚未添加模型'
  return models.slice(0, 2).join(' · ') + (models.length > 2 ? ' …' : '')
}
export interface GroupAssociation {
  accountId: string
  name: string
  missing: boolean
  savedMode: Mode
}
export function groupAssociations(
  groupId: string,
  bindings: Record<string, Binding>,
  accounts: readonly Account[],
): GroupAssociation[] {
  const accountsById = new Map(accounts.map((account) => [account.id, account]))
  return Object.entries(bindings)
    .filter(([, binding]) => binding.groupIds.includes(groupId))
    .map(([accountId, binding]) => {
      const account = accountsById.get(accountId)
      return {
        accountId,
        name: account?.name ?? '已移除的账号',
        missing: !account,
        savedMode: binding.appliedPolicy.mode,
      }
    })
}
