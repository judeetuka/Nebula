import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../cluster/domain/entities/cluster.dart';
import '../../../cluster/domain/entities/node_info.dart';

/// Summary card showing high-level cluster statistics.
///
/// Displays total node count, online/offline ratio, and aggregated task
/// or cluster counts.
class ClusterStatsCard extends StatelessWidget {
  final List<Cluster> clusters;
  final List<NodeInfo> nodes;

  const ClusterStatsCard({
    super.key,
    required this.clusters,
    required this.nodes,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final totalNodes = nodes.length;
    final onlineNodes = nodes
        .where((n) => n.status == 'online' || n.status == 'busy')
        .length;
    final offlineNodes = totalNodes - onlineNodes;
    final avgCpu = totalNodes > 0
        ? nodes.fold<double>(0, (sum, n) => sum + n.cpuLoad) / totalNodes
        : 0.0;
    final avgBattery = totalNodes > 0
        ? nodes.fold<int>(0, (sum, n) => sum + n.batteryLevel) ~/ totalNodes
        : 0;

    return Card(
      child: Padding(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Cluster Overview',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),
            Row(
              children: [
                Expanded(
                  child: _Stat(
                    icon: Icons.cloud,
                    label: 'Clusters',
                    value: '${clusters.length}',
                    color: theme.colorScheme.primary,
                  ),
                ),
                Expanded(
                  child: _Stat(
                    icon: Icons.devices,
                    label: 'Total Nodes',
                    value: '$totalNodes',
                    color: theme.colorScheme.secondary,
                  ),
                ),
                Expanded(
                  child: _Stat(
                    icon: Icons.check_circle,
                    label: 'Online',
                    value: '$onlineNodes',
                    color: MannyTheme.tertiaryTeal,
                  ),
                ),
                Expanded(
                  child: _Stat(
                    icon: Icons.cancel,
                    label: 'Offline',
                    value: '$offlineNodes',
                    color: theme.colorScheme.error,
                  ),
                ),
              ],
            ),
            const SizedBox(height: UIConstants.spacingLG),
            Row(
              children: [
                Expanded(
                  child: _ProgressStat(
                    label: 'Avg CPU',
                    value: avgCpu,
                    displayValue: '${(avgCpu * 100).toStringAsFixed(0)}%',
                    color: theme.colorScheme.primary,
                    theme: theme,
                  ),
                ),
                const SizedBox(width: UIConstants.spacingLG),
                Expanded(
                  child: _ProgressStat(
                    label: 'Avg Battery',
                    value: avgBattery / 100,
                    displayValue: '$avgBattery%',
                    color: MannyTheme.tertiaryTeal,
                    theme: theme,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;
  final Color color;

  const _Stat({
    required this.icon,
    required this.label,
    required this.value,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Column(
      children: [
        Icon(icon, color: color, size: UIConstants.iconLG),
        const SizedBox(height: UIConstants.spacingXS),
        Text(
          value,
          style: theme.textTheme.headlineSmall?.copyWith(
            fontWeight: FontWeight.bold,
          ),
        ),
        Text(
          label,
          style: theme.textTheme.labelSmall?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }
}

class _ProgressStat extends StatelessWidget {
  final String label;
  final double value;
  final String displayValue;
  final Color color;
  final ThemeData theme;

  const _ProgressStat({
    required this.label,
    required this.value,
    required this.displayValue,
    required this.color,
    required this.theme,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(label, style: theme.textTheme.bodySmall),
            Text(
              displayValue,
              style: theme.textTheme.bodySmall?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
        const SizedBox(height: UIConstants.spacingXS),
        ClipRRect(
          borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
          child: LinearProgressIndicator(
            value: value.clamp(0.0, 1.0),
            minHeight: 6,
            backgroundColor: theme.colorScheme.surfaceContainerHighest,
            valueColor: AlwaysStoppedAnimation<Color>(color),
          ),
        ),
      ],
    );
  }
}
