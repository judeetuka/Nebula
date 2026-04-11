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
        return MannyTheme.tertiaryTeal;
      case 'busy':
        return Colors.orange;
      case 'offline':
        return theme.colorScheme.error;
      default:
        return theme.colorScheme.outline;
    }
  }

  IconData _batteryIcon() {
    if (node.batteryLevel > 80) return Icons.battery_full;
    if (node.batteryLevel > 50) return Icons.battery_5_bar;
    if (node.batteryLevel > 20) return Icons.battery_3_bar;
    return Icons.battery_1_bar;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final truncatedId =
        node.nodeId.length > 10 ? '${node.nodeId.substring(0, 10)}...' : node.nodeId;

    return ListTile(
      leading: CircleAvatar(
        backgroundColor: _statusColor(context).withValues(alpha: 0.15),
        child: Icon(
          Icons.developer_board,
          color: _statusColor(context),
          size: UIConstants.iconMD,
        ),
      ),
      title: Text(truncatedId, style: theme.textTheme.bodyMedium),
      subtitle: Row(
        children: [
          _RoleBadge(role: node.role),
          const SizedBox(width: UIConstants.spacingSM),
          Icon(
            _batteryIcon(),
            size: UIConstants.iconSM,
            color: node.batteryLevel > 20
                ? theme.colorScheme.onSurfaceVariant
                : theme.colorScheme.error,
          ),
          const SizedBox(width: UIConstants.spacingXS),
          Text(
            '${node.batteryLevel}%',
            style: theme.textTheme.bodySmall,
          ),
        ],
      ),
      trailing: SizedBox(
        width: 80,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Text(
              'CPU ${(node.cpuLoad * 100).toStringAsFixed(0)}%',
              style: theme.textTheme.bodySmall,
            ),
            const SizedBox(height: UIConstants.spacingXS),
            ClipRRect(
              borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
              child: LinearProgressIndicator(
                value: node.cpuLoad,
                minHeight: 4,
                backgroundColor:
                    theme.colorScheme.surfaceContainerHighest,
                valueColor: AlwaysStoppedAnimation<Color>(
                  node.cpuLoad > 0.8
                      ? theme.colorScheme.error
                      : theme.colorScheme.primary,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _RoleBadge extends StatelessWidget {
  final String role;

  const _RoleBadge({required this.role});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: UIConstants.spacingSM,
        vertical: 2,
      ),
      decoration: BoxDecoration(
        color: theme.colorScheme.secondaryContainer,
        borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
      ),
      child: Text(
        role,
        style: theme.textTheme.labelSmall?.copyWith(
          color: theme.colorScheme.onSecondaryContainer,
        ),
      ),
    );
  }
}
