export interface Model {
  display_name: string
  reasoning_levels: string[]
  context_window: number
  max_output_tokens?: number
  input_modalities: string[]
}
export type Catalog = Record<string, Model>
export interface Snapshot {
  version: string
  models: Catalog
  modelOrder: string[]
  filePath: string
  pluginVersion: string
  initialized?: boolean
}
export interface Diff {
  added: string[]
  removed: string[]
  changed: string[]
  orderChanged: boolean
}
export const efforts = [
  'none',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
]
export const modalities = ['text', 'image', 'audio', 'video']
export const modalityLabels: Record<string, string> = {
  text: '文本',
  image: '图片',
  audio: '音频',
  video: '视频',
}
export function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}
export function validate(models: unknown): asserts models is Catalog {
  if (!models || typeof models !== 'object' || Array.isArray(models))
    throw new Error('JSON 顶层必须是以模型 ID 为键的对象')
  const entries = Object.entries(models)
  if (entries.length > 2000) throw new Error('模型数量过多')
  for (const [id, value] of entries) {
    if (
      !id ||
      id.trim() !== id ||
      /[\s\u0000-\u001f\u007f*]/u.test(id) ||
      new TextEncoder().encode(id).length > 256 ||
      id.startsWith('__') ||
      ['constructor', 'prototype'].includes(id)
    )
      throw new Error(`模型 ID 无效：${id}`)
    if (!value || typeof value !== 'object' || Array.isArray(value))
      throw new Error(`${id}：配置必须是对象`)
    const m = value as Record<string, unknown>
    if (
      Object.keys(m).some(
        (k) =>
          ![
            'display_name',
            'reasoning_levels',
            'context_window',
            'max_output_tokens',
            'input_modalities',
          ].includes(k),
      )
    )
      throw new Error(
        `${id}：只能包含五个基础字段，不接受默认思考或描述等客户端策略`,
      )
    if (
      typeof m.display_name !== 'string' ||
      !m.display_name.trim() ||
      m.display_name.trim() !== m.display_name ||
      new TextEncoder().encode(m.display_name).length > 512 ||
      /[\u0000-\u001f\u007f]/u.test(m.display_name)
    )
      throw new Error(`${id}：显示名称无效`)
    if (
      !Array.isArray(m.reasoning_levels) ||
      m.reasoning_levels.length > 16 ||
      m.reasoning_levels.some(
        (x) => typeof x !== 'string' || !efforts.includes(x),
      ) ||
      new Set(m.reasoning_levels).size !== m.reasoning_levels.length
    )
      throw new Error(`${id}：思考等级无效或重复，ultra 不属于基础数据`)
    if (
      !Number.isSafeInteger(m.context_window) ||
      Number(m.context_window) <= 0
    )
      throw new Error(`${id}：上下文必须是正整数`)
    if (
      'max_output_tokens' in m &&
      (!Number.isSafeInteger(m.max_output_tokens) ||
        Number(m.max_output_tokens) <= 0)
    )
      throw new Error(`${id}：输出上限必须是正整数，未知时请不填写`)
    if (
      !Array.isArray(m.input_modalities) ||
      !m.input_modalities.length ||
      m.input_modalities.length > 4 ||
      m.input_modalities.some(
        (x) => typeof x !== 'string' || !modalities.includes(x),
      ) ||
      new Set(m.input_modalities).size !== m.input_modalities.length
    )
      throw new Error(`${id}：输入类型无效或重复`)
  }
  if (new TextEncoder().encode(format(models as Catalog)).length > 1024 * 1024)
    throw new Error('配置文件过大')
}
// Recursive JSON parser keeps duplicate keys visible instead of silently overwriting them.
export function parseDocument(text: string): {
  models: Catalog
  modelOrder: string[]
} {
  const modelOrder: string[] = []
  if (new TextEncoder().encode(text).length > 1024 * 1024)
    throw new Error('导入文件最多 1 MiB')
  let i = 0
  const fail = (): never => {
    throw new Error(`JSON 格式无效，位置 ${i}`)
  }
  const ws = () => {
    while (/[\x20\x09\x0a\x0d]/u.test(text[i] ?? '!')) i++
  }
  const string = (): string => {
    const start = i
    if (text[i++] !== '"') fail()
    let escaped = false
    while (i < text.length) {
      const c = text[i++]!
      if (c === '"' && !escaped)
        return JSON.parse(text.slice(start, i)) as string
      if (c === '\\' && !escaped) escaped = true
      else escaped = false
    }
    return fail()
  }
  const value = (depth: number): unknown => {
    if (depth > 16) throw new Error('JSON 层级过深')
    ws()
    const c = text[i]
    if (c === '"') return string()
    if (c === '{' || c === '[') {
      i++
      ws()
      const object =
        c === '{' ? (Object.create(null) as Record<string, unknown>) : null
      const array: unknown[] = []
      const close = c === '{' ? '}' : ']'
      if (text[i] === close) {
        i++
        return object ?? array
      }
      while (i < text.length) {
        ws()
        if (object) {
          const key = string()
          ws()
          if (text[i++] !== ':') fail()
          if (Object.hasOwn(object, key))
            throw new Error(`JSON 字段重复：${key}`)
          if (depth === 0) modelOrder.push(key)
          object[key] = value(depth + 1)
        } else array.push(value(depth + 1))
        ws()
        if (text[i] === close) {
          i++
          return object ?? array
        }
        if (text[i++] !== ',') fail()
      }
      return fail()
    }
    const literal =
      /^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/u.exec(
        text.slice(i),
      )
    if (!literal) return fail()
    i += literal[0].length
    return JSON.parse(literal[0]) as unknown
  }
  const result = value(0)
  ws()
  if (i !== text.length) fail()
  validate(result)
  return { models: result, modelOrder }
}
export function parse(text: string): Catalog {
  return parseDocument(text).models
}
export function validateOrder(models: Catalog, order: readonly string[]): void {
  if (
    !Array.isArray(order) ||
    order.length !== Object.keys(models).length ||
    new Set(order).size !== order.length ||
    order.some((id) => typeof id !== 'string' || !Object.hasOwn(models, id))
  )
    throw new Error('模型顺序与模型集合不一致')
}
export function moveId(
  order: readonly string[],
  id: string,
  index: number,
): string[] {
  if (!order.includes(id) || !Number.isInteger(index))
    throw new Error('排序目标无效')
  const next = order.filter((x) => x !== id)
  next.splice(Math.max(0, Math.min(index, next.length)), 0, id)
  return next
}
export function format(
  models: Catalog,
  order: readonly string[] = Object.keys(models),
): string {
  validateOrder(models, order)
  const blocks = order.map((id) => {
    const m = models[id]!

    const fields: Record<string, unknown> = {
      display_name: m.display_name,
      reasoning_levels: m.reasoning_levels,
      context_window: m.context_window,
      ...(m.max_output_tokens === undefined
        ? {}
        : { max_output_tokens: m.max_output_tokens }),
      input_modalities: m.input_modalities,
    }
    return `  ${JSON.stringify(id)}: {\n${Object.entries(fields)
      .map(
        ([k, v]) =>
          `    ${JSON.stringify(k)}: ${JSON.stringify(v).replace(/","/gu, '", "')}`,
      )
      .join(',\n')}\n  }`
  })
  return blocks.length ? `{\n${blocks.join(',\n')}\n}\n` : '{}\n'
}
export function changes(
  before: Catalog,
  after: Catalog,
  beforeOrder: readonly string[] = Object.keys(before),
  afterOrder: readonly string[] = Object.keys(after),
): Diff {
  return {
    orderChanged:
      JSON.stringify(beforeOrder.filter((id) => Object.hasOwn(after, id))) !==
      JSON.stringify(afterOrder.filter((id) => Object.hasOwn(before, id))),
    added: Object.keys(after).filter((id) => !Object.hasOwn(before, id)),
    removed: Object.keys(before).filter((id) => !Object.hasOwn(after, id)),
    changed: Object.keys(after).filter(
      (id) =>
        Object.hasOwn(before, id) &&
        format({ [id]: before[id]! }) !== format({ [id]: after[id]! }),
    ),
  }
}
export function integer(text: string, label: string): number {
  if (!/^\d+$/u.test(text.trim())) throw new Error(`${label}必须是正整数`)
  const value = Number(text)
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new Error(`${label}必须是正整数`)
  return value
}
