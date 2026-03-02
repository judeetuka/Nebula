import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../core/di/injection.dart';
import '../../../auth/presentation/providers/auth_provider.dart';
import '../../../cluster/presentation/pages/dashboard_page.dart';
import '../../../cluster/presentation/providers/cluster_provider.dart';
import '../../../../config/router.dart';

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

class _OverviewPage extends ConsumerWidget {
  const _OverviewPage();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final clustersState = ref.watch(clustersProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Overview')),
      body: Padding(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Cluster Summary', style: theme.textTheme.headlineSmall),
            const SizedBox(height: UIConstants.spacingLG),
            Row(
              children: [
                _StatCard(
                  label: 'Clusters',
                  value: '${clustersState.clusters.length}',
                  icon: Icons.cloud,
                  color: theme.colorScheme.primary,
                ),
                const SizedBox(width: UIConstants.spacingMD),
                _StatCard(
                  label: 'Total Nodes',
                  value:
                      '${clustersState.clusters.fold<int>(0, (sum, c) => sum + c.nodeCount)}',
                  icon: Icons.devices,
                  color: theme.colorScheme.secondary,
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _StatCard extends StatelessWidget {
  final String label;
  final String value;
  final IconData icon;
  final Color color;

  const _StatCard({
    required this.label,
    required this.value,
    required this.icon,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Expanded(
      child: Card(
        child: Padding(
          padding: UIConstants.paddingLG,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(icon, color: color, size: UIConstants.iconXL),
              const SizedBox(height: UIConstants.spacingSM),
              Text(value, style: theme.textTheme.headlineMedium),
              Text(label, style: theme.textTheme.bodySmall),
            ],
          ),
        ),
      ),
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
            padding: const EdgeInsets.symmetric(
              horizontal: 24,
              vertical: 8,
            ),
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
            padding: const EdgeInsets.symmetric(
              horizontal: 24,
              vertical: 8,
            ),
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
            padding: const EdgeInsets.symmetric(
              horizontal: 24,
              vertical: 8,
            ),
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
                message: 'Version 0.1.0+1\nDistributed compute cluster dashboard.',
              );
            },
          ),
        ],
      ),
    );
  }
}
