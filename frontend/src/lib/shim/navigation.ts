export async function goto(path: string): Promise<void> {
  if (path === location.pathname) return;
  history.pushState({}, '', path);
  window.dispatchEvent(new CustomEvent('app:navigate'));
}

export function afterNavigate(callback: () => void): () => void {
  const handler = () => callback();
  window.addEventListener('app:navigate', handler);
  window.addEventListener('popstate', handler);
  return () => {
    window.removeEventListener('app:navigate', handler);
    window.removeEventListener('popstate', handler);
  };
}
