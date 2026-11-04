import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    port: 5173,
    proxy: { '/api': 'http://127.0.0.1:8795' },
  },
  build: {
    outDir: 'dist', emptyOutDir: true, target: 'es2022',
    rollupOptions: {
      // plugins are installable packages (github.com/MatthiasDM/plannr-plugins), served by the server from data/plugins
      input: { main: 'index.html', home: 'home.html' },
      output: { entryFileNames: 'assets/[name]-[hash].js' },
    },
  },
});
