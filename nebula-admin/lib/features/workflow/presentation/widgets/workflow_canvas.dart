import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/workflow_provider.dart';
import 'edge_painter.dart';
import 'node_widget.dart';

/// The main canvas area for the workflow editor.
///
/// Uses [InteractiveViewer] for pan & zoom. Nodes are rendered as positioned
/// widgets inside a [Stack], and edges are painted beneath them via
/// [CustomPaint] + [EdgePainter].
class WorkflowCanvas extends ConsumerStatefulWidget {
  const WorkflowCanvas({super.key});

  @override
  ConsumerState<WorkflowCanvas> createState() => _WorkflowCanvasState();
}

class _WorkflowCanvasState extends ConsumerState<WorkflowCanvas> {
  /// Canvas size — large enough that users can spread nodes out.
  static const double _canvasWidth = 4000;
  static const double _canvasHeight = 3000;

  Offset? _dragLineEnd;

  @override
  Widget build(BuildContext context) {
    final editorState = ref.watch(workflowEditorProvider);
    final workflow = editorState.workflow;
    if (workflow == null) return const SizedBox.shrink();

    final theme = Theme.of(context);

    // Compute the drag-start position from the connecting node's output port.
    Offset? dragStart;
    if (editorState.connectingFromNodeId != null) {
      final fromNode = workflow.nodes
          .where((n) => n.id == editorState.connectingFromNodeId)
          .firstOrNull;
      if (fromNode != null) {
        dragStart = Offset(
          fromNode.position.dx + NodeWidget.width,
          fromNode.position.dy + NodeWidget.height / 2,
        );
      }
    }

    return InteractiveViewer(
      constrained: false,
      boundaryMargin: const EdgeInsets.all(200),
      minScale: 0.25,
      maxScale: 2.0,
      child: GestureDetector(
        onTapDown: (_) {
          // Deselect node when tapping empty canvas
          ref.read(workflowEditorProvider.notifier).selectNode(null);
          ref.read(workflowEditorProvider.notifier).cancelConnecting();
          setState(() => _dragLineEnd = null);
        },
        child: SizedBox(
          width: _canvasWidth,
          height: _canvasHeight,
          child: Stack(
            children: [
              // Grid background
              Positioned.fill(
                child: CustomPaint(
                  painter: _GridPainter(
                    color: theme.colorScheme.outlineVariant,
                  ),
                ),
              ),

              // Edges layer
              Positioned.fill(
                child: MouseRegion(
                  onHover: (event) {
                    if (editorState.connectingFromNodeId != null) {
                      setState(() => _dragLineEnd = event.localPosition);
                    }
                  },
                  child: CustomPaint(
                    painter: EdgePainter(
                      edges: workflow.edges,
                      nodes: workflow.nodes,
                      dragStart: dragStart,
                      dragEnd: _dragLineEnd,
                      edgeColor: theme.colorScheme.onSurface.withValues(
                        alpha: 0.5,
                      ),
                      dragColor: theme.colorScheme.primary,
                    ),
                  ),
                ),
              ),

              // Nodes layer
              for (final node in workflow.nodes)
                Positioned(
                  left: node.position.dx,
                  top: node.position.dy,
                  child: NodeWidget(
                    node: node,
                    isSelected: node.id == editorState.selectedNodeId,
                    onTap: () {
                      // If we're connecting, finish the connection.
                      if (editorState.connectingFromNodeId != null) {
                        ref
                            .read(workflowEditorProvider.notifier)
                            .finishConnecting(node.id);
                        setState(() => _dragLineEnd = null);
                      } else {
                        ref
                            .read(workflowEditorProvider.notifier)
                            .selectNode(node.id);
                      }
                    },
                    onDragUpdate: (delta) {
                      ref
                          .read(workflowEditorProvider.notifier)
                          .moveNode(node.id, node.position + delta);
                    },
                    onDragEnd: () {},
                    onOutputPortTap: () {
                      ref
                          .read(workflowEditorProvider.notifier)
                          .startConnecting(node.id);
                    },
                    onInputPortTap: () {
                      if (editorState.connectingFromNodeId != null) {
                        ref
                            .read(workflowEditorProvider.notifier)
                            .finishConnecting(node.id);
                        setState(() => _dragLineEnd = null);
                      }
                    },
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Paints a dot-grid background on the canvas for spatial orientation.
class _GridPainter extends CustomPainter {
  final Color color;

  _GridPainter({required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    const spacing = 32.0;
    final paint = Paint()
      ..color = color.withValues(alpha: 0.3)
      ..strokeWidth = 1;

    for (double x = 0; x < size.width; x += spacing) {
      for (double y = 0; y < size.height; y += spacing) {
        canvas.drawCircle(Offset(x, y), 1, paint);
      }
    }
  }

  @override
  bool shouldRepaint(covariant _GridPainter oldDelegate) {
    return color != oldDelegate.color;
  }
}
