import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import { LIVE_REFETCH_MS, startLivePolling } from './api/client'
import App from './App'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      // Data refreshed within the current heartbeat is fresh enough that
      // navigating between pages must not refetch it (and must not reset
      // the countdown ring).
      staleTime: LIVE_REFETCH_MS,
    },
  },
})

// One shared heartbeat refreshes every live query in lockstep.
startLivePolling(queryClient)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
)
