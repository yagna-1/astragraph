import type { GraphNode, ViolationDetail } from "../api/graphClient";

type Props = {
  node?: GraphNode;
  driftPath?: string[];
  violation?: ViolationDetail | null;
};

export function ViolationDetail({ node, driftPath, violation }: Props) {
  if (!node) {
    return <div className="meta">Select a node to inspect details.</div>;
  }

  return (
    <div>
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
      {driftPath && driftPath.length > 0 && (
        <>
          <div className="meta">Drift Path</div>
          <div>{driftPath.join(" → ")}</div>
        </>
      )}
      {violation && (
        <>
          <div className="meta">Violation Detail</div>
          <div>{violation.details}</div>
        </>
      )}
    </div>
  );
}
