import { useQuery } from '@tanstack/react-query';
import { api } from '../api';
import { Layout } from 'lucide-react';
import { Link } from 'react-router-dom';

export default function Dashboard() {
    const { data: workflows, isLoading: loadingWorkflows } = useQuery({
        queryKey: ['workflows'],
        queryFn: api.getWorkflows,
    });

    const { data: agents, isLoading: loadingAgents } = useQuery({
        queryKey: ['agents'],
        queryFn: api.getAgents,
    });

    if (loadingWorkflows || loadingAgents) {
        return <div className="p-8">Loading...</div>;
    }

    return (
        <div className="p-8 max-w-7xl mx-auto">
            <h1 className="text-3xl font-bold mb-8">Kinetic Dashboard</h1>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                <div>
                    <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
                        <Layout className="w-5 h-5" /> Workflows
                    </h2>
                    <div className="grid gap-4">
                        {workflows?.map((wf) => (
                            <Link to={`/workflows/${wf.id}`} key={wf.id} className="block border p-4 rounded-lg hover:bg-gray-50 transition cursor-pointer bg-white shadow-sm">
                                <div className="font-medium">{wf.name}</div>
                                <div className="text-sm text-gray-500">{wf.file}</div>
                            </Link>
                        ))}
                        {workflows?.length === 0 && <div className="text-gray-500">No workflows found.</div>}
                    </div>
                </div>

                <div>
                    <h2 className="text-xl font-semibold mb-4">Agents</h2>
                    <div className="grid gap-4">
                        {agents?.map((agent) => (
                            <Link to={`/agents/${agent.id}`} key={agent.id} className="block border p-4 rounded-lg bg-white shadow-sm hover:bg-gray-50 transition cursor-pointer">
                                <div className="font-medium">{agent.name}</div>
                                <div className="text-sm text-gray-500">{agent.file}</div>
                            </Link>
                        ))}
                        {agents?.length === 0 && <div className="text-gray-500">No agents found.</div>}
                    </div>
                </div>
            </div>
        </div>
    );
}
