export interface Route {
  dir: string;
  keys: string[];
  regex: RegExp;
  component: any;
}

export function buildRoutes(pages: Record<string, { default: any }>): Route[] {
  return Object.entries(pages).map(([file, module]) => {
    const dir = file.replace(/\/\+page\.svelte$/, '');
    const relative = dir.replace(/^\/src\/routes/, '') || '/';
    const keys: string[] = [];

    let pattern: string;
    if (relative === '/') {
      pattern = '/';
    } else {
      pattern = relative.replace(/\[([^\]]+)\]/g, (_match, key: string) => {
        keys.push(key);
        return '([^/]+)';
      });
    }

    const regex = new RegExp(`^${pattern === '/' ? '/' : `${pattern}/?`}$`);
    return { dir, keys, regex, component: module.default };
  });
}

export function matchRoute(routes: Route[], path: string) {
  const clean = path === '/' ? '/' : path.replace(/\/+$/, '') || '/';
  for (const route of routes) {
    const match = clean.match(route.regex);
    if (!match) continue;
    const params: Record<string, string> = {};
    route.keys.forEach((key, index) => {
      params[key] = decodeURIComponent(match[index + 1] ?? '');
    });
    return { route, params };
  }
  return null;
}
