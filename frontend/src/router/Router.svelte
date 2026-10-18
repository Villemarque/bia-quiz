<script lang="ts">
  import { onMount } from 'svelte';
  import Layout from '../routes/+layout.svelte';
  import ErrorPage from '../routes/+error.svelte';
  import { buildRoutes, matchRoute } from './match';

  const pages = import.meta.glob('/src/routes/**/+page.svelte', {
    eager: true,
  }) as Record<string, { default: any }>;

  const loads = import.meta.glob('/src/routes/**/+page.ts', {
    eager: true,
  }) as Record<string, { load?: (context: any) => any }>;

  const routes = buildRoutes(pages);

  let Page = $state<any>(null);
  let data = $state<any>(null);
  let error = $state<{ status: number; error: string } | null>(null);

  async function navigate(): Promise<void> {
    error = null;
    data = null;
    const found = matchRoute(routes, location.pathname);
    if (!found) {
      Page = null;
      error = { status: 404, error: 'Page introuvable.' };
      return;
    }
    Page = found.route.component;
    const loader = loads[`${found.route.dir}/+page.ts`];
    if (loader?.load) {
      try {
        data = await loader.load({ params: found.params, url: new URL(location.href) });
      } catch (cause) {
        error = { status: 500, error: String(cause) };
      }
    }
  }

  onMount(() => {
    const onNavigate = () => void navigate();
    window.addEventListener('app:navigate', onNavigate);
    window.addEventListener('popstate', onNavigate);
    void navigate();
    return () => {
      window.removeEventListener('app:navigate', onNavigate);
      window.removeEventListener('popstate', onNavigate);
    };
  });
</script>

{#if error}
  <ErrorPage status={error.status} error={error.error} />
{:else if Page}
  <Layout>
    <Page {data} />
  </Layout>
{/if}
