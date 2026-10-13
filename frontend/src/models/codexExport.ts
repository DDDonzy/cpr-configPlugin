import { format, validate, validateOrder, type Catalog } from './domain.ts'
export interface CodexModel {
  slug: string
  display_name: string
  supported_reasoning_levels: { effort: string; description: string }[]
  shell_type: 'unified_exec'
  visibility: 'list'
  supported_in_api: true
  priority: number
  base_instructions: string
  support_verbosity: true
  truncation_policy: { mode: 'tokens'; limit: 128000 }
  experimental_supported_tools: string[]
  context_window: number
  max_context_window: number
  input_modalities: string[]
  supports_reasoning_summary_parameter: true
}
export interface ExportVariant {
  name: string
  text: string | null
  error: string
}
export interface ExportBundle {
  base: ExportVariant
  codex: ExportVariant
  isDraft: boolean
}
const descriptions: Record<string, string> = {
  none: 'No reasoning effort',
  minimal: 'Minimal reasoning effort',
  low: 'Low reasoning effort',
  medium: 'Medium reasoning effort',
  high: 'High reasoning effort',
  xhigh: 'Extra-high reasoning effort',
  max: 'Maximum reasoning effort',
  ultra: 'Ultra reasoning effort',
}
export function toCodex(
  models: Catalog,
  order: readonly string[],
): { models: CodexModel[] } {
  validate(models)
  validateOrder(models, order)
  return {
    models: order.map((id, priority) => {
      const m = models[id]!
      if (
        m.input_modalities.some(
          (type) => !['text', 'image', 'audio'].includes(type),
        )
      )
        throw new Error(
          `${id}：当前 Codex 不支持 video 输入标记，无法生成兼容目录。Base 仍可正常导出。`,
        )
      return {
        slug: id,
        display_name: m.display_name,
        supported_reasoning_levels: [...m.reasoning_levels, 'ultra'].map((effort) => ({
          effort,
          description: descriptions[effort] ?? `${effort} reasoning effort`,
        })),
        shell_type: 'unified_exec',
        visibility: 'list',
        supported_in_api: true,
        priority,
        base_instructions:
          'You are a coding assistant. Help the user complete their task using the available tools.',
        support_verbosity: true,
        truncation_policy: { mode: 'tokens', limit: 128000 },
        experimental_supported_tools: [],
        context_window: m.context_window,
        max_context_window: m.context_window,
        input_modalities: [...m.input_modalities],
        supports_reasoning_summary_parameter: true,
      }
    }),
  }
}
function inline(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(inline).join(', ')}]`
  if (value !== null && typeof value === 'object')
    return `{${Object.entries(value)
      .map(([key, item]) => `${JSON.stringify(key)}: ${inline(item)}`)
      .join(', ')}}`
  return JSON.stringify(value)
}
export function formatCodex(catalog: { models: CodexModel[] }): string {
  const blocks = catalog.models.map((model) => {
    const fields = Object.entries(model).map(([key, value]) => {
      const text =
        key === 'supported_reasoning_levels' &&
        Array.isArray(value) &&
        value.length
          ? `[\n${value.map((level) => `        ${inline(level)}`).join(',\n')}\n      ]`
          : inline(value)
      return `      ${JSON.stringify(key)}: ${text}`
    })
    return `    {\n${fields.join(',\n')}\n    }`
  })
  return blocks.length
    ? `{\n  "models": [\n${blocks.join(',\n')}\n  ]\n}\n`
    : '{\n  "models": []\n}\n'
}
export function buildExport(
  models: Catalog,
  order: readonly string[],
  baseName = 'models.common.json',
  isDraft = false,
): ExportBundle {
  validate(models)
  validateOrder(models, order)
  const base: ExportVariant = {
    name: baseName,
    text: format(models, order),
    error: '',
  }
  try {
    return {
      base,
      codex: {
        name: 'models.json',
        text: formatCodex(toCodex(models, order)),
        error: '',
      },
      isDraft,
    }
  } catch (e) {
    return {
      base,
      codex: {
        name: 'models.json',
        text: null,
        error: e instanceof Error ? e.message : 'Codex 转换失败',
      },
      isDraft,
    }
  }
}
