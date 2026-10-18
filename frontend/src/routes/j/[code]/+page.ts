export function load({ params }: { params: Record<string, string> }) {
  return { code: (params.code ?? '').toUpperCase() };
}
