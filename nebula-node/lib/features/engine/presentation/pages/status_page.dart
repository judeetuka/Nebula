import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/engine_provider.dart';
import '../widgets/connection_indicator.dart';
import '../widgets/node_metrics_card.dart';
import '../widgets/role_badge.dart';

class StatusPage extends ConsumerWidget {
  const StatusPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final statusAsync = ref.watch(nodeStatusStreamProvider);

    return Scaffold(
      appBar: BlurredAppBar(
        title: 'Node Status',
        centerTitle: true,
        actions: [
          IconButton(
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => Navigator.pushNamed(context, AppRoutes.settings),
            tooltip: 'Settings',
          ),
        ],
      ),
      body: statusAsync.when(
        loading: () => Padding(
          padding: UIConstants.paddingXL,
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const ProgressBar(progress: 1.0),
              const SizedBox(height: UIConstants.spacingLG),
              Text(
                'Loading node status...',
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
              ),
            ],
          ),
        ),
        error: (error, stack) => _buildErrorView(context, ref, error),
        data: (status) => _buildStatusView(context, ref, status),
      ),
    );
  }

  Widget _buildErrorView(
    BuildContext context,
    WidgetRef ref,
    Object error,
  ) {
    final theme = Theme.of(context);

    // Show toast on first render of error state.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      NotificationToast.error(context, 'Failed to load node status');
    });

    return Center(
      child: Padding(
        padding: UIConstants.paddingXL,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.error_outline,
              size: 48,
              color: theme.colorScheme.error,
            ),
            const SizedBox(height: UIConstants.spacingLG),
            Text(
              'Failed to load node status',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              error.toString(),
              style: theme.textTheme.bodySmall,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            FilledButton.icon(
              onPressed: () => ref.invalidate(nodeStatusStreamProvider),
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatusView(
    BuildContext context,
    WidgetRef ref,
    dynamic status,
  ) {
    return SingleChildScrollView(
      padding: UIConstants.paddingXL,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Center(child: RoleBadge(state: status.state)),
          const SizedBox(height: UIConstants.spacingXL),
          ConnectionIndicator(isActive: status.isActive),
          const SizedBox(height: UIConstants.spacingXL),
          NodeMetricsCard(
            nodeId: status.nodeId,
            clusterId: status.clusterId,
            state: status.state,
            isConfigured: status.isConfigured,
          ),
          const SizedBox(height: UIConstants.spacingXL),

          // Action tiles
          if (status.isActive) ...[
            ActionTile(
              icon: Icons.stop_circle_outlined,
              title: 'Shutdown Engine',
              isDestructive: true,
              onTap: () => _confirmShutdown(context, ref),
            ),
          ] else if (status.isConfigured) ...[
            ActionTile(
              icon: Icons.play_arrow,
              title: 'Start Engine',
              onTap: () => _startEngine(context, ref),
            ),
          ],
        ],
      ),
    );
  }

  void _confirmShutdown(BuildContext context, WidgetRef ref) {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Shutdown Engine?',
      message:
          'This will disconnect the node from the cluster. '
          'You can restart it later.',
      actionText: 'Shutdown',
      onActionPressed: () => _shutdownEngine(context, ref),
    );
  }

  Future<void> _shutdownEngine(BuildContext context, WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.shutdownEngine();
    ref.invalidate(nodeStatusStreamProvider);

    if (context.mounted) {
      NotificationToast.info(context, 'Engine shut down');
    }
  }

  Future<void> _startEngine(BuildContext context, WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.startEngine();
    ref.invalidate(nodeStatusStreamProvider);

    if (context.mounted) {
      NotificationToast.success(context, 'Engine started');
    }
  }
}
