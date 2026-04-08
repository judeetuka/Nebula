import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../data/models/workflow_edge_model.dart';
import '../../data/models/workflow_node_model.dart';

/// Paints bezier-curve edges between workflow nodes on the canvas.
///
/// Also draws an in-progress drag line from [dragStart] to [dragEnd] when a
/// user is connecting two nodes.
class EdgePainter extends CustomPainter {
  final List<WorkflowEdge> edges;
  final List<WorkflowNode> nodes;
  final Offset? dragStart;
  final Offset? dragEnd;
  final Color edgeColor;
  final Color dragColor;

  /// Width / height assumed per node card for port offset calculations.
  static const double nodeWidth = 180;
  static const double nodeHeight = 72;

  EdgePainter({
    required this.edges,
    required this.nodes,
    this.dragStart,
    this.dragEnd,
    this.edgeColor = Colors.white70,
    this.dragColor = Colors.blueAccent,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.0;

    for (final edge in edges) {
      final from = _nodeById(edge.fromNodeId);
      final to = _nodeById(edge.toNodeId);
      if (from == null || to == null) continue;

      final start = _outputPort(from);
      final end = _inputPort(to);

      paint.color = edgeColor;
      _drawBezier(canvas, start, end, paint);
      _drawArrowhead(canvas, start, end, paint);

      // Condition label
      if (edge.condition != null && edge.condition!.isNotEmpty) {
        final mid = Offset((start.dx + end.dx) / 2, (start.dy + end.dy) / 2);
        _drawLabel(canvas, mid, edge.condition!);
      }
    }

    // In-progress drag line
    if (dragStart != null && dragEnd != null) {
      paint.color = dragColor;
      paint.strokeWidth = 2.5;
      _drawBezier(canvas, dragStart!, dragEnd!, paint);
    }
  }

  void _drawBezier(Canvas canvas, Offset start, Offset end, Paint paint) {
    final dx = (end.dx - start.dx).abs() * 0.5;
    final path = Path()
      ..moveTo(start.dx, start.dy)
      ..cubicTo(start.dx + dx, start.dy, end.dx - dx, end.dy, end.dx, end.dy);
    canvas.drawPath(path, paint);
  }

  void _drawArrowhead(Canvas canvas, Offset start, Offset end, Paint paint) {
    final arrowPaint = Paint()
      ..color = paint.color
      ..style = PaintingStyle.fill;

    final angle = math.atan2(end.dy - start.dy, end.dx - start.dx);
    const arrowLength = 10.0;
    const arrowAngle = 0.5;

    final path = Path()
      ..moveTo(end.dx, end.dy)
      ..lineTo(
        end.dx - arrowLength * math.cos(angle - arrowAngle),
        end.dy - arrowLength * math.sin(angle - arrowAngle),
      )
      ..lineTo(
        end.dx - arrowLength * math.cos(angle + arrowAngle),
        end.dy - arrowLength * math.sin(angle + arrowAngle),
      )
      ..close();
    canvas.drawPath(path, arrowPaint);
  }

  void _drawLabel(Canvas canvas, Offset position, String text) {
    final tp = TextPainter(
      text: TextSpan(
        text: text,
        style: const TextStyle(
          color: Colors.white,
          fontSize: 10,
          fontWeight: FontWeight.w500,
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();

    final bg = RRect.fromRectAndRadius(
      Rect.fromCenter(
        center: position,
        width: tp.width + 12,
        height: tp.height + 6,
      ),
      const Radius.circular(4),
    );
    canvas.drawRRect(bg, Paint()..color = Colors.black54);
    tp.paint(
      canvas,
      Offset(position.dx - tp.width / 2, position.dy - tp.height / 2),
    );
  }

  WorkflowNode? _nodeById(String id) {
    return nodes.where((n) => n.id == id).firstOrNull;
  }

  /// Output port sits at the right-center of the node card.
  Offset _outputPort(WorkflowNode node) {
    return Offset(
      node.position.dx + nodeWidth,
      node.position.dy + nodeHeight / 2,
    );
  }

  /// Input port sits at the left-center of the node card.
  Offset _inputPort(WorkflowNode node) {
    return Offset(node.position.dx, node.position.dy + nodeHeight / 2);
  }

  @override
  bool shouldRepaint(covariant EdgePainter oldDelegate) {
    return edges != oldDelegate.edges ||
        nodes != oldDelegate.nodes ||
        dragStart != oldDelegate.dragStart ||
        dragEnd != oldDelegate.dragEnd;
  }
}
