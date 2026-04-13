import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../cluster/domain/entities/cluster.dart';
import '../../../cluster/domain/entities/node_info.dart';

/// Summary card showing high-level cluster statistics.
///
/// Displays total node count, online/offline ratio, and aggregated
/// CPU / battery metrics using FrostedGlass + ProgressBar.
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

    return FrostedGlass(
      padding: UIConstants.paddingLG,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: theme.colorScheme.primary.withValues(alpha: 0.15),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Icon(
                  IconlyBold.discovery,
                  color: theme.colorScheme.primary,
                ),
              ),
              const SizedBox(width: 12),
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Cluster Overview',
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  Text(
                    '${clusters.length} cluster${clusters.length == 1 ? '' : 's'} · $totalNodes nodes',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
                    ),
                  ),
                ],
              ),
              const Spacer(),
              FrostedGlass(
                borderRadius: BorderRadius.circular(20),
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 6,
                ),
                shadow: false,
                tintColor: offlineNodes == 0 ? Colors.green : Colors.orange,
                opacity: 0.15,
                child: Text(
                  offlineNodes == 0 ? 'All Online' : '$offlineNodes Offline',
                  style: TextStyle(
                    color: offlineNodes == 0 ? Colors.green : Colors.orange,
                    fontWeight: FontWeight.w600,
                    fontSize: 12,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 20),

          // Stat counters
          Row(
            children: [
              Expanded(
                child: _Stat(
                  icon: IconlyBold.discovery,
                  label: 'Clusters',
                  value: '${clusters.length}',
                  color: theme.colorScheme.primary,
                ),
              ),
              Expanded(
                child: _Stat(
                  icon: IconlyBold.user_3,
                  label: 'Total Nodes',
                  value: '$totalNodes',
                  color: theme.colorScheme.secondary,
                ),
              ),
              Expanded(
                child: _Stat(
                  icon: IconlyBold.shield_done,
                  label: 'Online',
                  value: '$onlineNodes',
                  color: Colors.green,
                ),
              ),
              Expanded(
                child: _Stat(
                  icon: IconlyBold.close_square,
                  label: 'Offline',
                  value: '$offlineNodes',
                  color: theme.colorScheme.error,
                ),
              ),
            ],
          ),
          const SizedBox(height: 20),

          // CPU progress
          Text(
            'Avg CPU Usage',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
            ),
          ),
          const SizedBox(height: 6),
          ProgressBar(
            progress: avgCpu.clamp(0.0, 1.0),
            height: 10,
            progressColor: theme.colorScheme.primary,
          ),
          const SizedBox(height: 4),
          Text(
            '${(avgCpu * 100).toStringAsFixed(0)}% across all nodes',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
          ),
          const SizedBox(height: 12),

          // Battery progress
          Text(
            'Avg Battery',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
            ),
          ),
          const SizedBox(height: 6),
          ProgressBar(
            progress: (avgBattery / 100).clamp(0.0, 1.0),
            height: 10,
            progressColor: theme.colorScheme.tertiary,
          ),
          const SizedBox(height: 4),
          Text(
            '$avgBattery% average across all nodes',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
          ),
        ],
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
            color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
          ),
        ),
      ],
    );
  }
}
