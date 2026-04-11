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

    return Card(
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: UIConstants.paddingLG,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.cloud_outlined,
                    color: theme.colorScheme.primary,
                    size: UIConstants.iconLG,
                  ),
                  const SizedBox(width: UIConstants.spacingSM),
                  Expanded(
                    child: Text(
                      cluster.name,
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.w600,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  Container(
                    width: 10,
                    height: 10,
                    decoration: BoxDecoration(
                      color: cluster.nodeCount > 0
                          ? MannyTheme.tertiaryTeal
                          : theme.colorScheme.outline,
                      shape: BoxShape.circle,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: UIConstants.spacingMD),
              Row(
                children: [
                  Icon(
                    Icons.devices,
                    size: UIConstants.iconSM,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: UIConstants.spacingXS),
                  Text(
                    '${cluster.nodeCount} node${cluster.nodeCount == 1 ? '' : 's'}',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: UIConstants.spacingXS),
              Text(
                cluster.serverUrl,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.outline,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
