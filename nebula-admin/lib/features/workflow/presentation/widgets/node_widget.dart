import 'package:flutter/material.dart';

import '../../data/models/workflow_node_model.dart';

/// Renders a single workflow node card on the canvas.
///
/// Features:
/// - Color-coded by node type (plugin / condition / delay / trigger / loop)
/// - Input port (left circle) and output port (right circle)
/// - Drag handle for repositioning
/// - Selection highlight
class NodeWidget extends StatelessWidget {
  final WorkflowNode node;
  final bool isSelected;
  final VoidCallback onTap;
  final ValueChanged<Offset> onDragUpdate;
  final VoidCallback onDragEnd;
  final VoidCallback onOutputPortTap;
  final VoidCallback onInputPortTap;

  static const double width = 180;
  static const double height = 72;
  static const double portRadius = 7;

  const NodeWidget({
    super.key,
    required this.node,
    required this.isSelected,
    required this.onTap,
    required this.onDragUpdate,
    required this.onDragEnd,
    required this.onOutputPortTap,
    required this.onInputPortTap,
  });

  Color _typeColor(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    switch (node.type) {
      case WorkflowNode.typePlugin:
        return cs.primaryContainer;
      case WorkflowNode.typeCondition:
        return cs.tertiaryContainer;
      case WorkflowNode.typeDelay:
        return cs.secondaryContainer;
      case WorkflowNode.typeTrigger:
        return cs.errorContainer;
      case WorkflowNode.typeLoop:
        return cs.secondaryContainer;
      default:
        return cs.surfaceContainerHighest;
    }
  }

  IconData get _typeIcon {
    switch (node.type) {
      case WorkflowNode.typePlugin:
        return Icons.extension;
      case WorkflowNode.typeCondition:
        return Icons.call_split;
      case WorkflowNode.typeDelay:
        return Icons.timer;
      case WorkflowNode.typeTrigger:
        return Icons.play_circle_outline;
      case WorkflowNode.typeLoop:
        return Icons.loop;
      default:
        return Icons.widgets;
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final bgColor = _typeColor(context);

    return GestureDetector(
      onTap: onTap,
      onPanUpdate: (d) => onDragUpdate(d.delta),
      onPanEnd: (_) => onDragEnd(),
      child: SizedBox(
        width: width,
        height: height,
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            // Card body
            Container(
              width: width,
              height: height,
              decoration: BoxDecoration(
                color: bgColor,
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: isSelected
                      ? theme.colorScheme.primary
                      : theme.colorScheme.outlineVariant,
                  width: isSelected ? 2.5 : 1,
                ),
                boxShadow: [
                  if (isSelected)
                    BoxShadow(
                      color: theme.colorScheme.primary.withValues(alpha: 0.3),
                      blurRadius: 8,
                      spreadRadius: 1,
                    ),
                ],
              ),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              child: Row(
                children: [
                  Icon(_typeIcon, size: 20, color: theme.colorScheme.onSurface),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          node.label,
                          style: theme.textTheme.labelLarge?.copyWith(
                            fontWeight: FontWeight.w600,
                          ),
                          overflow: TextOverflow.ellipsis,
                        ),
                        Text(
                          node.action,
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),

            // Input port (left)
            Positioned(
              left: -portRadius,
              top: height / 2 - portRadius,
              child: GestureDetector(
                onTap: onInputPortTap,
                child: _PortDot(
                  color: theme.colorScheme.secondary,
                  radius: portRadius,
                ),
              ),
            ),

            // Output port (right)
            Positioned(
              right: -portRadius,
              top: height / 2 - portRadius,
              child: GestureDetector(
                onTap: onOutputPortTap,
                child: _PortDot(
                  color: theme.colorScheme.primary,
                  radius: portRadius,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _PortDot extends StatelessWidget {
  final Color color;
  final double radius;

  const _PortDot({required this.color, required this.radius});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: radius * 2,
      height: radius * 2,
      decoration: BoxDecoration(
        color: color,
        shape: BoxShape.circle,
        border: Border.all(
          color: Theme.of(context).colorScheme.surface,
          width: 2,
        ),
      ),
    );
  }
}
