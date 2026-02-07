import * as d3 from "d3";
import { useEffect, useRef } from "react";

import type { GraphEdge, GraphNode } from "../api/graphClient";

type Props = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  onSelectNode?: (node: GraphNode) => void;
};

export function CausalGraph({ nodes, edges, onSelectNode }: Props) {
  const svgRef = useRef<SVGSVGElement | null>(null);

  useEffect(() => {
    const svg = d3.select(svgRef.current);
    if (svg.empty()) {
      return;
    }

    const width = 720;
    const height = 520;

    svg.selectAll("*").remove();
    svg.attr("viewBox", `0 0 ${width} ${height}`);

    const nodeById = new Map(nodes.map((node) => [node.id, node]));
    const links = edges
      .map((edge) => ({
        source: nodeById.get(edge.from),
        target: nodeById.get(edge.to),
        type: edge.type,
      }))
      .filter((edge) => edge.source && edge.target) as Array<{
      source: GraphNode;
      target: GraphNode;
      type: string;
    }>;

    const simulation = d3
      .forceSimulation(nodes)
      .force(
        "link",
        d3
          .forceLink(links)
          .id((d: GraphNode) => d.id)
          .distance(120),
      )
      .force("charge", d3.forceManyBody().strength(-320))
      .force("center", d3.forceCenter(width / 2, height / 2));

    const link = svg
      .append("g")
      .attr("stroke", "#2d3344")
      .attr("stroke-width", 1.4)
      .selectAll("line")
      .data(links)
      .enter()
      .append("line");

    const node = svg
      .append("g")
      .selectAll("circle")
      .data(nodes)
      .enter()
      .append("circle")
      .attr("r", 18)
      .attr("fill", "#2b6dff")
      .attr("stroke", "#15234d")
      .attr("stroke-width", 1.5)
      .call(
        d3
          .drag<SVGCircleElement, GraphNode>()
          .on("start", (event, d) => {
            if (!event.active) {
              simulation.alphaTarget(0.3).restart();
            }
            d.x = event.x;
            d.y = event.y;
          })
          .on("drag", (event, d) => {
            d.x = event.x;
            d.y = event.y;
          })
          .on("end", (event) => {
            if (!event.active) {
              simulation.alphaTarget(0);
            }
          }),
      )
      .on("click", (_, d) => onSelectNode?.(d));

    const labels = svg
      .append("g")
      .selectAll("text")
      .data(nodes)
      .enter()
      .append("text")
      .text((d) => d.type)
      .attr("fill", "#e6e6e6")
      .attr("font-size", 10)
      .attr("text-anchor", "middle");

    simulation.on("tick", () => {
      link
        .attr("x1", (d) => d.source.x ?? 0)
        .attr("y1", (d) => d.source.y ?? 0)
        .attr("x2", (d) => d.target.x ?? 0)
        .attr("y2", (d) => d.target.y ?? 0);

      node.attr("cx", (d) => d.x ?? 0).attr("cy", (d) => d.y ?? 0);
      labels.attr("x", (d) => d.x ?? 0).attr("y", (d) => (d.y ?? 0) - 24);
    });

    return () => {
      simulation.stop();
    };
  }, [edges, nodes, onSelectNode]);

  return <svg ref={svgRef} />;
}
