import { useEffect, useMemo, useState } from "react";

import { CausalGraph } from "./components/CausalGraph";
import { ViolationDetail } from "./components/ViolationDetail";
import {
  fetchDriftPath,
  fetchGraph,
  fetchGraphs,
  fetchNodes,
  fetchViolationDetail,
  fetchViolations,
  GraphNode,
  GraphResponse,
  GraphSummary,
  ViolationDetail as ViolationDetailType,
  ViolationSummary,
} from "./api/graphClient";

const DEFAULT_TOKEN = "";

export function App() {
  const [token, setToken] = useState(
    localStorage.getItem("astragraph_token") ?? DEFAULT_TOKEN
  );
  const [graphs, setGraphs] = useState<GraphSummary[]>([]);
  const [selectedGraphId, setSelectedGraphId] = useState<string>("");
  const [graph, setGraph] = useState<GraphResponse | null>(null);
  const [selectedNode, setSelectedNode] = useState<GraphNode | undefined>(
    undefined
  );
  const [driftPath, setDriftPath] = useState<string[] | undefined>(undefined);
  const [violations, setViolations] = useState<ViolationSummary[]>([]);
  const [selectedViolation, setSelectedViolation] =
    useState<ViolationDetailType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [polling, setPolling] = useState(false);
  const [nodeType, setNodeType] = useState<string>("");
  const [agentId, setAgentId] = useState<string>("");
  const [status, setStatus] = useState<string>("");

  useEffect(() => {
    localStorage.setItem("astragraph_token", token);
  }, [token]);

  useEffect(() => {
    fetchGraphs()
      .then((items) => {
        setGraphs(items);
        if (!selectedGraphId && items.length > 0) {
          setSelectedGraphId(items[0].graph_id);
        }
      })
      .catch((err: Error) => setError(err.message));
    fetchViolations()
      .then(setViolations)
      .catch((err: Error) => setError(err.message));
  }, []);

  useEffect(() => {
    if (!selectedGraphId) {
      return;
    }
    let cancelled = false;

    const loadGraph = async () => {
      try {
        const baseGraph = await fetchGraph(selectedGraphId);
        let nodes = baseGraph.nodes;
        if (nodeType || agentId || status) {
          nodes = await fetchNodes(selectedGraphId, {
            type: nodeType || undefined,
            agent_id: agentId || undefined,
            status: status || undefined,
          });
        }
        const nodeIds = new Set(nodes.map((node) => node.id));
        const edges = baseGraph.edges.filter(
          (edge) => nodeIds.has(edge.from) && nodeIds.has(edge.to)
        );
        if (!cancelled) {
          setGraph({ ...baseGraph, nodes, edges });
        }
      } catch (err) {
        if (!cancelled) {
          setError((err as Error).message);
        }
      }
    };

    loadGraph();
    const timer = polling ? setInterval(loadGraph, 5000) : undefined;
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [selectedGraphId, nodeType, agentId, status, polling]);

  useEffect(() => {
    if (!selectedGraphId || !selectedNode) {
      setDriftPath(undefined);
      return;
    }
    fetchDriftPath(selectedGraphId, selectedNode.id)
      .then(setDriftPath)
      .catch(() => setDriftPath(undefined));
  }, [selectedGraphId, selectedNode]);

  const violationOptions = useMemo(() => violations.slice(0, 50), [violations]);

  const handleSelectViolation = async (violation: ViolationSummary) => {
    try {
      const detail = await fetchViolationDetail(violation.violation_id);
      setSelectedViolation(detail);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div className="app">
      <div className="panel">
        <h2>Causal Coordination Graph</h2>
        {error && <div className="meta">{error}</div>}
        <div className="controls">
          <label>
            Auth token
            <input
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder="Bearer token"
            />
          </label>
          <label>
            Graph
            <select
              value={selectedGraphId}
              onChange={(event) => setSelectedGraphId(event.target.value)}
            >
              <option value="">Select graph</option>
              {graphs.map((item) => (
                <option key={item.graph_id} value={item.graph_id}>
                  {item.graph_id} ({item.status})
                </option>
              ))}
            </select>
          </label>
          <label>
            Node type
            <input
              value={nodeType}
              onChange={(event) => setNodeType(event.target.value)}
              placeholder="ActionNode"
            />
          </label>
          <label>
            Agent
            <input
              value={agentId}
              onChange={(event) => setAgentId(event.target.value)}
              placeholder="agent-123"
            />
          </label>
          <label>
            Status
            <input
              value={status}
              onChange={(event) => setStatus(event.target.value)}
              placeholder="allowed"
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={polling}
              onChange={(event) => setPolling(event.target.checked)}
            />
            Poll every 5s
          </label>
        </div>
        {graph ? (
          <CausalGraph
            nodes={graph.nodes}
            edges={graph.edges}
            onSelectNode={setSelectedNode}
          />
        ) : (
          <div className="meta">Loading graph...</div>
        )}
      </div>
      <div className="panel">
        <h2>Node Details</h2>
        <ViolationDetail
          node={selectedNode}
          driftPath={driftPath}
          violation={selectedViolation}
        />
        <h3>Violations</h3>
        <div className="meta">
          {violationOptions.length === 0
            ? "No violations reported."
            : "Select a violation to inspect details."}
        </div>
        <ul>
          {violationOptions.map((violation) => (
            <li key={violation.violation_id}>
              <button onClick={() => handleSelectViolation(violation)}>
                {violation.violation_id} — {violation.rule_id}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
