import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../domain/entities/node_info.dart';

class NodeListTile extends StatelessWidget {
  final NodeInfo node;

  const NodeListTile({super.key, required this.node});

  Color _statusColor(ThemeData theme) {
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

  String get _statusLabel {
    switch (node.status) {
      case 'online':
        return 'Online';
      case 'busy':
        return 'Busy';
      case 'offline':
        return 'Offline';
      default:
        return 'Unknown';
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final statusColor = _statusColor(theme);
    final truncatedId =
        node.nodeId.length > 12 ? '${node.nodeId.substring(0, 12)}...' : node.nodeId;
    final batteryFraction = node.batteryLevel / 100.0;
    final cpuPercent = (node.cpuLoad * 100).toStringAsFixed(0);

    return FrostedGlass(
      borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
      padding: const EdgeInsets.all(UIConstants.spacingLG),
      child: Row(
        children: [
          // Status icon with colored background
          Container(
            width: 44,
            height: 44,
            decoration: BoxDecoration(
              color: statusColor.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
            ),
            child: Icon(
              IconlyBold.discovery,
              color: statusColor,
              size: 22,
            ),
          ),
          const SizedBox(width: UIConstants.spacingMD),

          // Node ID + role badge + status badge
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  truncatedId,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontWeight: FontWeight.w700,
                    letterSpacing: -0.2,
                  ),
                ),
                const SizedBox(height: 6),
                Row(
                  children: [
                    // Role badge
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(6),
                      padding: const EdgeInsets.symmetric(
                        horizontal: UIConstants.spacingSM,
                        vertical: 3,
                      ),
                      shadow: false,
                      tintColor: theme.colorScheme.primary,
                      opacity: 0.1,
                      child: Text(
                        node.role.toUpperCase(),
                        style: theme.textTheme.labelSmall?.copyWith(
                          color: theme.colorScheme.primary,
                          fontWeight: FontWeight.w700,
                          fontSize: 10,
                          letterSpacing: 0.8,
                        ),
                      ),
                    ),
                    const SizedBox(width: UIConstants.spacingSM),

                    // Status badge
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(6),
                      padding: const EdgeInsets.symmetric(
                        horizontal: UIConstants.spacingSM,
                        vertical: 3,
                      ),
                      shadow: false,
                      tintColor: statusColor,
                      opacity: 0.1,
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Container(
                            width: 6,
                            height: 6,
                            decoration: BoxDecoration(
                              color: statusColor,
                              shape: BoxShape.circle,
                            ),
                          ),
                          const SizedBox(width: 4),
                          Text(
                            _statusLabel,
                            style: TextStyle(
                              color: statusColor,
                              fontWeight: FontWeight.w600,
                              fontSize: 10,
                              letterSpacing: 0.3,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),

          // CPU + Battery metric columns
          SizedBox(
            width: 96,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                // CPU
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    Icon(
                      IconlyBroken.activity,
                      size: 13,
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                    ),
                    const SizedBox(width: 4),
                    Text(
                      'CPU $cpuPercent%',
                      style: theme.textTheme.labelSmall?.copyWith(
                        fontWeight: FontWeight.w600,
                        color: theme.colorScheme.onSurface.withValues(alpha: 0.7),
                        fontSize: 11,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 5),
                ProgressBar(
                  progress: node.cpuLoad,
                  height: 5,
                  progressColor: node.cpuLoad > 0.8
                      ? theme.colorScheme.error
                      : theme.colorScheme.primary,
                ),
                const SizedBox(height: UIConstants.spacingSM),

                // Battery
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    Icon(
                      IconlyBroken.chart,
                      size: 13,
                      color: node.batteryLevel > 20
                          ? theme.colorScheme.onSurface.withValues(alpha: 0.5)
                          : theme.colorScheme.error,
                    ),
                    const SizedBox(width: 4),
                    Text(
                      'BAT ${node.batteryLevel}%',
                      style: theme.textTheme.labelSmall?.copyWith(
                        fontWeight: FontWeight.w600,
                        color: node.batteryLevel > 20
                            ? theme.colorScheme.onSurface.withValues(alpha: 0.7)
                            : theme.colorScheme.error,
                        fontSize: 11,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 5),
                ProgressBar(
                  progress: batteryFraction,
                  height: 5,
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
