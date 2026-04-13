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

    return FrostedScaffold(
      title: 'Node Status',
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.setting),
          onPressed: () => Navigator.pushNamed(context, AppRoutes.settings),
          tooltip: 'Settings',
        ),
      ],
      body: statusAsync.when(
        loading: () => _buildLoadingView(context),
        error: (error, stack) => _buildErrorView(context, ref, error),
        data: (status) => _buildStatusView(context, ref, status),
      ),
    );
  }

  Widget _buildLoadingView(BuildContext context) {
    final theme = Theme.of(context);

    return Center(
      child: Padding(
        padding: UIConstants.paddingXL,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            FrostedGlass(
              borderRadius: BorderRadius.circular(UIConstants.radiusCircle),
              padding: const EdgeInsets.all(24),
              child: Icon(
                IconlyBroken.time_circle,
                size: 40,
                color: theme.colorScheme.primary,
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),
            SizedBox(
              width: 200,
              child: ProgressBar(
                progress: 1.0,
                height: 6,
                progressColor: theme.colorScheme.primary,
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),
            Text(
              'Loading node status...',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildErrorView(
    BuildContext context,
    WidgetRef ref,
    Object error,
  ) {
    final theme = Theme.of(context);

    WidgetsBinding.instance.addPostFrameCallback((_) {
      NotificationToast.error(context, 'Failed to load node status');
    });

    return Center(
      child: Padding(
        padding: UIConstants.paddingXL,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            FrostedGlass(
              borderRadius: BorderRadius.circular(UIConstants.radiusCircle),
              tintColor: theme.colorScheme.error,
              opacity: 0.08,
              padding: const EdgeInsets.all(24),
              child: Icon(
                IconlyBold.danger,
                size: 40,
                color: theme.colorScheme.error,
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),
            Text(
              'Failed to load node status',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              error.toString(),
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            FrostedGlass(
              borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
              padding: EdgeInsets.zero,
              child: InkWell(
                borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                onTap: () => ref.invalidate(nodeStatusStreamProvider),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: UIConstants.spacingXL,
                    vertical: UIConstants.spacingMD,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        IconlyBroken.swap,
                        size: UIConstants.iconMD,
                        color: theme.colorScheme.primary,
                      ),
                      const SizedBox(width: UIConstants.spacingSM),
                      Text(
                        'Retry',
                        style: theme.textTheme.labelLarge?.copyWith(
                          color: theme.colorScheme.primary,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
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
    final theme = Theme.of(context);

    return SingleChildScrollView(
      padding: UIConstants.paddingXL,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Large centered role badge with glow
          Center(child: RoleBadge(state: status.state)),
          const SizedBox(height: UIConstants.spacingLG),

          // Connection pill
          ConnectionIndicator(isActive: status.isActive),
          const SizedBox(height: UIConstants.spacingXXL),

          // Metrics card
          NodeMetricsCard(
            nodeId: status.nodeId,
            clusterId: status.clusterId,
            state: status.state,
            isConfigured: status.isConfigured,
          ),
          const SizedBox(height: UIConstants.spacingXXL),

          // Action buttons
          if (status.isActive)
            _ActionButton(
              icon: IconlyBold.close_square,
              label: 'Shutdown Engine',
              color: theme.colorScheme.error,
              onTap: () => _confirmShutdown(context, ref),
            )
          else if (status.isConfigured)
            _ActionButton(
              icon: IconlyBold.play,
              label: 'Start Engine',
              color: Colors.green,
              onTap: () => _startEngine(context, ref),
            ),
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

/// A styled frosted glass action button with icon and label.
class _ActionButton extends StatelessWidget {
  final IconData icon;
  final String label;
  final Color color;
  final VoidCallback onTap;

  const _ActionButton({
    required this.icon,
    required this.label,
    required this.color,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedGlass(
      borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
      tintColor: color,
      opacity: 0.06,
      padding: EdgeInsets.zero,
      child: InkWell(
        borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: UIConstants.spacingXL,
            vertical: UIConstants.spacingLG,
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, color: color, size: UIConstants.iconLG),
              const SizedBox(width: UIConstants.spacingMD),
              Text(
                label,
                style: theme.textTheme.titleSmall?.copyWith(
                  color: color,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
