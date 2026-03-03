import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../config/router.dart';
import '../providers/engine_provider.dart';

class SettingsPage extends ConsumerWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final statusAsync = ref.watch(nodeStatusStreamProvider);

    return Scaffold(
      appBar: BlurredAppBar(
        title: 'Settings',
        centerTitle: true,
      ),
      body: statusAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, _) => Center(
          child: Padding(
            padding: UIConstants.paddingXL,
            child: Text(
              'Failed to load node info: $error',
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.center,
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
    final theme = Theme.of(context);

    return SingleChildScrollView(
      padding: UIConstants.paddingXL,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Node Info section
          Text(
            'Node Info',
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: UIConstants.spacingSM),
          Card(
            clipBehavior: Clip.antiAlias,
            child: Column(
              children: [
                ActionTile(
                  icon: Icons.fingerprint,
                  title: 'Node ID: ${_truncateId(status.nodeId)}',
                  onTap: () => _copyToClipboard(context, status.nodeId, 'Node ID'),
                ),
                const Divider(height: 1),
                ActionTile(
                  icon: Icons.cloud_outlined,
                  title: 'Cluster: ${status.clusterId ?? "Not joined"}',
                  onTap: () {
                    if (status.clusterId != null) {
                      _copyToClipboard(context, status.clusterId!, 'Cluster ID');
                    }
                  },
                ),
                const Divider(height: 1),
                ActionTile(
                  icon: Icons.circle,
                  title: 'State: ${status.state}',
                  onTap: () {},
                ),
              ],
            ),
          ),

          const SizedBox(height: UIConstants.spacingXL),

          // Actions section
          Text(
            'Actions',
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: UIConstants.spacingSM),
          Card(
            clipBehavior: Clip.antiAlias,
            child: Column(
              children: [
                if (status.isConfigured) ...[
                  ActionTile(
                    icon: Icons.logout,
                    title: 'Leave Cluster',
                    isDestructive: true,
                    onTap: () => _confirmLeaveCluster(context, ref),
                  ),
                  const Divider(height: 1),
                ],
                ActionTile(
                  icon: Icons.article_outlined,
                  title: 'View Logs',
                  onTap: () {
                    NotificationToast.info(context, 'Log viewer coming soon');
                  },
                ),
                const Divider(height: 1),
                ActionTile(
                  icon: Icons.restart_alt,
                  title: 'Restart Engine',
                  onTap: () => _confirmRestart(context, ref),
                ),
              ],
            ),
          ),

          const SizedBox(height: UIConstants.spacingXL),

          // About section
          Text(
            'About',
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: UIConstants.spacingSM),
          Card(
            clipBehavior: Clip.antiAlias,
            child: Column(
              children: [
                ActionTile(
                  icon: Icons.info_outline,
                  title: 'App Version: 0.1.0',
                  onTap: () {},
                ),
                const Divider(height: 1),
                ActionTile(
                  icon: Icons.memory,
                  title: 'Engine Version: 0.1.0',
                  onTap: () {},
                ),
              ],
            ),
          ),
        ],
      ),
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
    // Navigate back to welcome screen after leaving
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
