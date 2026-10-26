import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    port: 5173,
    proxy: { '/api': 'http://127.0.0.1:8795' },
  },
  build: {
    outDir: 'dist', emptyOutDir: true, target: 'es2022',
    rollupOptions: {
      input: { main: 'index.html', home: 'home.html', construction: '../plugins/construction/web/index.ts', electrical: '../plugins/electrical/web/index.ts', 'walking-mode': '../plugins/walking-mode/web/index.ts', framing: '../plugins/framing/web/index.ts' },
      output: {
        entryFileNames: (c) => (['construction', 'electrical', 'walking-mode', 'framing'].includes(c.name) ? `plugins/${c.name}.js` : 'assets/[name]-[hash].js'),
        // plugins are self-contained: never share chunks with the app (they use window.plannr at runtime)
        manualChunks: undefined,
      },
    },
  },
});
