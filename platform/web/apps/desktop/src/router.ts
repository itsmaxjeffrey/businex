/** Tiny hash router: real links, real back button, deep-linkable URLs. */

export function currentPath(): string {
  const raw = window.location.hash.replace(/^#/, "");
  return raw.length > 0 ? raw : "/";
}

export function navigate(to: string): void {
  window.location.hash = "#" + to;
}
