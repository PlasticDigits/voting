import { Navigate, Route, Routes } from 'react-router-dom'
import AppLayout from '@/components/layout/AppLayout'
import VoteListPage from '@/pages/VoteListPage'
import VoteNewPage from '@/pages/VoteNewPage'
import VoteDetailPage from '@/pages/VoteDetailPage'

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/vote" replace />} />
      <Route element={<AppLayout />}>
        <Route path="/vote" element={<VoteListPage />} />
        <Route path="/vote/new" element={<VoteNewPage />} />
        <Route path="/vote/:id" element={<VoteDetailPage />} />
      </Route>
    </Routes>
  )
}
