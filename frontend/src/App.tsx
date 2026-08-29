import { Route, Routes } from 'react-router-dom'
import AppLayout from '@/components/layout/AppLayout'
import VoteListPage from '@/pages/VoteListPage'
import VoteNewPage from '@/pages/VoteNewPage'
import VoteDetailPage from '@/pages/VoteDetailPage'

/**
 * Dedicated host (`vote.cl8y.com`) uses `/`, `/new`, `/:id`.
 * `/vote*` aliases keep DEX-era and Legal-return URLs working (#8).
 * Static `/new` and `/vote` outrank `/:id`.
 */
export default function App() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route path="/" element={<VoteListPage />} />
        <Route path="/new" element={<VoteNewPage />} />
        <Route path="/vote" element={<VoteListPage />} />
        <Route path="/vote/new" element={<VoteNewPage />} />
        <Route path="/vote/:id" element={<VoteDetailPage />} />
        <Route path="/:id" element={<VoteDetailPage />} />
      </Route>
    </Routes>
  )
}
