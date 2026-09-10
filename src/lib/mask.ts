const SECRET_KEY = /(token|secret|key|password|passwd|auth|credential|bearer)/i;

/** Keys whose values are shown masked by default in structured views. */
export function isSecretKey(key: string): boolean {
  return SECRET_KEY.test(key) && !/^(allowed[-_]?tools|hotkey|keymap|keybind)/i.test(key);
}

export function maskValue(value: string): string {
  if (value.length === 0) return "";
  if (value.startsWith("${") && value.endsWith("}")) return value;
  return "•".repeat(Math.min(12, Math.max(6, value.length)));
}
