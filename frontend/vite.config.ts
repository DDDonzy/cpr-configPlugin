import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath } from 'node:url'
export default defineConfig(({ mode }) => {
  const page = mode === 'routing' ? 'routing' : 'models'
  return {
    root: fileURLToPath(new URL(`./pages/${page}/`, import.meta.url)),
    plugins: [vue(), { name: 'cpr-classic-page', apply: 'build', transformIndexHtml: { order: 'post', handler: (html: string) => html.replace(/ type="module"/g, '').replace(/ crossorigin/g, '').replace(/<script src=/g, '<script defer src=') } }],
    base: './',
    build: { outDir: fileURLToPath(new URL(`./dist/${page}/`, import.meta.url)), emptyOutDir: true, cssCodeSplit: false, rollupOptions: { output: { format: 'iife', name: 'CprConfigPlugin', entryFileNames: 'app.js', assetFileNames: asset => asset.names?.some(n => n.endsWith('.css')) ? 'app.css' : '[name][extname]', codeSplitting: false } } }
  }
})
