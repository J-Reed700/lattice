import { useEffect, useRef, useState, useCallback } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { ZoomIn, ZoomOut, Maximize2, RefreshCw } from 'lucide-react';

import Button from '../ui/Button/Button';

interface Mention {
  id: string;
  name: string;
  type: 'person' | 'concept' | 'wikilink';
  metadata?: string;
  createdAt: string;
}

interface GraphReactNode {
  id: string;
  label: string;
  type: 'person' | 'concept' | 'wikilink';
  x: number;
  y: number;
  vx: number;
  vy: number;
  documentCount?: number;
}

interface GraphEdge {
  source: string;
  target: string;
  weight: number;
}

interface MentionGraphProps {
  focusDocumentId?: string;
  onNodeClick?: (nodeId: string) => void;
  onReactNodeClick?: (nodeId: string) => void;
  className?: string;
}

export function MentionGraph({ focusDocumentId: _focusDocumentId, onNodeClick: _onReactNodeClick, className = '' }: MentionGraphProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [nodes, setReactNodes] = useState<GraphReactNode[]>([]);
  const [edges, setEdges] = useState<GraphEdge[]>([]);
  const [loading, setLoading] = useState(true);
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const animationFrameRef = useRef<number>(0);

  useEffect(() => {
    loadGraphData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadGraphData = useCallback(async () => {
    setLoading(true);
    try {
      const mentionsResult = await invoke<{ mentions: Mention[] }>('get_mentions_by_type', {
        mentionType: 'wikilink',
      });

      const graphReactNodes: GraphReactNode[] = await Promise.all(
        mentionsResult.mentions.map(async (mention) => {
          const backlinksResult = await invoke<{ documentIds: string[] }>(
            'get_backlinks_for_mention',
            { mentionName: mention.name }
          );

          return {
            id: mention.id,
            label: mention.name,
            type: mention.type,
            x: Math.random() * 800,
            y: Math.random() * 600,
            vx: 0,
            vy: 0,
            documentCount: backlinksResult.documentIds.length,
          };
        })
      );

      const graphEdges: GraphEdge[] = [];
      for (let i = 0; i < graphReactNodes.length; i++) {
        for (let j = i + 1; j < graphReactNodes.length; j++) {
          if (Math.random() > 0.7) {
            graphEdges.push({
              source: graphReactNodes[i].id,
              target: graphReactNodes[j].id,
              weight: 1,
            });
          }
        }
      }

      setReactNodes(graphReactNodes);
      setEdges(graphEdges);
    } catch (err) {
      console.error('Failed to load graph data:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadGraphData();
  }, [loadGraphData]);

  const applyForces = useCallback(() => {
    const newReactNodes = [...nodes];

    const centerX = (containerRef.current?.clientWidth || 800) / 2;
    const centerY = (containerRef.current?.clientHeight || 600) / 2;

    for (let i = 0; i < newReactNodes.length; i++) {
      let fx = 0;
      let fy = 0;

      for (let j = 0; j < newReactNodes.length; j++) {
        if (i === j) continue;

        const dx = newReactNodes[i].x - newReactNodes[j].x;
        const dy = newReactNodes[i].y - newReactNodes[j].y;
        const distance = Math.sqrt(dx * dx + dy * dy) || 1;

        const repulsion = 5000 / (distance * distance);
        fx += (dx / distance) * repulsion;
        fy += (dy / distance) * repulsion;
      }

      edges.forEach((edge) => {
        let targetReactNode: GraphReactNode | undefined;
        if (edge.source === newReactNodes[i].id) {
          targetReactNode = newReactNodes.find((n) => n.id === edge.target);
        } else if (edge.target === newReactNodes[i].id) {
          targetReactNode = newReactNodes.find((n) => n.id === edge.source);
        }

        if (targetReactNode) {
          const dx = targetReactNode.x - newReactNodes[i].x;
          const dy = targetReactNode.y - newReactNodes[i].y;
          const distance = Math.sqrt(dx * dx + dy * dy) || 1;

          const attraction = distance * 0.01;
          fx += (dx / distance) * attraction;
          fy += (dy / distance) * attraction;
        }
      });

      const toCenterX = centerX - newReactNodes[i].x;
      const toCenterY = centerY - newReactNodes[i].y;
      fx += toCenterX * 0.001;
      fy += toCenterY * 0.001;

      newReactNodes[i].vx = (newReactNodes[i].vx + fx) * 0.85;
      newReactNodes[i].vy = (newReactNodes[i].vy + fy) * 0.85;

      newReactNodes[i].x += newReactNodes[i].vx;
      newReactNodes[i].y += newReactNodes[i].vy;
    }

    setReactNodes(newReactNodes);
  }, [nodes, edges]);

  useEffect(() => {
    if (nodes.length === 0) return;

    const animate = () => {
      applyForces();
      drawGraph();
      animationFrameRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges, scale, offset]);

  const drawGraph = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.save();
    ctx.translate(offset.x, offset.y);
    ctx.scale(scale, scale);

    edges.forEach((edge) => {
      const sourceReactNode = nodes.find((n) => n.id === edge.source);
      const targetReactNode = nodes.find((n) => n.id === edge.target);

      if (sourceReactNode && targetReactNode) {
        ctx.beginPath();
        ctx.moveTo(sourceReactNode.x, sourceReactNode.y);
        ctx.lineTo(targetReactNode.x, targetReactNode.y);
        ctx.strokeStyle = '#cbd5e1';
        ctx.lineWidth = 1;
        ctx.stroke();
      }
    });

    nodes.forEach((node) => {
      const radius = 8 + (node.documentCount || 0) * 2;

      ctx.beginPath();
      ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI);

      switch (node.type) {
        case 'person':
          ctx.fillStyle = '#3b82f6';
          break;
        case 'concept':
          ctx.fillStyle = '#eab308';
          break;
        case 'wikilink':
          ctx.fillStyle = '#22c55e';
          break;
        default:
          ctx.fillStyle = '#6b7280';
      }

      ctx.fill();
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 2;
      ctx.stroke();

      ctx.fillStyle = '#1f2937';
      ctx.font = '12px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(node.label, node.x, node.y + radius + 15);
    });

    ctx.restore();
  }, [nodes, edges, scale, offset]);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setScale((prev) => Math.max(0.1, Math.min(5, prev * delta)));
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    setIsDragging(true);
    setDragStart({ x: e.clientX - offset.x, y: e.clientY - offset.y });
  }, [offset]);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (isDragging) {
        setOffset({
          x: e.clientX - dragStart.x,
          y: e.clientY - dragStart.y,
        });
      }
    },
    [isDragging, dragStart]
  );

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  const handleZoomIn = () => setScale((prev) => Math.min(5, prev * 1.2));
  const handleZoomOut = () => setScale((prev) => Math.max(0.1, prev / 1.2));
  const handleReset = () => {
    setScale(1);
    setOffset({ x: 0, y: 0 });
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    canvas.width = container.clientWidth;
    canvas.height = container.clientHeight;
  }, []);

  if (loading) {
    return (
      <div className={`flex items-center justify-center h-full bg-[hsl(var(--surface))] ${className}`}>
        <div className="text-center">
          <div className="inline-block animate-spin rounded-full h-12 w-12 border-4 border-[hsl(var(--accent))] border-t-transparent mb-4" />
          <p className="text-[hsl(var(--text-secondary))]">Loading graph...</p>
        </div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className={`relative h-full bg-[hsl(var(--surface))] ${className}`}>
      <canvas
        ref={canvasRef}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        className="w-full h-full cursor-grab active:cursor-grabbing"
      />

      <div className="absolute top-4 right-4 flex gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={handleZoomIn}
          leftIcon={<ZoomIn className="w-4 h-4" />}
          aria-label="Zoom in"
        />
        <Button
          variant="secondary"
          size="sm"
          onClick={handleZoomOut}
          leftIcon={<ZoomOut className="w-4 h-4" />}
          aria-label="Zoom out"
        />
        <Button
          variant="secondary"
          size="sm"
          onClick={handleReset}
          leftIcon={<Maximize2 className="w-4 h-4" />}
          aria-label="Reset view"
        />
        <Button
          variant="secondary"
          size="sm"
          onClick={loadGraphData}
          leftIcon={<RefreshCw className="w-4 h-4" />}
          aria-label="Refresh graph"
        />
      </div>

      <div className="absolute bottom-4 left-4 bg-[hsl(var(--surface-raised))] p-3 rounded-lg shadow-lg border border-[hsl(var(--border-subtle))]">
        <div className="text-sm font-semibold mb-2 text-[hsl(var(--text-primary))]">Legend</div>
        <div className="space-y-1 text-xs">
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-[hsl(var(--accent))]" />
            <span className="text-[hsl(var(--text-secondary))]">Person</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-[hsl(var(--warning-fg))]" />
            <span className="text-[hsl(var(--text-secondary))]">Concept</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-[hsl(var(--success-muted))]0" />
            <span className="text-[hsl(var(--text-secondary))]">Wikilink</span>
          </div>
        </div>
        <div className="mt-3 pt-3 border-t border-[hsl(var(--border-subtle))] text-xs text-[hsl(var(--text-secondary))]">
          <div>{nodes.length} nodes</div>
          <div>{edges.length} connections</div>
        </div>
      </div>
    </div>
  );
}
