import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../domain/entities/cluster.dart';

class ClusterCard extends StatelessWidget {
  final Cluster cluster;
  final VoidCallback onTap;

  const ClusterCard({super.key, required this.cluster, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isHealthy = cluster.nodeCount > 0;
    final statusColor = isHealthy ? Colors.green : Colors.red;

    return GestureDetector(
      onTap: onTap,
      child: FrostedGlass(
        borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Top row: icon + name + status pill
            Row(
              children: [
                // Cluster icon container
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: theme.colorScheme.primary.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                  ),
                  child: Icon(
                    IconlyBold.discovery,
                    color: theme.colorScheme.primary,
                    size: 22,
                  ),
                ),
                const SizedBox(width: UIConstants.spacingMD),

                // Name + node count
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        cluster.name,
                        style: theme.textTheme.titleMedium?.copyWith(
                          fontWeight: FontWeight.bold,
                          letterSpacing: -0.3,
                        ),
                        overflow: TextOverflow.ellipsis,
                        maxLines: 1,
                      ),
                      const SizedBox(height: 2),
                      Row(
                        children: [
                          Icon(
                            IconlyBroken.user_3,
                            size: UIConstants.iconSM,
                            color: theme.colorScheme.onSurface
                                .withValues(alpha: 0.5),
                          ),
                          const SizedBox(width: UIConstants.spacingXS),
                          Text(
                            '${cluster.nodeCount} node${cluster.nodeCount == 1 ? '' : 's'}',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.onSurface
                                  .withValues(alpha: 0.55),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),

                // Status pill
                FrostedGlass(
                  borderRadius: BorderRadius.circular(20),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 5,
                  ),
                  shadow: false,
                  tintColor: statusColor,
                  opacity: 0.12,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 7,
                        height: 7,
                        decoration: BoxDecoration(
                          color: statusColor,
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Text(
                        isHealthy ? 'Online' : 'Offline',
                        style: TextStyle(
                          color: statusColor,
                          fontWeight: FontWeight.w600,
                          fontSize: 12,
                          letterSpacing: 0.2,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),

            const Spacer(),

            // Bottom section: server URL + date
            Container(
              padding: const EdgeInsets.symmetric(
                horizontal: UIConstants.spacingMD,
                vertical: UIConstants.spacingSM,
              ),
              decoration: BoxDecoration(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.04),
                borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
              ),
              child: Row(
                children: [
                  Icon(
                    IconlyBroken.work,
                    size: UIConstants.iconSM,
                    color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
                  ),
                  const SizedBox(width: UIConstants.spacingSM),
                  Expanded(
                    child: Text(
                      cluster.serverUrl,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface
                            .withValues(alpha: 0.45),
                        fontFamily: 'monospace',
                        fontSize: 11,
                      ),
                      overflow: TextOverflow.ellipsis,
                      maxLines: 1,
                    ),
                  ),
                  const SizedBox(width: UIConstants.spacingSM),
                  Icon(
                    IconlyBroken.calendar,
                    size: 13,
                    color: theme.colorScheme.onSurface.withValues(alpha: 0.35),
                  ),
                  const SizedBox(width: 4),
                  Text(
                    _formatDate(cluster.createdAt),
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurface
                          .withValues(alpha: 0.4),
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  String _formatDate(DateTime date) {
    const months = [
      'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
    ];
    return '${months[date.month - 1]} ${date.day}';
  }
}
