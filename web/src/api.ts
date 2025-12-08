import axios from 'axios';

export interface Workflow {
    id: string;
    name: string;
    file: string;
}

export interface Agent {
    id: string;
    name: string;
    file: string;
}

export const api = {
    getWorkflows: async () => {
        const res = await axios.get<Workflow[]>('/api/workflows');
        return res.data;
    },
    getAgents: async () => {
        const res = await axios.get<Agent[]>('/api/agents');
        return res.data;
    },
    getWorkflow: async (id: string) => {
        const res = await axios.get<any>(`/api/workflows/${id}`);
        return res.data;
    },
    getAgent: async (id: string) => {
        const res = await axios.get<any>(`/api/agents/${id}`);
        return res.data;
    },
    runExecution: async (workflowId: string, input: string) => {
        const res = await axios.post<any>('/api/executions', { workflow_id: workflowId, input });
        return res.data;
    },
    streamExecution: async (workflowId: string, input: string, onEvent: (event: any) => void) => {
        const response = await fetch('http://localhost:3000/api/executions/stream', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ workflow_id: workflowId, input })
        });

        if (!response.body) throw new Error("No response body");
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';

        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            const chunk = decoder.decode(value, { stream: true });
            buffer += chunk;

            const lines = buffer.split('\n\n');
            buffer = lines.pop() || '';

            for (const line of lines) {
                const trimmed = line.trim();
                if (trimmed.startsWith('data:')) {
                    const jsonStr = trimmed.substring(5).trim();
                    try {
                        onEvent(JSON.parse(jsonStr));
                    } catch (e) {
                        console.error('Failed to parse SSE', e);
                    }
                }
            }
        }
    }
};
