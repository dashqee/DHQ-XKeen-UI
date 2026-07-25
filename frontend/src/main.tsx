import { createRoot } from 'react-dom/client'

import './globals.css'
import './lib/outboundParser.js'

const loadApp =
  import.meta.env.VITE_APP_MODE === 'external'
    ? () => import('./ExternalApp.tsx')
    : () => import('./App.tsx')

void loadApp().then(({ default: App }) => {
  createRoot(document.getElementById('root')!).render(<App />)
})
