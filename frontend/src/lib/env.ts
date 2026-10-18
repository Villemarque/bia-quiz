export const isBrowser = typeof window !== 'undefined';

export function readLocal(key: string): string {
  if (!isBrowser) return '';
  try {
    return localStorage.getItem(key) ?? '';
  } catch {
    return '';
  }
}

export function writeLocal(key: string, value: string): void {
  if (!isBrowser) return;
  try {
    localStorage.setItem(key, value);
  } catch {
    /* private mode */
  }
}

export function readSession(key: string): string {
  if (!isBrowser) return '';
  try {
    return sessionStorage.getItem(key) ?? '';
  } catch {
    return '';
  }
}

export function writeSession(key: string, value: string): void {
  if (!isBrowser) return;
  try {
    sessionStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}
