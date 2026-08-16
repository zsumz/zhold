export function normalizePathText(value: string): string {
  const normalized = value.replaceAll("\\\\?\\", "");
  return process.platform === "win32" ? normalized.toLowerCase() : normalized;
}
