import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/cluster_provider.dart';
import '../widgets/cluster_card.dart';

class DashboardPage extends ConsumerStatefulWidget {
  const DashboardPage({super.key, this.scrollController});
  final ScrollController? scrollController;

  @override
  ConsumerState<DashboardPage> createState() => _DashboardPageState();
}

class _DashboardPageState extends ConsumerState<DashboardPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(() => ref.read(clustersProvider.notifier).loadClusters());
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(clustersProvider);
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: 'Clusters',
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.plus),
          onPressed: () =>
              Navigator.of(context).pushNamed(AppRoutes.clusterCreate),
          tooltip: 'New Cluster',
        ),
      ],
      body: _buildBody(state, theme),
    );
  }

  Widget _buildBody(ClustersState state, ThemeData theme) {
    if (state.isLoading && state.clusters.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }

    if (state.error != null && state.clusters.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(IconlyBroken.danger, size: 48, color: theme.colorScheme.error),
            const SizedBox(height: UIConstants.spacingMD),
            Text(state.error!, style: theme.textTheme.bodyLarge),
            const SizedBox(height: UIConstants.spacingLG),
            FilledButton(
              onPressed: () =>
                  ref.read(clustersProvider.notifier).loadClusters(),
              child: const Text('Retry'),
            ),
          ],
        ),
      );
    }

    if (state.clusters.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              IconlyBroken.discovery,
              size: 64,
              color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
            ),
            const SizedBox(height: UIConstants.spacingMD),
            Text(
              'No clusters yet',
              style: theme.textTheme.titleMedium?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              'Create your first compute cluster',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
              ),
            ),
          ],
        ),
      );
    }

    return RefreshIndicator(
      onRefresh: () => ref.read(clustersProvider.notifier).loadClusters(),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final crossAxisCount = constraints.maxWidth > 800
              ? 3
              : (constraints.maxWidth > 500 ? 2 : 1);

          if (crossAxisCount == 1) {
            return ListView.separated(
              controller: widget.scrollController,
              padding: UIConstants.paddingLG,
              itemCount: state.clusters.length + 1, // +1 for bottom spacer
              separatorBuilder: (_, _) =>
                  const SizedBox(height: UIConstants.spacingSM),
              itemBuilder: (context, index) {
                if (index == state.clusters.length) {
                  return const SizedBox(height: 80);
                }
                final cluster = state.clusters[index];
                return ClusterCard(
                  cluster: cluster,
                  onTap: () => Navigator.of(
                    context,
                  ).pushNamed(AppRoutes.clusterDetail(cluster.id)),
                );
              },
            );
          }

          return GridView.builder(
            controller: widget.scrollController,
            padding: UIConstants.paddingLG,
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: crossAxisCount,
              crossAxisSpacing: UIConstants.spacingMD,
              mainAxisSpacing: UIConstants.spacingMD,
              childAspectRatio: 2.2,
            ),
            itemCount: state.clusters.length,
            itemBuilder: (context, index) {
              final cluster = state.clusters[index];
              return ClusterCard(
                cluster: cluster,
                onTap: () => Navigator.of(
                  context,
                ).pushNamed(AppRoutes.clusterDetail(cluster.id)),
              );
            },
          );
        },
      ),
    );
  }
}
