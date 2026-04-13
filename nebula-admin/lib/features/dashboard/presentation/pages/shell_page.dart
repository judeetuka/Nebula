import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../core/di/injection.dart';
import '../../../../core/services/server_event.dart';
import '../../../../core/services/websocket_service.dart';
import '../../../auth/presentation/providers/auth_provider.dart';
import '../../../cluster/presentation/pages/dashboard_page.dart';
import '../../../cluster/presentation/providers/cluster_provider.dart';
import '../../../workflow/presentation/pages/workflow_list_page.dart';
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
  final _scrollController = ScrollController();
  late final HideOnScrollController _hideController;
  final _navToast = NavToastController();
  bool _navVisible = true;

  @override
  void initState() {
    super.initState();
    _hideController = HideOnScrollController(_scrollController);
    _hideController.addListener(() {
      if (mounted) setState(() => _navVisible = _hideController.visible);
    });
    _navToast.scrollController = _hideController;
    // Wire the global nav toast so NotificationToast.show(useNav: true) works
    NotificationToast.navToastController = _navToast;
  }

  @override
  void dispose() {
    _hideController.dispose();
    _navToast.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  Widget _buildPage() {
    switch (_selectedIndex) {
      case 0:
        return _OverviewPage(scrollController: _scrollController);
      case 1:
        return DashboardPage(scrollController: _scrollController);
      case 2:
        return WorkflowListPage(scrollController: _scrollController);
      case 3:
        return _SettingsPage(scrollController: _scrollController);
      default:
        return _OverviewPage(scrollController: _scrollController);
    }
  }

  Future<void> _handleSignOut() async {
    await ref.read(authProvider.notifier).signOut();
    if (mounted) {
      Navigator.of(context).pushReplacementNamed(AppRoutes.login);
    }
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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final primaryColor = theme.colorScheme.primary;
    final isWide = MediaQuery.sizeOf(context).width > 800;

    if (isWide) {
      return Scaffold(
        body: Row(
          children: [
            FrostedGlass(
              borderRadius: BorderRadius.zero,
              child: NavigationRail(
                backgroundColor: Colors.transparent,
                selectedIndex: _selectedIndex,
                onDestinationSelected: (index) =>
                    setState(() => _selectedIndex = index),
                labelType: NavigationRailLabelType.all,
                indicatorColor: primaryColor.withValues(alpha: 0.15),
                selectedIconTheme: IconThemeData(color: primaryColor),
                selectedLabelTextStyle: TextStyle(
                  color: primaryColor,
                  fontWeight: FontWeight.w600,
                  fontSize: 12,
                ),
                leading: Padding(
                  padding: const EdgeInsets.symmetric(
                    vertical: UIConstants.spacingLG,
                  ),
                  child: Icon(
                    IconlyBold.discovery,
                    size: 40,
                    color: primaryColor,
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
                        icon: const Icon(IconlyBroken.logout),
                        tooltip: 'Sign Out',
                      ),
                    ),
                  ),
                ),
                destinations: const [
                  NavigationRailDestination(
                    icon: Icon(IconlyBroken.home),
                    selectedIcon: Icon(IconlyBold.home),
                    label: Text('Dashboard'),
                  ),
                  NavigationRailDestination(
                    icon: Icon(IconlyBroken.discovery),
                    selectedIcon: Icon(IconlyBold.discovery),
                    label: Text('Clusters'),
                  ),
                  NavigationRailDestination(
                    icon: Icon(IconlyBroken.activity),
                    selectedIcon: Icon(IconlyBold.activity),
                    label: Text('Workflows'),
                  ),
                  NavigationRailDestination(
                    icon: Icon(IconlyBroken.setting),
                    selectedIcon: Icon(IconlyBold.setting),
                    label: Text('Settings'),
                  ),
                ],
              ),
            ),
            const VerticalDivider(width: 1, thickness: 1),
            Expanded(child: _buildPage()),
          ],
        ),
      );
    }

    return Scaffold(
      extendBody: true,
      body: _buildPage(),
      bottomNavigationBar: NavigationView(
        useTooltip: false,
        floating: true,
        floatingWidthFactor: 0.82,
        floatingMarginBottom: 18,
        visible: _navVisible,
        toastController: _navToast,
        onChangePage: (i) => setState(() => _selectedIndex = i),
        selectedIndex: _selectedIndex,
        curve: Curves.fastLinearToSlowEaseIn,
        durationAnimation: const Duration(milliseconds: 500),
        backgroundColor: theme.scaffoldBackgroundColor,
        color: primaryColor,
        enableGlassmorphism: true,
        items: [
          ItemNavigationView(
            iconBefore: Icon(
              IconlyBroken.home,
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
            iconAfter: Icon(IconlyBold.home, color: primaryColor),
            tooltip: 'Dashboard',
          ),
          ItemNavigationView(
            iconBefore: Icon(
              IconlyBroken.discovery,
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
            iconAfter: Icon(IconlyBold.discovery, color: primaryColor),
            tooltip: 'Clusters',
          ),
          ItemNavigationView(
            iconBefore: Icon(
              IconlyBroken.activity,
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
            iconAfter: Icon(IconlyBold.activity, color: primaryColor),
            tooltip: 'Workflows',
          ),
          ItemNavigationView(
            iconBefore: Icon(
              IconlyBroken.setting,
              color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
            ),
            iconAfter: Icon(IconlyBold.setting, color: primaryColor),
            tooltip: 'Settings',
          ),
        ],
      ),
    );
  }
}

/// Overview / Dashboard page with charts, stats, and real-time event feed.
class _OverviewPage extends ConsumerStatefulWidget {
  const _OverviewPage({this.scrollController});
  final ScrollController? scrollController;

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

    return FrostedScaffold(
      title: 'Dashboard',
      actions: [
        IconButton(
          icon: Icon(
            theme.brightness == Brightness.dark
                ? Icons.light_mode_outlined
                : Icons.dark_mode_outlined,
          ),
          onPressed: () {
            // Theme toggle not available at widget level — placeholder
          },
        ),
        IconButton(
          icon: const Icon(IconlyBroken.setting),
          onPressed: () {
            OptionsMenu.show(
              context: context,
              title: 'Quick Settings',
              options: [
                MenuOption(
                  icon: IconlyBroken.profile,
                  label: 'Account',
                  subtitle: 'Manage your profile',
                  onTap: () {},
                ),
                MenuOption(
                  icon: IconlyBroken.shield_done,
                  label: 'Security',
                  subtitle: 'Cluster encryption',
                  onTap: () {},
                ),
                MenuOption(
                  icon: IconlyBroken.logout,
                  label: 'Sign Out',
                  isDestructive: true,
                  onTap: () {},
                ),
              ],
            );
          },
        ),
      ],
      body: RefreshIndicator(
        onRefresh: () => ref.read(clustersProvider.notifier).loadClusters(),
        child: ListView(
          controller: widget.scrollController,
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

            // Task deployment tracker
            FrostedGlass(
              padding: UIConstants.paddingLG,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Task Deployment',
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  const SizedBox(height: 16),
                  StepTracker(
                    steps: [
                      TrackerStep(
                        label: 'Created',
                        description: 'Task queued by master node',
                        icon: IconlyBroken.paper,
                        status: StepStatus.completed,
                        timestamp: '10:42 AM',
                      ),
                      TrackerStep(
                        label: 'Distributed',
                        description: 'Sent to worker nodes via MQTT',
                        icon: IconlyBroken.send,
                        status: StepStatus.completed,
                        timestamp: '10:42 AM',
                      ),
                      TrackerStep(
                        label: 'Executing',
                        description: 'Workers processing task payload',
                        icon: IconlyBroken.activity,
                        status: StepStatus.active,
                        timestamp: '10:43 AM',
                      ),
                      TrackerStep(
                        label: 'Completed',
                        description: 'Results aggregated and reported',
                        icon: IconlyBroken.shield_done,
                        status: StepStatus.pending,
                      ),
                    ],
                    activeStepInfo: FrostedGlass(
                      borderRadius: BorderRadius.circular(8),
                      padding: const EdgeInsets.all(10),
                      shadow: false,
                      tintColor: Colors.blue,
                      opacity: 0.1,
                      child: Row(
                        children: [
                          const Icon(
                            IconlyBroken.time_circle,
                            size: 16,
                            color: Colors.blue,
                          ),
                          const SizedBox(width: 8),
                          Text(
                            'Est. ~45s remaining',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: Colors.blue,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),

            // Real-time event feed
            FrostedGlass(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
                    child: Text(
                      'Recent Events',
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  if (_recentEvents.isEmpty)
                    Padding(
                      padding: UIConstants.paddingLG,
                      child: Row(
                        children: [
                          Icon(
                            IconlyBroken.notification,
                            color: theme.colorScheme.onSurface.withValues(
                              alpha: 0.5,
                            ),
                          ),
                          const SizedBox(width: UIConstants.spacingSM),
                          Expanded(
                            child: Text(
                              'Listening for real-time events...',
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.colorScheme.onSurface.withValues(
                                  alpha: 0.5,
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    )
                  else
                    for (final event in _recentEvents) _EventTile(event: event),
                  const SizedBox(height: 8),
                ],
              ),
            ),

            // Bottom spacer for floating nav
            const SizedBox(height: 80),
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
        return IconlyBroken.add_user;
      case ServerEvent.nodeLeft:
        return IconlyBroken.delete;
      case ServerEvent.nodeStatusChanged:
        return IconlyBroken.swap;
      case ServerEvent.clusterCreated:
        return IconlyBroken.discovery;
      case ServerEvent.clusterDeleted:
        return IconlyBroken.close_square;
      case ServerEvent.metricsUpdate:
        return IconlyBroken.chart;
      default:
        return IconlyBroken.info_circle;
    }
  }

  @override
  Widget build(BuildContext context) {
    return ActionTile(
      icon: _icon,
      title: event.type.replaceAll('_', ' ').toUpperCase(),
      onTap: () {},
    );
  }
}

/// Real Settings page with server configuration, sign out, and app info.
class _SettingsPage extends ConsumerStatefulWidget {
  const _SettingsPage({this.scrollController});
  final ScrollController? scrollController;

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

    return FrostedScaffold(
      title: 'Settings',
      body: ListView(
        controller: widget.scrollController,
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
            FrostedGlass(
              padding: UIConstants.paddingLG,
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
                            color: theme.colorScheme.onSurface.withValues(
                              alpha: 0.6,
                            ),
                          ),
                        ),
                        if (authState.user!.role != 'viewer')
                          Padding(
                            padding: const EdgeInsets.only(
                              top: UIConstants.spacingXS,
                            ),
                            child: FrostedGlass(
                              borderRadius: BorderRadius.circular(
                                UIConstants.radiusSmall,
                              ),
                              padding: const EdgeInsets.symmetric(
                                horizontal: UIConstants.spacingSM,
                                vertical: 2,
                              ),
                              shadow: false,
                              tintColor: theme.colorScheme.primary,
                              opacity: 0.15,
                              child: Text(
                                authState.user!.role.toUpperCase(),
                                style: theme.textTheme.labelSmall?.copyWith(
                                  color: theme.colorScheme.primary,
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
          const SizedBox(height: UIConstants.spacingSM),
          ActionTile(
            icon: IconlyBroken.logout,
            title: 'Sign Out',
            isDestructive: true,
            onTap: _confirmSignOut,
          ),

          const SizedBox(height: UIConstants.spacingLG),

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
            icon: IconlyBroken.shield_done,
            title: 'Server URL',
            onTap: _editServerUrl,
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 64),
            child: Text(
              currentServerUrl,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
              ),
            ),
          ),

          const SizedBox(height: UIConstants.spacingLG),

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
            icon: IconlyBroken.info_circle,
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

          // Bottom spacer for floating nav
          const SizedBox(height: 80),
        ],
      ),
    );
  }
}
