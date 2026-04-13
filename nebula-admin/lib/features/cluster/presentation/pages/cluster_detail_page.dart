import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../domain/entities/cluster.dart';
import '../providers/cluster_provider.dart';
import '../widgets/node_list_tile.dart';
import '../widgets/qr_display.dart';

class ClusterDetailPage extends ConsumerStatefulWidget {
  final String clusterId;

  const ClusterDetailPage({super.key, required this.clusterId});

  @override
  ConsumerState<ClusterDetailPage> createState() => _ClusterDetailPageState();
}

class _ClusterDetailPageState extends ConsumerState<ClusterDetailPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(
      () => ref
          .read(clusterNodesProvider.notifier)
          .loadNodes(clusterId: widget.clusterId),
    );
  }

  Cluster? _findCluster() {
    final clusters = ref.read(clustersProvider).clusters;
    for (final c in clusters) {
      if (c.id == widget.clusterId) return c;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final nodesState = ref.watch(clusterNodesProvider);
    final cluster = _findCluster();
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: cluster?.name ?? 'Cluster Detail',
      body: SingleChildScrollView(
        padding: const EdgeInsets.only(
          top: UIConstants.spacingLG,
          bottom: UIConstants.spacingXXL + 40,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // -- Hero header card --
            if (cluster != null) _ClusterHeroCard(cluster: cluster),

            const SizedBox(height: UIConstants.spacingXXL),

            // -- QR onboarding section --
            _SectionHeader(
              icon: IconlyBroken.scan,
              title: 'Onboarding QR',
            ),
            const SizedBox(height: UIConstants.spacingMD),
            Center(
              child: FrostedGlass(
                borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
                padding: const EdgeInsets.all(UIConstants.spacingXL),
                child: Column(
                  children: [
                    QrDisplay(
                      clusterId: widget.clusterId,
                      serverUrl: cluster?.serverUrl ?? '',
                    ),
                    const SizedBox(height: UIConstants.spacingMD),
                    Text(
                      'Scan with Nebula Node to join this cluster',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                      ),
                    ),
                  ],
                ),
              ),
            ),

            const SizedBox(height: UIConstants.spacingXXL),

            // -- Nodes section --
            _SectionHeader(
              icon: IconlyBroken.user_3,
              title: 'Nodes',
              trailing: nodesState.nodes.isNotEmpty
                  ? _CountBadge(count: nodesState.nodes.length)
                  : null,
            ),
            const SizedBox(height: UIConstants.spacingMD),
            _buildNodesList(nodesState, theme),
          ],
        ),
      ),
    );
  }

  Widget _buildNodesList(ClusterNodesState state, ThemeData theme) {
    if (state.isLoading) {
      return const Padding(
        padding: EdgeInsets.all(UIConstants.spacingXXL),
        child: Center(child: CupertinoActivityIndicator()),
      );
    }

    if (state.error != null) {
      return Padding(
        padding: const EdgeInsets.all(UIConstants.spacingXL),
        child: FrostedGlass(
          borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
          padding: const EdgeInsets.all(UIConstants.spacingLG),
          tintColor: theme.colorScheme.error,
          opacity: 0.06,
          shadow: false,
          child: Row(
            children: [
              Icon(IconlyBroken.danger, color: theme.colorScheme.error, size: 20),
              const SizedBox(width: UIConstants.spacingMD),
              Expanded(
                child: Text(
                  state.error!,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.error,
                  ),
                ),
              ),
            ],
          ),
        ),
      );
    }

    if (state.nodes.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: UIConstants.spacingXXL),
        child: Center(
          child: Column(
            children: [
              Icon(
                IconlyBroken.user_3,
                size: 40,
                color: theme.colorScheme.onSurface.withValues(alpha: 0.25),
              ),
              const SizedBox(height: UIConstants.spacingMD),
              Text(
                'No nodes connected',
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurface.withValues(alpha: 0.45),
                ),
              ),
              const SizedBox(height: UIConstants.spacingXS),
              Text(
                'Scan the QR code above from a Nebula Node device',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurface.withValues(alpha: 0.35),
                ),
              ),
            ],
          ),
        ),
      );
    }

    return Column(
      children: [
        for (int i = 0; i < state.nodes.length; i++) ...[
          NodeListTile(node: state.nodes[i]),
          if (i < state.nodes.length - 1)
            const SizedBox(height: UIConstants.spacingSM),
        ],
      ],
    );
  }
}

