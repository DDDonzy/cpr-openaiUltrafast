export const MAX_MODELS = 256
export function validModelId(model: string): boolean {
  return !!model && [...model].length <= 128 && !/[\s\p{Cc}]/u.test(model)
}
export function parseModelIds(input: string): string[] {
  return [...new Set(input.split(/[\s,，]+/u).filter(Boolean))]
}
