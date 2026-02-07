export type GraphEdge = {
  from: string;
  to: string;
  type: string;
};

export type GraphNode = {
  id: string;
  type: string;
  agent_id?: string;
  action_type?: string;
  tool_name?: string;
  deviation_score?: number;
  status?: string;
};

export type GraphResponse = {
  graph_id: string;
  workflow_id: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
};

export type GraphSummary = {
  graph_id: string;
  workflow_id: string;
  status: string;
};

export type ViolationSummary = {
  violation_id: string;
  rule_id: string;
  agent_id: string;
};

export type ViolationDetail = {
  violation_id: string;
  details: string;
};

const DEFAULT_BASE_URL =
  import.meta.env.VITE_GRAPH_API ?? "http://localhost:8080";

function authHeaders(): HeadersInit {
  const token = localStorage.getItem("astragraph_token");
  if (!token) {
    return {};
  }
  return { Authorization: `Bearer ${token}` };
}

export async function fetchGraphs(): Promise<GraphSummary[]> {
  const response = await fetch(`${DEFAULT_BASE_URL}/graphs`, {
    headers: authHeaders(),
  });
  if (!response.ok) {
    throw new Error("Failed to load graphs");
  }
  return response.json() as Promise<GraphSummary[]>;
}

export async function fetchGraph(graphId: string): Promise<GraphResponse> {
  const response = await fetch(`${DEFAULT_BASE_URL}/graphs/${graphId}`, {
    headers: authHeaders(),
  });
  if (!response.ok) {
    throw new Error(`Failed to load graph ${graphId}`);
  }
  return response.json() as Promise<GraphResponse>;
}

export async function fetchNodes(
  graphId: string,
  filter?: { type?: string; agent_id?: string; status?: string }
): Promise<GraphNode[]> {
  const params = new URLSearchParams();
  if (filter?.type) params.set("type", filter.type);
  if (filter?.agent_id) params.set("agent_id", filter.agent_id);
  if (filter?.status) params.set("status", filter.status);
  const response = await fetch(
    `${DEFAULT_BASE_URL}/graphs/${graphId}/nodes?${params.toString()}`,
    { headers: authHeaders() }
  );
  if (!response.ok) {
    throw new Error(`Failed to load nodes for ${graphId}`);
  }
  return response.json() as Promise<GraphNode[]>;
}

export async function fetchDriftPath(
  graphId: string,
  nodeId: string
): Promise<string[]> {
  const response = await fetch(
    `${DEFAULT_BASE_URL}/graphs/${graphId}/drift-path/${nodeId}`,
    { headers: authHeaders() }
  );
  if (!response.ok) {
    throw new Error("Failed to load drift path");
  }
  return response.json() as Promise<string[]>;
}

export async function fetchViolations(): Promise<ViolationSummary[]> {
  const response = await fetch(`${DEFAULT_BASE_URL}/audit/violations`, {
    headers: authHeaders(),
  });
  if (!response.ok) {
    throw new Error("Failed to load violations");
  }
  return response.json() as Promise<ViolationSummary[]>;
}

export async function fetchViolationDetail(
  violationId: string
): Promise<ViolationDetail> {
  const response = await fetch(
    `${DEFAULT_BASE_URL}/audit/violations/${violationId}`,
    { headers: authHeaders() }
  );
  if (!response.ok) {
    throw new Error("Failed to load violation details");
  }
  return response.json() as Promise<ViolationDetail>;
}
