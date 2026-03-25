import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../core/di/injection.dart';
import '../../../../core/services/server_event.dart';
import '../../../../core/services/websocket_service.dart';
import '../../../auth/presentation/providers/auth_provider.dart';
import '../../../cluster/presentation/pages/dashboard_page.dart';
import '../../../cluster/presentation/providers/cluster_provider.dart';
import '../../../../config/router.dart';
import '../widgets/cluster_stats_card.dart';
import '../widgets/metrics_chart.dart';

class ShellPage extends ConsumerStatefulWidget {
  const ShellPage({super.key});

  @override
  ConsumerState<ShellPage> createState() => _ShellPageState();
}

class _ShellPageState extends ConsumerState<ShellPage> {
  int _selectedIndex = 0;

  static const _destinations = [
    NavigationDestination(
      icon: Icon(Icons.dashboard_outlined),
      selectedIcon: Icon(Icons.dashboard),
      label: 'Dashboard',
    ),
    NavigationDestination(
      icon: Icon(Icons.cloud_outlined),
      selectedIcon: Icon(Icons.cloud),
      label: 'Clusters',
    ),
    NavigationDestination(
      icon: Icon(Icons.settings_outlined),
      selectedIcon: Icon(Icons.settings),
      label: 'Settings',
    ),
  ];

  static const _railDestinations = [
    NavigationRailDestination(
      icon: Icon(Icons.dashboard_outlined),
      selectedIcon: Icon(Icons.dashboard),
      label: Text('Dashboard'),
    ),
    NavigationRailDestination(
      icon: Icon(Icons.cloud_outlined),
      selectedIcon: Icon(Icons.cloud),
      label: Text('Clusters'),
    ),
    NavigationRailDestination(
      icon: Icon(Icons.settings_outlined),
      selectedIcon: Icon(Icons.settings),
      label: Text('Settings'),
    ),
  ];

  Widget _buildPage() {
    switch (_selectedIndex) {
      case 0:
        return const _OverviewPage();
      case 1:
        return const DashboardPage();
      case 2:
        return const _SettingsPage();
      default:
        return const _OverviewPage();
    }
  }

  Future<void> _handleSignOut() async {
    await ref.read(authProvider.notifier).signOut();
    if (mounted) {
      Navigator.of(context).pushReplacementNamed(AppRoutes.login);
    }
  }

  @override
  Widget build(BuildContext context) {
    final isWide = MediaQuery.sizeOf(context).width > 800;

    if (isWide) {
      return Scaffold(
        body: Row(
          children: [
            NavigationRail(
              selectedIndex: _selectedIndex,
              onDestinationSelected: (index) =>
                  setState(() => _selectedIndex = index),
              labelType: NavigationRailLabelType.all,
              leading: Padding(
                padding: const EdgeInsets.symmetric(
                  vertical: UIConstants.spacingLG,
                ),
                child: Icon(
                  Icons.cloud_circle,
                  size: 40,
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
              trailing: Expanded(
                child: Align(
                  alignment: Alignment.bottomCenter,
                  child: Padding(
                    padding: const EdgeInsets.only(
                      bottom: UIConstants.spacingLG,
                    ),
                    child: IconButton(
                      onPressed: () => _confirmSignOut(context),
                      icon: const Icon(Icons.logout),
                      tooltip: 'Sign Out',
                    ),
                  ),
                ),
              ),
              destinations: _railDestinations,
            ),
            const VerticalDivider(width: 1, thickness: 1),
            Expanded(child: _buildPage()),
          ],
        ),
      );
    }

    return Scaffold(
      body: _buildPage(),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _selectedIndex,
        onDestinationSelected: (index) =>
            setState(() => _selectedIndex = index),
        destinations: _destinations,
      ),
    );
  }

  void _confirmSignOut(BuildContext context) {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Sign Out',
      message: 'Are you sure you want to sign out of NEBULA Admin?',
      actionText: 'Sign Out',
      onActionPressed: _handleSignOut,
    );
  }
}

/// Overview / Dashboard page with charts, stats, and real-time event feed.
class _OverviewPage extends ConsumerStatefulWidget {
  const _OverviewPage();

