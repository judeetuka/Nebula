import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/cluster_provider.dart';
import '../widgets/cluster_card.dart';

class DashboardPage extends ConsumerStatefulWidget {
  const DashboardPage({super.key});

  @override
  ConsumerState<DashboardPage> createState() => _DashboardPageState();
}

class _DashboardPageState extends ConsumerState<DashboardPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(
      () => ref.read(clustersProvider.notifier).loadClusters(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(clustersProvider);
    final theme = Theme.of(context);

    return Scaffold(
      body: _buildBody(state, theme),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () =>
            Navigator.of(context).pushNamed(AppRoutes.clusterCreate),
        icon: const Icon(Icons.add),
        label: const Text('New Cluster'),
      ),
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
            Icon(Icons.error_outline,
                size: 48, color: theme.colorScheme.error),
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
            Icon(Icons.cloud_off,
                size: 64, color: theme.colorScheme.outlineVariant),
            const SizedBox(height: UIConstants.spacingMD),
            Text(
              'No clusters yet',
              style: theme.textTheme.titleMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              'Create your first compute cluster',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.outline,
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
          final crossAxisCount = constraints.maxWidth > 800 ? 3 : (constraints.maxWidth > 500 ? 2 : 1);

          if (crossAxisCount == 1) {
            return ListView.separated(
              padding: UIConstants.paddingLG,
              itemCount: state.clusters.length,
              separatorBuilder: (_, _) =>
                  const SizedBox(height: UIConstants.spacingSM),
              itemBuilder: (context, index) {
                final cluster = state.clusters[index];
                return ClusterCard(
                  cluster: cluster,
                  onTap: () => Navigator.of(context)
                      .pushNamed(AppRoutes.clusterDetail(cluster.id)),
                );
              },
            );
          }

          return GridView.builder(
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
                onTap: () => Navigator.of(context)
                    .pushNamed(AppRoutes.clusterDetail(cluster.id)),
              );
            },
          );
        },
      ),
    );
  }
}
