import type { GraphNode, ViolationDetail } from "../api/graphClient";

function formatTimestamp(unixSeconds: number): string {
  if (!unixSeconds) {
    return "unknown";
  }
  return new Date(unixSeconds * 1000).toLocaleString();
}

type Props = {
  node?: GraphNode;
  driftPath?: string[];
  violation?: ViolationDetail | null;
};

export function ViolationDetail({ node, driftPath, violation }: Props) {
  if (!node && !violation) {
    return <div className="meta">Select an incident or node to inspect details.</div>;
  }

  return (
    <div>
      {violation && (
        <>
          <div className="meta">Violation ID</div>
          <div>{violation.violation_id}</div>
          <div className="meta">Rule ID</div>
          <div>{violation.rule_id || "unknown"}</div>
          <div className="meta">Workflow</div>
          <div>{violation.workflow_id}</div>
          <div className="meta">Node</div>
          <div>{violation.node_id}</div>
          <div className="meta">Agent</div>
          <div>{violation.agent_id}</div>
          <div className="meta">Detected At</div>
          <div>{formatTimestamp(violation.timestamp)}</div>
          <div className="meta">Score / Threshold</div>
          <div>
            {violation.deviation_score.toFixed(2)} /{" "}
            {violation.threshold.toFixed(2)}
          </div>
        </>
      )}
      {node && (
        <>
          <div className="meta">Node ID</div>
          <div>{node.id}</div>
          <div className="meta">Type</div>
          <div>{node.type}</div>
          {node.tool_name && (
            <>
              <div className="meta">Tool</div>
              <div>{node.tool_name}</div>
            </>
          )}
          {typeof node.deviation_score === "number" && (
            <>
              <div className="meta">Deviation Score</div>
              <div>{node.deviation_score.toFixed(2)}</div>
            </>
          )}
          {node.status && (
            <>
              <div className="meta">Status</div>
              <div>{node.status}</div>
            </>
          )}
        </>
      )}
      {driftPath && driftPath.length > 0 && (
        <>
          <div className="meta">Drift Path</div>
          <div>{driftPath.join(" → ")}</div>
        </>
      )}
    </div>
  );
}
