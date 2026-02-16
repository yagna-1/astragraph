import { useEffect, useMemo, useState } from "react";

import { CausalGraph } from "./components/CausalGraph";
import { ViolationDetail } from "./components/ViolationDetail";
import {
  fetchDriftPath,
  fetchGraph,
  fetchGraphs,
  fetchNodes,
  fetchSloSummary,
  fetchViolationDetail,
  fetchViolations,
  GraphNode,
  GraphResponse,
  GraphSummary,
  SloSummary,
  ViolationDetail as ViolationDetailType,
  ViolationSummary,
} from "./api/graphClient";

const DEFAULT_TOKEN = "";
const TIMELINE_LIMIT = 100;

function formatTimestamp(unixSeconds: number): string {
  if (!unixSeconds) {
    return "unknown";
  }
  return new Date(unixSeconds * 1000).toLocaleString();
}

function formatPercent(value: number): string {
  if (!Number.isFinite(value)) {
    return "0.00%";
  }
  return `${(value * 100).toFixed(2)}%`;
}

function buildHitCounts(items: string[]): Array<{ key: string; count: number }> {
  const counts = new Map<string, number>();
  for (const item of items) {
    const key = item.trim() || "unknown";
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([key, count]) => ({ key, count }))
    .sort((left, right) => right.count - left.count);
}

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
  const [sloSummary, setSloSummary] = useState<SloSummary | null>(null);
  const [selectedViolation, setSelectedViolation] =
    useState<ViolationDetailType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [polling, setPolling] = useState(false);
  const [nodeType, setNodeType] = useState<string>("");
  const [agentId, setAgentId] = useState<string>("");
  const [status, setStatus] = useState<string>("");
  const [violationRuleFilter, setViolationRuleFilter] = useState<string>("");
  const [violationAgentFilter, setViolationAgentFilter] = useState<string>("");
  const [violationWorkflowFilter, setViolationWorkflowFilter] =
    useState<string>("");
  const [timelineWindowHours, setTimelineWindowHours] = useState<string>("24");
  const [pendingSelectedNodeId, setPendingSelectedNodeId] = useState<
    string | undefined
  >(undefined);

  useEffect(() => {
    localStorage.setItem("astragraph_token", token);
  }, [token]);

  useEffect(() => {
    setError(null);
    fetchGraphs()
      .then((items) => {
        setGraphs(items);
        if (!selectedGraphId && items.length > 0) {
          setSelectedGraphId(items[0].graph_id);
        }
      })
      .catch((err: Error) => setError(err.message));
  }, [token]);

  useEffect(() => {
    let cancelled = false;

    const loadViolations = async () => {
      try {
        const hours = Number.parseInt(timelineWindowHours, 10);
        const fromTs =
          Number.isFinite(hours) && hours > 0
            ? Math.floor(Date.now() / 1000) - hours * 60 * 60
            : undefined;
        const records = await fetchViolations({
          rule_id: violationRuleFilter || undefined,
          agent_id: violationAgentFilter || undefined,
          workflow_id: violationWorkflowFilter || undefined,
          from_ts: fromTs,
        });
        const slo = await fetchSloSummary();
        records.sort((left, right) => right.timestamp - left.timestamp);
        if (!cancelled) {
          setViolations(records);
          setSloSummary(slo);
        }
      } catch (err) {
        if (!cancelled) {
          setError((err as Error).message);
        }
      }
    };

    loadViolations();
    const timer = polling ? setInterval(loadViolations, 5000) : undefined;
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [
    token,
    polling,
    timelineWindowHours,
    violationRuleFilter,
    violationAgentFilter,
    violationWorkflowFilter,
  ]);

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

  useEffect(() => {
    if (!graph || !pendingSelectedNodeId) {
      return;
    }
    const foundNode = graph.nodes.find((node) => node.id === pendingSelectedNodeId);
    if (foundNode) {
      setSelectedNode(foundNode);
      setPendingSelectedNodeId(undefined);
    }
  }, [graph, pendingSelectedNodeId]);

  const violationOptions = useMemo(
    () => violations.slice(0, TIMELINE_LIMIT),
    [violations]
  );
  const ruleHits = useMemo(
    () => buildHitCounts(violations.map((violation) => violation.rule_id)),
    [violations]
  );
  const agentHits = useMemo(
    () => buildHitCounts(violations.map((violation) => violation.agent_id)),
    [violations]
  );
  const workflowHits = useMemo(
    () => buildHitCounts(violations.map((violation) => violation.workflow_id)),
    [violations]
  );
  const topRule = ruleHits[0]?.key ?? "none";
  const topAgent = agentHits[0]?.key ?? "none";
  const latestViolation = violationOptions[0];
  const p50 = sloSummary?.latency_ms.p50_ms ?? 0;
  const p95 = sloSummary?.latency_ms.p95_ms ?? 0;
  const p99 = sloSummary?.latency_ms.p99_ms ?? 0;
  const blockRate = sloSummary?.actions.block_rate ?? 0;
  const fpQueueCount = sloSummary?.false_positive_review_queue.count ?? 0;

  const handleSelectViolation = async (violation: ViolationSummary) => {
    try {
      const detail = await fetchViolationDetail(violation.violation_id);
      setSelectedViolation(detail);
      setSelectedGraphId(detail.workflow_id);
      setNodeType("");
      setAgentId("");
      setStatus("");
      setPendingSelectedNodeId(detail.node_id);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const clearViolationFilters = () => {
    setViolationRuleFilter("");
    setViolationAgentFilter("");
    setViolationWorkflowFilter("");
    setTimelineWindowHours("24");
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
        <h2>Incident Triage</h2>
        <div className="stats-grid">
          <div className="stat-card">
            <div className="meta">Total violations</div>
            <strong>{violations.length}</strong>
          </div>
          <div className="stat-card">
            <div className="meta">Top rule</div>
            <strong>{topRule}</strong>
          </div>
          <div className="stat-card">
            <div className="meta">Top agent</div>
            <strong>{topAgent}</strong>
          </div>
        </div>
        <div className="stats-grid slo-grid">
          <div className="stat-card">
            <div className="meta">p50 latency</div>
            <strong>{p50.toFixed(1)} ms</strong>
          </div>
          <div className="stat-card">
            <div className="meta">p95 latency</div>
            <strong>{p95.toFixed(1)} ms</strong>
          </div>
          <div className="stat-card">
            <div className="meta">p99 latency</div>
            <strong>{p99.toFixed(1)} ms</strong>
          </div>
          <div className="stat-card">
            <div className="meta">Block rate</div>
            <strong>{formatPercent(blockRate)}</strong>
          </div>
          <div className="stat-card">
            <div className="meta">FP review queue</div>
            <strong>{fpQueueCount}</strong>
          </div>
          <div className="stat-card">
            <div className="meta">Latency samples</div>
            <strong>{sloSummary?.latency_ms.samples ?? 0}</strong>
          </div>
        </div>
        <div className="controls">
          <label>
            Rule filter
            <input
              value={violationRuleFilter}
              onChange={(event) => setViolationRuleFilter(event.target.value)}
              placeholder="rule-export-block"
            />
          </label>
          <label>
            Agent filter
            <input
              value={violationAgentFilter}
              onChange={(event) => setViolationAgentFilter(event.target.value)}
              placeholder="lead-scorer"
            />
          </label>
          <label>
            Workflow filter
            <input
              value={violationWorkflowFilter}
              onChange={(event) => setViolationWorkflowFilter(event.target.value)}
              placeholder="wf-three-agent-e2e"
            />
          </label>
          <label>
            Timeline window (hours)
            <input
              value={timelineWindowHours}
              onChange={(event) => setTimelineWindowHours(event.target.value)}
              placeholder="24"
            />
          </label>
        </div>
        <div className="controls">
          <button type="button" onClick={clearViolationFilters}>
            Clear triage filters
          </button>
          <button
            type="button"
            onClick={() => latestViolation && handleSelectViolation(latestViolation)}
            disabled={!latestViolation}
          >
            Open latest incident
          </button>
        </div>
        <h3>Node + Violation Details</h3>
        <ViolationDetail
          node={selectedNode}
          driftPath={driftPath}
          violation={selectedViolation}
        />
        <h3>Policy Hit Analytics</h3>
        <div className="analytics-grid">
          <div>
            <div className="meta">Top rules</div>
            <ul>
              {ruleHits.slice(0, 6).map((hit) => (
                <li key={`rule-${hit.key}`}>
                  <button type="button" onClick={() => setViolationRuleFilter(hit.key)}>
                    {hit.key} — {hit.count}
                  </button>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <div className="meta">Top agents</div>
            <ul>
              {agentHits.slice(0, 6).map((hit) => (
                <li key={`agent-${hit.key}`}>
                  <button type="button" onClick={() => setViolationAgentFilter(hit.key)}>
                    {hit.key} — {hit.count}
                  </button>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <div className="meta">Top workflows</div>
            <ul>
              {workflowHits.slice(0, 6).map((hit) => (
                <li key={`workflow-${hit.key}`}>
                  <button type="button" onClick={() => setViolationWorkflowFilter(hit.key)}>
                    {hit.key} — {hit.count}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        </div>
        <h3>Incident Timeline</h3>
        <div className="meta">
          {violationOptions.length === 0
            ? "No violations reported."
            : "Select an incident to inspect node details and drift path."}
        </div>
        <ul>
          {violationOptions.map((violation) => (
            <li key={violation.violation_id}>
              <button
                type="button"
                className={
                  selectedViolation?.violation_id === violation.violation_id
                    ? "is-selected"
                    : ""
                }
                onClick={() => handleSelectViolation(violation)}
              >
                {formatTimestamp(violation.timestamp)} — {violation.rule_id} —{" "}
                {violation.agent_id} — score {violation.deviation_score.toFixed(2)}/
                {violation.threshold.toFixed(2)}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
