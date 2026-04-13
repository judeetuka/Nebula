import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

/// Frosted glass card displaying core node metrics in a structured layout.
///
/// Shows node ID, cluster ID, state, and configuration status with
/// Iconly icons and consistent spacing.
class NodeMetricsCard extends StatelessWidget {
  final String nodeId;
  final String? clusterId;
  final String state;
  final bool isConfigured;

  const NodeMetricsCard({
    super.key,
    required this.nodeId,
    this.clusterId,
    required this.state,
    required this.isConfigured,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedGlass(
      borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Section header
          Row(
            children: [
              Icon(
                IconlyBold.graph,
                size: UIConstants.iconMD,
                color: theme.colorScheme.primary,
              ),
              const SizedBox(width: UIConstants.spacingSM),
              Text(
                'Node Information',
                style: theme.textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
              ),
            ],
          ),
          const SizedBox(height: UIConstants.spacingLG),

          _MetricRow(
            icon: IconlyBroken.document,
            label: 'Node ID',
            value: _truncateId(nodeId),
          ),
          const SizedBox(height: UIConstants.spacingMD),
          _MetricRow(
            icon: IconlyBroken.discovery,
            label: 'Cluster',
            value: clusterId != null ? _truncateId(clusterId!) : 'Not joined',
          ),
          const SizedBox(height: UIConstants.spacingMD),
          _MetricRow(
            icon: IconlyBroken.activity,
            label: 'State',
            value: state,
            valueColor: _stateColor(state),
          ),
          const SizedBox(height: UIConstants.spacingMD),
          _MetricRow(
            icon: IconlyBroken.shield_done,
            label: 'Configured',
            value: isConfigured ? 'Yes' : 'No',
            valueColor: isConfigured ? Colors.green : Colors.grey,
          ),
        ],
      ),
    );
  }

  String _truncateId(String id) {
    if (id.length <= 16) return id;
    return '${id.substring(0, 8)}...${id.substring(id.length - 8)}';
  }

  Color _stateColor(String state) {
    return switch (state) {
      'active' => Colors.green,
      'configured' => Colors.blue,
      'idle' => Colors.orange,
      'uninitialized' => Colors.red,
      _ => Colors.grey,
    };
  }
}

class _MetricRow extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;
  final Color? valueColor;

  const _MetricRow({
    required this.icon,
    required this.label,
    required this.value,
    this.valueColor,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Row(
      children: [
        Icon(
          icon,
          size: UIConstants.iconMD,
          color: theme.colorScheme.onSurfaceVariant,
        ),
        const SizedBox(width: UIConstants.spacingMD),
        Text(
          label,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const Spacer(),
        Flexible(
          child: Text(
            value,
            style: theme.textTheme.bodyMedium?.copyWith(
              fontWeight: FontWeight.w600,
              color: valueColor,
              fontFamily: 'monospace',
            ),
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.end,
          ),
        ),
      ],
    );
  }
}
