interface Host {
  version: number
  request: (v: { method: 'GET' | 'POST'; path: string; contentType?: string; body?: string }) => Promise<{ status: number; contentType: string; body: ArrayBuffer }>
}
declare global { interface Window { codexProxyPlugin?: Host } }

export class ApiError extends Error {
  readonly status: number
  readonly code?: string
  constructor(message: string, status: number, code?: string) {
    super(message)
    this.status = status
    this.code = code
  }
}

export async function api<T>(path: string, data?: unknown): Promise<T> {
  const host = window.codexProxyPlugin
  if (!host || host.version !== 2) throw new Error('请从 CPR 的插件菜单打开此页面')
  const r = await host.request({ method: data === undefined ? 'GET' : 'POST', path, ...(data === undefined ? {} : { contentType: 'application/json', body: JSON.stringify(data) }) })
  if (r.contentType.split(';')[0]?.trim() !== 'application/json') throw new Error('插件返回格式无效，请刷新确认')
  const value = JSON.parse(new TextDecoder().decode(r.body))
  if (r.status < 200 || r.status >= 300) throw new ApiError(value.error?.message || value.message || `操作失败 HTTP ${r.status}`, r.status, value.error?.code)
  return value as T
}

export interface Rule { name: string; accountIds: string[]; clientKeyIds: string[]; models: string[] }
export interface KeyFact { id: string; name: string; enabled: boolean }
export interface AccountFact { account_id: string; name: string; email: string | null; provider_id: string; enabled: boolean }
export interface RuleSet { version: number | null; rules: Rule[] }
export interface Snapshot extends RuleSet {
  pluginVersion: string
  models: string[]
  modelWarnings?: string[]
  keys: KeyFact[]
  accounts: AccountFact[]
  instance: {
    id: string; enabled: boolean; running: boolean; revision: number
    artifactSha256: string; versionWarning: string | null; loadError: string | null
    runtime: { status: string; drainingRevisions: number[] } | null
  }
}
export interface SaveRules { rules: Rule[]; expectedVersion: number | null }
