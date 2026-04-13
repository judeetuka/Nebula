import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../domain/entities/node_info.dart';

class NodeListTile extends StatelessWidget {
  final NodeInfo node;

  const NodeListTile({super.key, required this.node});

  Color _statusColor(BuildContext context) {
    final theme = Theme.of(context);
    switch (node.status) {
      case 'online':
        return Colors.green;
      case 'busy':
        return Colors.orange;
      case 'offline':
        return theme.colorScheme.error;
      default:
        return theme.colorScheme.onSurface.withValues(alpha: 0.4);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final truncatedId = node.nodeId.length > 10
        ? '${node.nodeId.substring(0, 10)}...'
        : node.nodeId;
    final batteryFraction = node.batteryLevel / 100.0;

    return FrostedGlass(
      padding: const EdgeInsets.all(14),
      child: Row(
        children: [
          // Status icon
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: _statusColor(context).withValues(alpha: 0.15),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Icon(
              IconlyBold.discovery,
              color: _statusColor(context),
              size: UIConstants.iconMD,
            ),
          ),
          const SizedBox(width: 12),

          // Node info
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  truncatedId,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    // Role badge
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(
                        UIConstants.radiusSmall,
                      ),
                      padding: const EdgeInsets.symmetric(
                        horizontal: UIConstants.spacingSM,
                        vertical: 2,
                      ),
                      shadow: false,
                      tintColor: theme.colorScheme.secondary,
                      opacity: 0.15,
                      child: Text(
                        node.role,
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: theme.colorScheme.secondary,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    const SizedBox(width: UIConstants.spacingSM),
                    Icon(
                      IconlyBroken.chart,
                      size: UIConstants.iconSM,
                      color: node.batteryLevel > 20
                          ? theme.colorScheme.onSurface.withValues(alpha: 0.5)
                          : theme.colorScheme.error,
                    ),
                    const SizedBox(width: UIConstants.spacingXS),
                    Text(
                      '${node.batteryLevel}%',
                      style: theme.textTheme.bodySmall,
                    ),
                  ],
                ),
              ],
            ),
          ),

          // CPU indicator
          SizedBox(
            width: 80,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Text(
                  'CPU ${(node.cpuLoad * 100).toStringAsFixed(0)}%',
                  style: theme.textTheme.bodySmall?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: UIConstants.spacingXS),
                ProgressBar(
                  progress: node.cpuLoad,
                  height: 6,
                  progressColor: node.cpuLoad > 0.8
                      ? theme.colorScheme.error
                      : theme.colorScheme.primary,
                ),
                const SizedBox(height: 6),
                ProgressBar(
                  progress: batteryFraction,
                  height: 6,
                  progressColor: node.batteryLevel > 20
                      ? theme.colorScheme.tertiary
                      : theme.colorScheme.error,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
