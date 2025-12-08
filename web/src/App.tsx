import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { BrowserRouter, Route, Routes } from 'react-router-dom';
import Dashboard from './pages/Dashboard';
import WorkflowDetail from './pages/WorkflowDetail';
import AgentDetail from './pages/AgentDetail';

const queryClient = new QueryClient();

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <div className="min-h-screen bg-gray-50 text-gray-900">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/workflows/:id" element={<WorkflowDetail />} />
            <Route path="/agents/:id" element={<AgentDetail />} />
          </Routes>
        </div>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

export default App;
