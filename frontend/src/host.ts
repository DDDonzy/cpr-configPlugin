export interface HostReply { status: number; contentType: string; body: ArrayBuffer }
export interface PluginHost {
  readonly version: 2
  readonly theme: 'light' | 'dark'
  resourceUrl?: (path: string) => string
  request: (value: { method: 'GET' | 'POST'; path: string; contentType?: string; body?: string }) => Promise<HostReply>
}
declare global { interface Window { readonly codexProxyPlugin?: PluginHost } }
