import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../core/di/injection.dart';
import '../../../../main.dart';
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
  int _currentPage = 0;
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

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final primaryColor = theme.colorScheme.primary;

    final pages = [
      _OverviewPage(scrollController: _scrollController),
      DashboardPage(scrollController: _scrollController),
      WorkflowListPage(scrollController: _scrollController),
      _NotificationsPage(scrollController: _scrollController),
    ];

    final isMobileNav = context.isMobile;

    final navItems = [
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
          IconlyBroken.notification,
          color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
        ),
        iconAfter: Icon(IconlyBold.notification, color: primaryColor),
        tooltip: 'Notifications',
      ),
    ];

    // Mobile: bottom floating navbar
    if (isMobileNav) {
      return Scaffold(
        extendBody: true,
        body: pages[_currentPage],
        bottomNavigationBar: NavigationView(
          useTooltip: false,
          floating: true,
          floatingWidthFactor: 0.88,
          floatingMarginBottom: 18,
          visible: _navVisible,
          toastController: _navToast,
          onChangePage: (i) => setState(() => _currentPage = i),
          selectedIndex: _currentPage,
          curve: Curves.fastLinearToSlowEaseIn,
          durationAnimation: const Duration(milliseconds: 500),
          backgroundColor: theme.scaffoldBackgroundColor,
          color: primaryColor,
          enableGlassmorphism: true,
          items: navItems,
        ),
      );
    }

    // Tablet+: vertical floating pill overlaying content on the left
    return Scaffold(
      body: Stack(
        children: [
          // Full-width content — fills the entire screen
          Positioned.fill(child: pages[_currentPage]),
          // Floating vertical navbar on top of content
          Positioned(
            left: 0,
            top: 0,
            bottom: 0,
            child: NavigationView(
              floating: true,
              vertical: true,
              floatingMarginLeft: 12,
              visible: _navVisible,
              toastController: _navToast,
              onChangePage: (i) => setState(() => _currentPage = i),
              selectedIndex: _currentPage,
              curve: Curves.fastLinearToSlowEaseIn,
              durationAnimation: const Duration(milliseconds: 500),
              backgroundColor: theme.scaffoldBackgroundColor,
              color: primaryColor,
              enableGlassmorphism: true,
              items: navItems,
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Section header helper
// ---------------------------------------------------------------------------

/// Consistent section header with icon and title.
class _SectionHeader extends StatelessWidget {
  final IconData icon;
  final String title;
  final Widget? trailing;

  const _SectionHeader({
    required this.icon,
    required this.title,
    this.trailing,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final effectiveColor = theme.colorScheme.primary;

    return Padding(
      padding: const EdgeInsets.only(bottom: UIConstants.spacingLG),
      child: Row(
        children: [
          Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              color: effectiveColor.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
            ),
            child: Icon(icon, color: effectiveColor, size: UIConstants.iconMD),
          ),
          const SizedBox(width: UIConstants.spacingMD),
          Expanded(
            child: Text(
              title,
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Overview / Dashboard page
// ---------------------------------------------------------------------------

/// Overview / Dashboard page with welcome banner, metric cards, gauges,
/// performance chart, deployment tracker, and recent events feed.
class _OverviewPage extends ConsumerStatefulWidget {
  const _OverviewPage({this.scrollController});
  final ScrollController? scrollController;

  @override
  ConsumerState<_OverviewPage> createState() => _OverviewPageState();
}

class _OverviewPageState extends ConsumerState<_OverviewPage> {
  Timer? _autoRefreshTimer;

  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      ref.read(clustersProvider.notifier).loadClusters();
    });
    // Auto-refresh every 30s — simple, battery-friendly, works everywhere.
    // Live mode (WebSocket) is opt-in via settings for wall-display use cases.
    _autoRefreshTimer = Timer.periodic(const Duration(seconds: 30), (_) {
      if (mounted) ref.read(clustersProvider.notifier).loadClusters();
    });
  }

  @override
  void dispose() {
    _autoRefreshTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final clustersState = ref.watch(clustersProvider);
    final nodesState = ref.watch(clusterNodesProvider);

    final isDark = theme.brightness == Brightness.dark;

    return FrostedScaffold(
      title: 'Dashboard',
      automaticallyImplyLeading: false,
      bodyPadding: EdgeInsets.only(
        left: context.isMobile ? 16 : 88,
        right: 16,
      ),
      actions: [
        IconButton(
          icon: Icon(isDark ? Icons.light_mode_outlined : Icons.dark_mode_outlined),
          tooltip: 'Toggle theme',
          onPressed: () => NebulaAdminApp.of(context).toggleTheme(),
        ),
        IconButton(
          icon: const Icon(IconlyBroken.setting),
          tooltip: 'Settings',
          onPressed: () => _showSettingsMenu(context, ref),
        ),
      ],
      body: RefreshIndicator(
        onRefresh: () => ref.read(clustersProvider.notifier).loadClusters(),
        child: ListView(
          controller: widget.scrollController,
          children: [
            // ================================================================
            // 1. WELCOME BANNER
            // ================================================================
            _WelcomeBanner(
              clusterCount: clustersState.clusters.length,
              nodeCount: nodesState.nodes.length,
            ),

            const SizedBox(height: UIConstants.spacingXXL),

            // ================================================================
            // 2. METRIC CARDS + GAUGES
            // ================================================================
            _SectionHeader(
              icon: IconlyBold.graph,
              title: 'Fleet Status',
              trailing: FrostedGlass(
                borderRadius: BorderRadius.circular(20),
                padding:
                    const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                shadow: false,
                tintColor: Colors.green,
                opacity: 0.1,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 6,
                      height: 6,
                      decoration: const BoxDecoration(
                        color: Colors.green,
                        shape: BoxShape.circle,
                      ),
                    ),
                    const SizedBox(width: 6),
                    Text(
                      'Live',
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: Colors.green,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ],
                ),
              ),
            ),
            ClusterStatsCard(
              clusters: clustersState.clusters,
              nodes: nodesState.nodes,
            ),

            const SizedBox(height: UIConstants.spacingXXL),

            // ================================================================
            // 3. PERFORMANCE CHART
            // ================================================================
            _SectionHeader(
              icon: IconlyBold.chart,
              title: 'Performance Metrics',
            ),
            MetricsChart(nodes: nodesState.nodes),

            const SizedBox(height: UIConstants.spacingXXL),

            // ================================================================
            // 4. BOTTOM ROW: Deployment Tracker + Recent Events
            // ================================================================
            LayoutBuilder(
              builder: (context, constraints) {
                final isWide = constraints.maxWidth >= 600;

                final deploymentTracker = _buildDeploymentTracker(theme);
                final recentEvents = _buildRecentEvents(theme);

                if (isWide) {
                  return Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(child: deploymentTracker),
                      const SizedBox(width: UIConstants.spacingLG),
                      Expanded(child: recentEvents),
                    ],
                  );
                }

                return Column(
                  children: [
                    deploymentTracker,
                    const SizedBox(height: UIConstants.spacingXXL),
                    recentEvents,
                  ],
                );
              },
            ),

            // Bottom spacer for floating nav
            const SizedBox(height: 100),
          ],
        ),
      ),
    );
  }

  // ---------- Settings Menu (modal) ----------

  void _showSettingsMenu(BuildContext context, WidgetRef ref) {
    final authState = ref.read(authProvider);
    final currentUrl = ref.read(serverUrlProvider);

    OptionsMenu.show(
      context: context,
      title: 'Settings',
      options: [
        if (authState.user != null)
          MenuOption(
            icon: IconlyBroken.profile,
            label: authState.user!.displayName ?? 'Account',
            subtitle: authState.user!.email,
            onTap: () {
              AppAlertDialog.showWithInput(
                context: context,
                title: 'Edit Display Name',
                message: 'Update your display name.',
                actionText: 'Save',
                hintText: 'Enter display name',
                initialValue: authState.user!.displayName ?? '',
                validator: (value) {
                  if (value.trim().isEmpty) return 'Name is required';
                  return null;
                },
                onActionPressed: (value) {
                  NotificationToast.success(context, 'Display name updated');
                },
              );
            },
          ),
        MenuOption(
          icon: IconlyBroken.shield_done,
          label: 'Server URL',
          subtitle: currentUrl,
          onTap: () {
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
                ref.read(localStorageProvider).setServerUrl(value);
                NotificationToast.success(context, 'Server URL updated');
              },
            );
          },
        ),
        MenuOption(
          icon: IconlyBroken.info_circle,
          label: 'About',
          subtitle: 'NEBULA Admin v0.1.0',
          onTap: () {
            AppAlertDialog.showInfo(
              context: context,
              title: 'NEBULA Admin',
              message: 'Version 0.1.0+1\nDistributed compute cluster dashboard.',
            );
          },
        ),
        MenuOption(
          icon: IconlyBroken.logout,
          label: 'Sign Out',
          isDestructive: true,
          onTap: () {
            AppAlertDialog.showDanger(
              context: context,
              title: 'Sign Out',
              message: 'Are you sure you want to sign out?',
              actionText: 'Sign Out',
              onActionPressed: () async {
                await ref.read(authProvider.notifier).signOut();
                if (context.mounted) {
                  Navigator.of(context).pushReplacementNamed(AppRoutes.login);
                }
              },
            );
          },
        ),
      ],
    );
  }

  // ---------- Deployment Tracker ----------

  Widget _buildDeploymentTracker(ThemeData theme) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(
          icon: IconlyBold.send,
          title: 'Task Deployment',
        ),
        FrostedGlass(
          padding: const EdgeInsets.all(20),
          child: StepTracker(
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
              borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
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
        ),
      ],
    );
  }

  // ---------- Recent Events ----------

  Widget _buildRecentEvents(ThemeData theme) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(
          icon: IconlyBold.notification,
          title: 'Recent Events',
        ),
        FrostedGlass(
          padding: const EdgeInsets.all(20),
          child: Column(
            children: [
              _EventItem(
                title: 'Node alpha-7 came online',
                subtitle: 'Joined cluster East-1 via mDNS',
                timestamp: '2m ago',
                type: _EventType.success,
              ),
              const Divider(height: 24),
              _EventItem(
                title: 'High CPU alert on bravo-3',
                subtitle: 'CPU load exceeded 90% threshold',
                timestamp: '8m ago',
                type: _EventType.warning,
              ),
              const Divider(height: 24),
              _EventItem(
                title: 'Tunnel reconnected',
                subtitle: 'Rathole proxy re-established to us-east',
                timestamp: '14m ago',
                type: _EventType.info,
              ),
              const Divider(height: 24),
              _EventItem(
                title: 'Task batch #47 failed',
                subtitle: '3 of 12 workers reported timeout',
                timestamp: '22m ago',
                type: _EventType.error,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Welcome Banner
// ---------------------------------------------------------------------------

class _WelcomeBanner extends StatelessWidget {
  final int clusterCount;
  final int nodeCount;

  const _WelcomeBanner({
    required this.clusterCount,
    required this.nodeCount,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final hour = DateTime.now().hour;
    final greeting = hour < 12
        ? 'Good morning'
        : hour < 17
            ? 'Good afternoon'
            : 'Good evening';

    return FrostedGlass(
      tintColor: theme.colorScheme.primary,
      opacity: 0.08,
      padding: const EdgeInsets.all(24),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '$greeting, Commander',
                  style: theme.textTheme.headlineSmall?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  clusterCount > 0
                      ? '$clusterCount cluster${clusterCount == 1 ? '' : 's'} running $nodeCount nodes. Fleet at a glance.'
                      : 'Your cluster fleet at a glance. Connect nodes to get started.',
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurface
                        .withValues(alpha: 0.6),
                    height: 1.4,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: UIConstants.spacingLG),
          Container(
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: theme.colorScheme.primary.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
            ),
            child: Icon(
              IconlyBold.star,
              color: theme.colorScheme.primary,
              size: 28,
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Event Item
// ---------------------------------------------------------------------------

enum _EventType { success, error, warning, info }

class _EventItem extends StatelessWidget {
  final String title;
  final String subtitle;
  final String timestamp;
  final _EventType type;

  const _EventItem({
    required this.title,
    required this.subtitle,
    required this.timestamp,
    required this.type,
  });

  Color _dotColor() {
    switch (type) {
      case _EventType.success:
        return Colors.green;
      case _EventType.error:
        return Colors.red;
      case _EventType.warning:
        return Colors.orange;
      case _EventType.info:
        return Colors.blue;
    }
  }

  IconData _eventIcon() {
    switch (type) {
      case _EventType.success:
        return IconlyBold.shield_done;
      case _EventType.error:
        return IconlyBold.close_square;
      case _EventType.warning:
        return IconlyBold.danger;
      case _EventType.info:
        return IconlyBold.info_circle;
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = _dotColor();

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Colored icon circle
        Container(
          padding: const EdgeInsets.all(8),
          decoration: BoxDecoration(
            color: color.withValues(alpha: 0.12),
            borderRadius: BorderRadius.circular(UIConstants.radiusSmall),
          ),
          child: Icon(
            _eventIcon(),
            size: 16,
            color: color,
          ),
        ),
        const SizedBox(width: UIConstants.spacingMD),
        // Title and subtitle
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: theme.textTheme.bodyMedium?.copyWith(
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                subtitle,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurface
                      .withValues(alpha: 0.45),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(width: UIConstants.spacingSM),
        // Timestamp
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Text(
            timestamp,
            style: theme.textTheme.labelSmall?.copyWith(
              color: theme.colorScheme.onSurface
                  .withValues(alpha: 0.35),
            ),
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Notifications page
// ---------------------------------------------------------------------------

class _NotificationsPage extends StatelessWidget {
  const _NotificationsPage({this.scrollController});
  final ScrollController? scrollController;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: 'Notifications',
      automaticallyImplyLeading: false,
      bodyPadding: EdgeInsets.only(
        left: context.isMobile ? 16 : 88,
        right: 16,
      ),
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.setting),
          tooltip: 'Settings',
          onPressed: () {
            OptionsMenu.show(
              context: context,
              title: 'Notification Settings',
              options: [
                MenuOption(
                  icon: IconlyBroken.notification,
                  label: 'Push Notifications',
                  subtitle: 'Enabled',
                  onTap: () {},
                ),
                MenuOption(
                  icon: IconlyBroken.volume_up,
                  label: 'Sound',
                  subtitle: 'Default',
                  onTap: () {},
                ),
                MenuOption(
                  icon: IconlyBroken.delete,
                  label: 'Clear All',
                  isDestructive: true,
                  onTap: () {
                    NotificationToast.success(
                        context, 'All notifications cleared');
                  },
                ),
              ],
            );
          },
        ),
      ],
      body: ListView(
        controller: scrollController,
        children: [
          const SizedBox(height: UIConstants.spacingSM),

          // Today
          _SectionHeader(
            icon: IconlyBold.notification,
            title: 'Today',
            trailing: FrostedGlass(
              borderRadius: BorderRadius.circular(20),
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
              shadow: false,
              tintColor: theme.colorScheme.primary,
              opacity: 0.1,
              child: Text(
                '4 new',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.primary,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ),

          _NotifCard(
            icon: IconlyBold.shield_done,
            color: Colors.green,
            title: 'Node alpha-7 came online',
            subtitle: 'Joined cluster East-1 via mDNS discovery',
            time: '2 min ago',
          ),
          const SizedBox(height: UIConstants.spacingSM),
          _NotifCard(
            icon: IconlyBold.danger,
            color: Colors.orange,
            title: 'High CPU alert on bravo-3',
            subtitle: 'CPU load exceeded 90% threshold for 5 minutes',
            time: '8 min ago',
          ),
          const SizedBox(height: UIConstants.spacingSM),
          _NotifCard(
            icon: IconlyBold.info_circle,
            color: Colors.blue,
            title: 'Tunnel reconnected',
            subtitle: 'Rathole proxy re-established to us-east relay',
            time: '14 min ago',
          ),
          const SizedBox(height: UIConstants.spacingSM),
          _NotifCard(
            icon: IconlyBold.close_square,
            color: Colors.red,
            title: 'Task batch #47 failed',
            subtitle: '3 of 12 workers reported execution timeout',
            time: '22 min ago',
          ),

          const SizedBox(height: UIConstants.spacingXXL),

          // Earlier
          _SectionHeader(
            icon: IconlyBold.time_circle,
            title: 'Earlier',
          ),

          _NotifCard(
            icon: IconlyBold.star,
            color: theme.colorScheme.tertiary,
            title: 'Cluster East-1 created',
            subtitle: 'New compute cluster initialized with 0 nodes',
            time: 'Yesterday',
          ),
          const SizedBox(height: UIConstants.spacingSM),
          _NotifCard(
            icon: IconlyBold.shield_done,
            color: Colors.green,
            title: 'Engine v0.1.0 deployed',
            subtitle: 'All nodes updated to latest engine version',
            time: '2 days ago',
          ),

          const SizedBox(height: 100),
        ],
      ),
    );
  }
}

class _NotifCard extends StatelessWidget {
  final IconData icon;
  final Color color;
  final String title;
  final String subtitle;
  final String time;

  const _NotifCard({
    required this.icon,
    required this.color,
    required this.title,
    required this.subtitle,
    required this.time,
  });

  void _showDetailModal(BuildContext context) {
    final theme = Theme.of(context);

    FrostedModal.showCupertino(
      context: context,
      builder: (context) => SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.all(UIConstants.spacingXL),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Drag handle
              Center(
                child: Container(
                  width: 36,
                  height: 4,
                  margin: const EdgeInsets.only(bottom: UIConstants.spacingXL),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.onSurface.withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
              ),

              // Icon and color
              Center(
                child: Container(
                  padding: const EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: color.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
                  ),
                  child: Icon(icon, size: 32, color: color),
                ),
              ),

              const SizedBox(height: UIConstants.spacingXL),

              // Title (larger)
              Text(
                title,
                style: theme.textTheme.titleLarge?.copyWith(
                  fontWeight: FontWeight.bold,
                  letterSpacing: -0.3,
                ),
              ),

              const SizedBox(height: UIConstants.spacingMD),

              // Full description
              Text(
                subtitle,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
                  height: 1.5,
                ),
              ),

              const SizedBox(height: UIConstants.spacingLG),

              // Timestamp
              Row(
                children: [
                  Icon(
                    IconlyBroken.time_circle,
                    size: 16,
                    color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
                  ),
                  const SizedBox(width: UIConstants.spacingSM),
                  Text(
                    time,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.45),
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ],
              ),

              const SizedBox(height: UIConstants.spacingXXL),

              // Dismiss button
              SizedBox(
                width: double.infinity,
                height: UIConstants.buttonLG,
                child: FilledButton.icon(
                  onPressed: () => Navigator.pop(context),
                  icon: const Icon(IconlyBroken.shield_done, size: 18),
                  label: const Text('Acknowledge'),
                  style: FilledButton.styleFrom(
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return GestureDetector(
      onTap: () => _showDetailModal(context),
      child: FrostedGlass(
        padding: const EdgeInsets.all(16),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: color.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
              ),
              child: Icon(icon, size: 20, color: color),
            ),
            const SizedBox(width: UIConstants.spacingMD),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Text(
                    subtitle,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                      height: 1.4,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(width: UIConstants.spacingSM),
            Text(
              time,
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.35),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
