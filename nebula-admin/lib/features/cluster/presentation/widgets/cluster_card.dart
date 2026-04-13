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

    return GestureDetector(
      onTap: onTap,
      child: FrostedGlass(
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
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        cluster.name,
                        style: theme.textTheme.titleMedium?.copyWith(
                          fontWeight: FontWeight.bold,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                      Row(
                        children: [
                          Icon(
                            IconlyBroken.user_3,
                            size: UIConstants.iconSM,
                            color: theme.colorScheme.onSurface.withValues(
                              alpha: 0.6,
                            ),
                          ),
                          const SizedBox(width: UIConstants.spacingXS),
                          Text(
                            '${cluster.nodeCount} node${cluster.nodeCount == 1 ? '' : 's'}',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.onSurface.withValues(
                                alpha: 0.6,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
                FrostedGlass(
                  borderRadius: BorderRadius.circular(20),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 6,
                  ),
                  shadow: false,
                  tintColor: isHealthy ? Colors.green : Colors.red,
                  opacity: 0.15,
                  child: Text(
                    isHealthy ? 'Online' : 'Offline',
                    style: TextStyle(
                      color: isHealthy ? Colors.green : Colors.red,
                      fontWeight: FontWeight.w600,
                      fontSize: 12,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              cluster.serverUrl,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );
  }
}