// -- Hero card with cluster icon, name, URL, creation date --
class _ClusterHeroCard extends StatelessWidget {
  final Cluster cluster;

  const _ClusterHeroCard({required this.cluster});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isHealthy = cluster.nodeCount > 0;
    final statusColor = isHealthy ? Colors.green : Colors.red;

    return FrostedGlass(
      borderRadius: BorderRadius.circular(20),
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Top row: icon + name + status
          Row(
            children: [
              Container(
                width: 52,
                height: 52,
                decoration: BoxDecoration(
                  color: theme.colorScheme.primary.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                ),
                child: Icon(
                  IconlyBold.discovery,
                  color: theme.colorScheme.primary,
                  size: 26,
                ),
              ),
              const SizedBox(width: UIConstants.spacingLG),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      cluster.name,
                      style: theme.textTheme.titleLarge?.copyWith(
                        fontWeight: FontWeight.bold,
                        letterSpacing: -0.5,
                      ),
                    ),
                    const SizedBox(height: 4),
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(20),
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
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
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),

          const SizedBox(height: UIConstants.spacingXL),

          // Info rows
          _HeroInfoRow(
            icon: IconlyBroken.work,
            label: 'Server',
            value: cluster.serverUrl,
          ),
          const SizedBox(height: UIConstants.spacingMD),
          _HeroInfoRow(
            icon: IconlyBroken.calendar,
            label: 'Created',
            value: _formatDate(cluster.createdAt),
          ),
          const SizedBox(height: UIConstants.spacingMD),
          _HeroInfoRow(
            icon: IconlyBroken.user_3,
            label: 'Nodes',
            value: '${cluster.nodeCount} connected',
          ),
        ],
      ),
    );
  }

  String _formatDate(DateTime date) {
    const months = [
      'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
    ];
    return '${months[date.month - 1]} ${date.day}, ${date.year}';
  }
}

class _HeroInfoRow extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;

  const _HeroInfoRow({
    required this.icon,
    required this.label,
    required this.value,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Row(
      children: [
        Icon(
          icon,
          size: UIConstants.iconSM,
          color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
        ),
        const SizedBox(width: UIConstants.spacingSM),
        SizedBox(
          width: 64,
          child: Text(
            label,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurface.withValues(alpha: 0.45),
              fontWeight: FontWeight.w500,
            ),
          ),
        ),
        Expanded(
          child: Text(
            value,
            style: theme.textTheme.bodyMedium?.copyWith(
              fontWeight: FontWeight.w500,
            ),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }
}

// -- Section header with icon and title --
class _SectionHeader extends StatelessWidget {
  final IconData icon;
  final String title;
  final Widget? trailing;

  const _SectionHeader({
    required this.icon,
    required this.title,
    this.trailing,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Row(
        children: [
          Icon(
            icon,
            size: 20,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(width: UIConstants.spacingSM),
          Text(
            title,
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.bold,
              letterSpacing: -0.3,
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: UIConstants.spacingSM),
            trailing!,
          ],
        ],
      ),
    );
  }
}

// -- Small count badge --
class _CountBadge extends StatelessWidget {
  final int count;

  const _CountBadge({required this.count});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedGlass(
      borderRadius: BorderRadius.circular(12),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
      shadow: false,
      tintColor: theme.colorScheme.primary,
      opacity: 0.1,
      child: Text(
        '$count',
        style: TextStyle(
          color: theme.colorScheme.primary,
          fontWeight: FontWeight.w700,
          fontSize: 12,
        ),
      ),
    );
  }
}
