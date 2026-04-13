import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/engine_provider.dart';

class SettingsPage extends ConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final statusAsync = ref.watch(nodeStatusStreamProvider);

    return FrostedScaffold(
      title: 'Settings',
      body: statusAsync.when(
        loading: () => const Center(
          child: Padding(
            padding: UIConstants.paddingXL,
            child: ProgressBar(progress: 1.0, height: 6),
          ),
        ),
        error: (error, _) => Center(
          child: Padding(
            padding: UIConstants.paddingXL,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  IconlyBold.danger,
                  size: 40,
                  color: Theme.of(context).colorScheme.error,
                ),
                const SizedBox(height: UIConstants.spacingLG),
                Text(
                  'Failed to load node info',
                  style: Theme.of(context).textTheme.titleMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: UIConstants.spacingSM),
                Text(
                  error.toString(),
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                  textAlign: TextAlign.center,
                ),
              ],
            ),
          ),
        ),
        data: (status) => _buildSettings(context, ref, status),
      ),
    );
  }

  Widget _buildSettings(
    BuildContext context,
    WidgetRef ref,
    dynamic status,
  ) {
    return SingleChildScrollView(
      padding: UIConstants.paddingXL,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // -- Node Information section --
          _SectionHeader(
            icon: IconlyBold.info_circle,
            label: 'Node Information',
          ),
          const SizedBox(height: UIConstants.spacingMD),
          FrostedGlass(
            borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              children: [
                ActionTile(
                  icon: IconlyBroken.document,
                  title: 'Node ID: ${_truncateId(status.nodeId)}',
                  onTap: () =>
                      _copyToClipboard(context, status.nodeId, 'Node ID'),
                ),
                _thinDivider(context),
                ActionTile(
                  icon: IconlyBroken.discovery,
                  title: 'Cluster: ${status.clusterId ?? "Not joined"}',
                  onTap: () {
                    if (status.clusterId != null) {
                      _copyToClipboard(
                          context, status.clusterId!, 'Cluster ID');
                    }
                  },
                ),
                _thinDivider(context),
                ActionTile(
                  icon: IconlyBroken.activity,
                  title: 'State: ${status.state}',
                  onTap: () {},
                ),
              ],
            ),
          ),

          const SizedBox(height: UIConstants.spacingXXL),

          // -- Actions section --
          _SectionHeader(
            icon: IconlyBold.category,
            label: 'Actions',
          ),
          const SizedBox(height: UIConstants.spacingMD),
          FrostedGlass(
            borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              children: [
                if (status.isConfigured) ...[
                  ActionTile(
                    icon: IconlyBroken.logout,
                    title: 'Leave Cluster',
                    isDestructive: true,
                    onTap: () => _confirmLeaveCluster(context, ref),
                  ),
                  _thinDivider(context),
                ],
                ActionTile(
                  icon: IconlyBroken.paper,
                  title: 'View Logs',
                  onTap: () {
                    NotificationToast.info(context, 'Log viewer coming soon');
                  },
                ),
                _thinDivider(context),
                ActionTile(
                  icon: IconlyBroken.swap,
                  title: 'Restart Engine',
                  onTap: () => _confirmRestart(context, ref),
                ),
              ],
            ),
          ),

          const SizedBox(height: UIConstants.spacingXXL),

          // -- About section --
          _SectionHeader(
            icon: IconlyBold.bookmark,
            label: 'About',
          ),
          const SizedBox(height: UIConstants.spacingMD),
          FrostedGlass(
            borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              children: [
                ActionTile(
                  icon: IconlyBroken.info_circle,
                  title: 'App Version: 0.1.0',
                  onTap: () {},
                ),
                _thinDivider(context),
                ActionTile(
                  icon: IconlyBroken.chart,
                  title: 'Engine Version: 0.1.0',
                  onTap: () {},
                ),
              ],
            ),
          ),

          const SizedBox(height: UIConstants.spacingXXL),
        ],
      ),
    );
  }

  Widget _thinDivider(BuildContext context) {
    return Divider(
      height: 1,
      thickness: 0.5,
      indent: UIConstants.spacingXL,
      endIndent: UIConstants.spacingXL,
      color: Theme.of(context).colorScheme.outlineVariant.withValues(alpha: 0.3),
    );
  }

  void _copyToClipboard(BuildContext context, String text, String label) {
    Clipboard.setData(ClipboardData(text: text));
    NotificationToast.success(context, '$label copied to clipboard');
  }

  String _truncateId(String id) {
    if (id.length <= 16) return id;
    return '${id.substring(0, 8)}...${id.substring(id.length - 8)}';
  }

  void _confirmLeaveCluster(BuildContext context, WidgetRef ref) {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Leave Cluster?',
      message:
          'This will disconnect the node from the cluster and clear all configuration. '
          'You will need to scan a new QR code to rejoin.',
      actionText: 'Leave',
      onActionPressed: () => _leaveCluster(context, ref),
    );
  }

  Future<void> _leaveCluster(BuildContext context, WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.shutdownEngine();
    if (context.mounted) {
      NotificationToast.info(context, 'Left cluster');
      Navigator.of(context).pushNamedAndRemoveUntil(
        AppRoutes.welcome,
        (_) => false,
      );
    }
  }

  void _confirmRestart(BuildContext context, WidgetRef ref) {
    AppAlertDialog.show(
      context: context,
      title: 'Restart Engine?',
      message: 'The engine will shut down and start again. '
          'This may briefly disconnect the node from the cluster.',
      actionText: 'Restart',
      onActionPressed: () => _restartEngine(context, ref),
    );
  }

  Future<void> _restartEngine(BuildContext context, WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.shutdownEngine();
    await repository.startEngine();
    ref.invalidate(nodeStatusStreamProvider);

    if (context.mounted) {
      NotificationToast.success(context, 'Engine restarted');
    }
  }
}

/// Reusable section header with icon and label.
class _SectionHeader extends StatelessWidget {
  final IconData icon;
  final String label;

  const _SectionHeader({required this.icon, required this.label});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Row(
      children: [
        Icon(
          icon,
          size: UIConstants.iconMD,
          color: theme.colorScheme.primary,
        ),
        const SizedBox(width: UIConstants.spacingSM),
        Text(
          label,
          style: theme.textTheme.titleSmall?.copyWith(
            fontWeight: FontWeight.bold,
            color: theme.colorScheme.primary,
          ),
        ),
      ],
    );
  }
}