  @override
  ConsumerState<_OverviewPage> createState() => _OverviewPageState();
}

class _OverviewPageState extends ConsumerState<_OverviewPage> {
  final List<ServerEvent> _recentEvents = [];
  static const int _maxEvents = 20;

  @override
  void initState() {
    super.initState();
    // Ensure clusters and nodes are loaded.
    Future.microtask(() {
      ref.read(clustersProvider.notifier).loadClusters();
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final clustersState = ref.watch(clustersProvider);
    final nodesState = ref.watch(clusterNodesProvider);

    // Listen for real-time events.
    ref.listen<AsyncValue<ServerEvent>>(serverEventsProvider, (_, next) {
      next.whenData((event) {
        setState(() {
          _recentEvents.insert(0, event);
          if (_recentEvents.length > _maxEvents) {
            _recentEvents.removeLast();
          }
        });

        // Auto-refresh clusters on relevant events.
        if (event.type == ServerEvent.clusterCreated ||
            event.type == ServerEvent.clusterDeleted ||
            event.type == ServerEvent.nodeJoined ||
            event.type == ServerEvent.nodeLeft) {
          ref.read(clustersProvider.notifier).loadClusters();
        }
      });
    });

    return Scaffold(
      appBar: AppBar(title: const Text('Overview')),
      body: RefreshIndicator(
        onRefresh: () => ref.read(clustersProvider.notifier).loadClusters(),
        child: ListView(
          padding: UIConstants.paddingLG,
          children: [
            // Stats card
            ClusterStatsCard(
              clusters: clustersState.clusters,
              nodes: nodesState.nodes,
            ),
            const SizedBox(height: UIConstants.spacingLG),

            // Metrics chart
            MetricsChart(nodes: nodesState.nodes),
            const SizedBox(height: UIConstants.spacingLG),

            // Real-time event feed
            Text(
              'Recent Events',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            if (_recentEvents.isEmpty)
              Card(
                child: Padding(
                  padding: UIConstants.paddingLG,
                  child: Row(
                    children: [
                      Icon(
                        Icons.wifi_tethering,
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                      const SizedBox(width: UIConstants.spacingSM),
                      Expanded(
                        child: Text(
                          'Listening for real-time events...',
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              )
            else
              Card(
                clipBehavior: Clip.antiAlias,
                child: Column(
                  children: [
                    for (int i = 0; i < _recentEvents.length; i++) ...[
                      _EventTile(event: _recentEvents[i]),
                      if (i < _recentEvents.length - 1)
                        const Divider(height: 1),
                    ],
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _EventTile extends StatelessWidget {
  final ServerEvent event;

  const _EventTile({required this.event});

  IconData get _icon {
    switch (event.type) {
      case ServerEvent.nodeJoined:
        return Icons.add_circle_outline;
      case ServerEvent.nodeLeft:
        return Icons.remove_circle_outline;
      case ServerEvent.nodeStatusChanged:
        return Icons.swap_horiz;
      case ServerEvent.clusterCreated:
        return Icons.cloud;
      case ServerEvent.clusterDeleted:
        return Icons.cloud_off;
      case ServerEvent.metricsUpdate:
        return Icons.show_chart;
      default:
        return Icons.info_outline;
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final time =
        '${event.timestamp.hour.toString().padLeft(2, '0')}:${event.timestamp.minute.toString().padLeft(2, '0')}:${event.timestamp.second.toString().padLeft(2, '0')}';

    return ListTile(
      dense: true,
      leading: Icon(_icon, size: UIConstants.iconMD),
      title: Text(
        event.type.replaceAll('_', ' ').toUpperCase(),
        style: theme.textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w600),
      ),
      subtitle: Text(
        event.nodeId ?? event.clusterId ?? '',
        style: theme.textTheme.labelSmall,
      ),
      trailing: Text(time, style: theme.textTheme.labelSmall),
    );
  }
}

/// Real Settings page with server configuration, sign out, and app info.
class _SettingsPage extends ConsumerStatefulWidget {
  const _SettingsPage();

  @override
  ConsumerState<_SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends ConsumerState<_SettingsPage> {
  late TextEditingController _serverUrlController;

  @override
  void initState() {
    super.initState();
    _serverUrlController = TextEditingController();
  }

  @override
  void dispose() {
    _serverUrlController.dispose();
    super.dispose();
  }

  Future<void> _handleSignOut() async {
    await ref.read(authProvider.notifier).signOut();
    if (mounted) {
      Navigator.of(context).pushReplacementNamed(AppRoutes.login);
    }
  }

  void _confirmSignOut() {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Sign Out',
      message: 'Are you sure you want to sign out of NEBULA Admin?',
      actionText: 'Sign Out',
      onActionPressed: _handleSignOut,
    );
  }

  void _editServerUrl() {
    final currentUrl = ref.read(serverUrlProvider);
    AppAlertDialog.showWithInput(
      context: context,
      title: 'Server URL',
      message: 'Enter the nebula-server base URL.',
      actionText: 'Save',
      hintText: 'http://localhost:8080',
      initialValue: currentUrl,
      validator: (value) {
        if (value.isEmpty) return 'URL is required';
        final uri = Uri.tryParse(value);
        if (uri == null || !uri.hasScheme) return 'Enter a valid URL';
        return null;
      },
      onActionPressed: (value) {
        ref.read(serverUrlProvider.notifier).state = value;
        // Persist to Hive so it survives restart.
        ref.read(localStorageProvider).setServerUrl(value);
        NotificationToast.success(context, 'Server URL updated');
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final currentServerUrl = ref.watch(serverUrlProvider);
    final authState = ref.watch(authProvider);

    return Scaffold(
      appBar: BlurredAppBar(title: 'Settings'),
      body: ListView(
        children: [
          const SizedBox(height: UIConstants.spacingMD),

          // --- Account section ---
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
            child: Text(
              'Account',
              style: theme.textTheme.titleSmall?.copyWith(
                color: theme.colorScheme.primary,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          if (authState.user != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
              child: Row(
                children: [
                  CircleAvatar(
                    radius: 20,
                    backgroundColor: theme.colorScheme.primaryContainer,
                    child: Text(
                      (authState.user!.displayName ?? authState.user!.email)
                          .substring(0, 1)
                          .toUpperCase(),
                      style: TextStyle(
                        color: theme.colorScheme.onPrimaryContainer,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          authState.user!.displayName ?? 'NEBULA User',
                          style: theme.textTheme.titleMedium,
                        ),
                        Text(
                          authState.user!.email,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurfaceVariant,
                          ),
                        ),
                        if (authState.user!.role != 'viewer')
                          Padding(
                            padding: const EdgeInsets.only(
                              top: UIConstants.spacingXS,
                            ),
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: UIConstants.spacingSM,
                                vertical: 2,
                              ),
                              decoration: BoxDecoration(
                                color: theme.colorScheme.primaryContainer,
                                borderRadius: BorderRadius.circular(
                                  UIConstants.radiusSmall,
                                ),
                              ),
                              child: Text(
                                authState.user!.role.toUpperCase(),
                                style: theme.textTheme.labelSmall?.copyWith(
                                  color: theme.colorScheme.onPrimaryContainer,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ActionTile(
            icon: Icons.logout,
            title: 'Sign Out',
            isDestructive: true,
            onTap: _confirmSignOut,
          ),

          const Divider(height: 32),

          // --- Server section ---
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
            child: Text(
              'Server',
              style: theme.textTheme.titleSmall?.copyWith(
                color: theme.colorScheme.primary,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          ActionTile(
            icon: Icons.dns_outlined,
            title: 'Server URL',
            onTap: _editServerUrl,
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 64),
            child: Text(
              currentServerUrl,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),

          const Divider(height: 32),

          // --- About section ---
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 8),
            child: Text(
              'About',
              style: theme.textTheme.titleSmall?.copyWith(
                color: theme.colorScheme.primary,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          ActionTile(
            icon: Icons.info_outline,
            title: 'App Version',
            onTap: () {
              AppAlertDialog.showInfo(
                context: context,
                title: 'NEBULA Admin',
                message:
                    'Version 0.1.0+1\nDistributed compute cluster dashboard.',
              );
            },
          ),
        ],
      ),
    );
  }
}
