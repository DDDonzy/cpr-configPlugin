export interface HostReply { status: number; contentType: string; body: ArrayBuffer }
export interface PluginHost {
  readonly version: 2
  readonly theme: 'light' | 'dark'
  resourceUrl?: (path: string) => string
  request: (value: { method: 'GET' | 'POST'; path: string; contentType?: string; body?: string }) => Promise<HostReply>
}
declare global { interface Window { readonly codexProxyPlugin?: PluginHost } }

export function syncThemeTokens(): void {
  const style = document.documentElement.style
  const entries: [string, string][] = []
  for (let i = 0; i < style.length; i++) {
    const prop = style[i]
    if (prop?.startsWith('--z-')) {
      const cpProp = `--cp-${prop.slice(4)}`
      if (!style.getPropertyValue(cpProp))
        entries.push([cpProp, style.getPropertyValue(prop)])
    } else if (prop?.startsWith('--cp-')) {
      const zProp = `--z-${prop.slice(5)}`
      if (!style.getPropertyValue(zProp))
        entries.push([zProp, style.getPropertyValue(prop)])
    }
  }
  for (const [k, v] of entries) style.setProperty(k, v)
}
export function installThemeSync(): void {
  syncThemeTokens()
  window.addEventListener('codex-proxy-themechange', syncThemeTokens)
}
