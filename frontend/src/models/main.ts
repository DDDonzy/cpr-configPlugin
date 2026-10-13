import { createApp } from 'vue'
import App from './App.vue'
import '@codex-proxy/ui/styles.css'
import './styles.css'
let demo = false
if (import.meta.env.DEV) {
  const { installDemo } = await import('./demo')
  demo = installDemo()
}
createApp(App, { demo }).mount('#app')
