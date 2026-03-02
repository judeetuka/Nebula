import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../providers/engine_provider.dart';
import '../widgets/connection_indicator.dart';
import '../widgets/node_metrics_card.dart';
import '../widgets/role_badge.dart';

class StatusPage extends ConsumerWidget {
  const StatusPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final statusAsync = ref.watch(nodeStatusProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Node Status'),
        centerTitle: true,
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () => ref.invalidate(nodeStatusProvider),
            tooltip: 'Refresh status',
          ),
        ],
      ),
      body: statusAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, stack) => Center(
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
                  onPressed: () => ref.invalidate(nodeStatusProvider),
                  icon: const Icon(Icons.refresh),
                  label: const Text('Retry'),
                ),
              ],
            ),
          ),
        ),
        data: (status) => SingleChildScrollView(
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
              const SizedBox(height: UIConstants.spacingXXL),
              if (status.isActive)
                FilledButton.tonalIcon(
                  onPressed: () => _shutdownEngine(ref),
                  icon: const Icon(Icons.stop_circle_outlined),
                  label: const Text('Shutdown Engine'),
                )
              else if (status.isConfigured)
                FilledButton.icon(
                  onPressed: () => _startEngine(ref),
                  icon: const Icon(Icons.play_arrow),
                  label: const Text('Start Engine'),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _shutdownEngine(WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.shutdownEngine();
    ref.invalidate(nodeStatusProvider);
  }

  Future<void> _startEngine(WidgetRef ref) async {
    final repository = ref.read(engineRepositoryProvider);
    await repository.startEngine();
    ref.invalidate(nodeStatusProvider);
  }
}
