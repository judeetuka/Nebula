import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

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

    return Scaffold(
      appBar: AppBar(
        title: Text(cluster?.name ?? 'Cluster Detail'),
      ),
      body: SingleChildScrollView(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Cluster header
            if (cluster != null) _ClusterHeader(cluster: cluster),
            const SizedBox(height: UIConstants.spacingXL),

            // QR code section
            Center(
              child: QrDisplay(
                clusterId: widget.clusterId,
                serverUrl: cluster?.serverUrl ?? '',
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),

            // Nodes section
            Text(
              'Nodes',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            _buildNodesList(nodesState, theme),
          ],
        ),
      ),
    );
  }

  Widget _buildNodesList(ClusterNodesState state, ThemeData theme) {
    if (state.isLoading) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(UIConstants.spacingXL),
          child: CircularProgressIndicator(),
        ),
      );
    }

    if (state.error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(UIConstants.spacingXL),
          child: Text(state.error!, style: TextStyle(color: theme.colorScheme.error)),
        ),
      );
    }

    if (state.nodes.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(UIConstants.spacingXL),
          child: Text(
            'No nodes connected',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.outline,
            ),
          ),
        ),
      );
    }

    return Card(
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          for (int i = 0; i < state.nodes.length; i++) ...[
            NodeListTile(node: state.nodes[i]),
            if (i < state.nodes.length - 1) const Divider(height: 1),
          ],
        ],
      ),
    );
  }
}

class _ClusterHeader extends StatelessWidget {
  final Cluster cluster;

  const _ClusterHeader({required this.cluster});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Card(
      child: Padding(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.cloud, color: theme.colorScheme.primary),
                const SizedBox(width: UIConstants.spacingSM),
                Expanded(
                  child: Text(
                    cluster.name,
                    style: theme.textTheme.titleLarge?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: UIConstants.spacingMD),
            _InfoRow(
              icon: Icons.link,
              label: 'Server',
              value: cluster.serverUrl,
            ),
            const SizedBox(height: UIConstants.spacingSM),
            _InfoRow(
              icon: Icons.calendar_today,
              label: 'Created',
              value: _formatDate(cluster.createdAt),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            _InfoRow(
              icon: Icons.devices,
              label: 'Nodes',
              value: '${cluster.nodeCount}',
            ),
          ],
        ),
      ),
    );
  }

  String _formatDate(DateTime date) {
    return '${date.year}-${date.month.toString().padLeft(2, '0')}-${date.day.toString().padLeft(2, '0')}';
  }
}

class _InfoRow extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;

  const _InfoRow({
    required this.icon,
    required this.label,
    required this.value,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Row(
      children: [
        Icon(icon, size: UIConstants.iconSM, color: theme.colorScheme.outline),
        const SizedBox(width: UIConstants.spacingSM),
        Text(
          '$label: ',
          style: theme.textTheme.bodySmall?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        Expanded(
          child: Text(
            value,
            style: theme.textTheme.bodySmall,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }
}
