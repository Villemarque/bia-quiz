# The frontend follows SvelteKit's conventions on purpose:
# `src/routes/**/+page.svelte`, `+layout.svelte`, `+error.svelte`, `+page.ts`,
# `$lib`, `static/`. Only `src/router/` and `src/lib/shim/` are non-Kit.
#
# If we ever switch to SvelteKit: add @sveltejs/kit + adapter-static, rename
# index.html to src/app.html, delete src/router + src/lib/shim, and point
# rust-embed at `build/` instead of `dist/`. No component moves.
