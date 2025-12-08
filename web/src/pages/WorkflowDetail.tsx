import { useQuery } from '@tanstack/react-query';
import { useParams, Link } from 'react-router-dom';
import { api } from '../api';
import { ArrowLeft, Play, Loader2, Terminal, CheckCircle, AlertCircle } from 'lucide-react';
import { useState, useRef, useEffect } from 'react';

export default function WorkflowDetail() {
    const { id } = useParams<{ id: string }>();
    const [input, setInput] = useState('');
    const [events, setEvents] = useState<any[]>([]);
    const [isRunning, setIsRunning] = useState(false);
    const scrollRef = useRef<HTMLDivElement>(null);

    const { data: workflow, isLoading, error } = useQuery({
        queryKey: ['workflow', id],
        queryFn: () => api.getWorkflow(id!),
        enabled: !!id,
    });

    // Auto-scroll to bottom of events
    useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
        }
    }, [events]);

    const handleRun = async () => {
        if (!input.trim()) return;
        setIsRunning(true);
        setEvents([]);

        try {
            await api.streamExecution(id!, input, (event) => {
                setEvents(prev => [...prev, event]);
            });
        } catch (err) {
            console.error(err);
            setEvents(prev => [...prev, { "Error": "Connection failed" }]);
        } finally {
            setIsRunning(false);
        }
    };

    if (isLoading) return <div className="p-8">Loading...</div>;
    if (error || (workflow && workflow.error)) {
        return <div className="p-8 text-red-500">Error: {workflow?.error || 'Failed to load'}</div>;
    }

    return (
        <div className="h-screen flex flex-col overflow-hidden p-4">
            {/* Header */}
            <div className="flex items-center justify-between mb-3 flex-shrink-0">
                <div className="flex items-center gap-4">
                    <Link to="/" className="flex items-center gap-2 text-gray-500 hover:text-gray-900">
                        <ArrowLeft className="w-4 h-4" /> Back
                    </Link>
                    <h1 className="text-2xl font-bold">{id}</h1>
                </div>
            </div>

            {/* Main Content */}
            <div className="flex-1 grid grid-cols-1 lg:grid-cols-[300px_1fr] gap-4 min-h-0">
                {/* Definition Column - Compact */}
                <div className="bg-white rounded-lg shadow-sm border p-3 flex flex-col min-h-0">
                    <h2 className="text-sm font-semibold mb-2 flex-shrink-0">Definition</h2>
                    <pre className="bg-gray-50 p-2 rounded-md overflow-auto text-xs flex-1">
                        {JSON.stringify(workflow, null, 2)}
                    </pre>
                </div>

                {/* Execution Column - Full Width */}
                <div className="bg-white rounded-lg shadow-sm border p-4 flex flex-col min-h-0">
                    <h2 className="text-xl font-semibold mb-4 text-blue-600 flex items-center gap-2">
                        <Play className="w-5 h-5" /> Run Workflow
                    </h2>

                    <div
                        ref={scrollRef}
                        className="flex-1 bg-gray-900 text-gray-100 rounded-md p-4 mb-3 overflow-auto border font-mono text-sm min-h-0"
                    >
                        {events.length === 0 && !isRunning && (
                            <div className="text-gray-500 text-center mt-20">Ready to run. Enter input below.</div>
                        )}

                        {events.map((ev, idx) => {
                            if (ev.Log) {
                                return (
                                    <div key={idx} className="mb-2 text-gray-400 text-xs font-mono">
                                        <span className="font-bold opacity-50">Log:</span> {ev.Log}
                                    </div>
                                );
                            }
                            if (ev.Thought) {
                                return (
                                    <div key={idx} className="mb-2 text-yellow-500 opacity-80">
                                        <span className="font-bold">Thinking:</span> {ev.Thought}
                                    </div>
                                );
                            }
                            if (ev.ToolCall) {
                                return (
                                    <div key={idx} className="mb-2 text-blue-400">
                                        <div className="flex items-center gap-2">
                                            <Terminal className="w-4 h-4" />
                                            <span className="font-bold">Tool Call:</span> {ev.ToolCall.name}
                                        </div>
                                    </div>
                                );
                            }
                            if (ev.ToolResult) {
                                return (
                                    <div key={idx} className="mb-2 text-green-400 pl-4 border-l-2 border-green-800">
                                        <div className="flex items-center gap-2">
                                            <CheckCircle className="w-3 h-3" />
                                            <span className="font-bold">Result:</span> {ev.ToolResult.name}
                                        </div>
                                        <div className="text-xs text-gray-500 truncate">{JSON.stringify(ev.ToolResult.result)}</div>
                                    </div>
                                );
                            }
                            if (ev.Answer) {
                                return (
                                    <div key={idx} className="mt-4 mb-2 p-3 bg-gray-800 rounded border border-gray-700 text-white">
                                        <span className="font-bold text-green-500 block mb-1">Final Answer:</span>
                                        <div className="whitespace-pre-wrap">{ev.Answer}</div>
                                    </div>
                                );
                            }
                            if (ev.Error) {
                                return (
                                    <div key={idx} className="mb-2 text-red-500 flex items-center gap-2">
                                        <AlertCircle className="w-4 h-4" />
                                        Error: {ev.Error}
                                    </div>
                                );
                            }
                            return <div key={idx} className="mb-1 text-gray-500">{JSON.stringify(ev)}</div>;
                        })}

                        {isRunning && (
                            <div className="flex items-center gap-2 text-gray-400 mt-2 animate-pulse">
                                <Loader2 className="w-4 h-4 animate-spin" />
                                Processing...
                            </div>
                        )}
                    </div>

                    <div className="flex gap-2 flex-shrink-0">
                        <textarea
                            className="flex-1 border rounded-md p-2 focus:ring-2 focus:ring-blue-500 focus:outline-none resize-none"
                            rows={2}
                            placeholder="Enter your instruction here..."
                            value={input}
                            onChange={(e) => setInput(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter' && !e.shiftKey) {
                                    e.preventDefault();
                                    handleRun();
                                }
                            }}
                        />
                        <button
                            onClick={handleRun}
                            disabled={isRunning || !input.trim()}
                            className="bg-blue-600 text-white px-6 py-2 rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed font-medium"
                        >
                            Run
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
