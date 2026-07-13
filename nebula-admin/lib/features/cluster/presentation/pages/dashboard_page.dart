import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/cluster_provider.dart';
import '../widgets/cluster_card.dart';

class DashboardPage extends ConsumerStatefulWidget {
  const DashboardPage({super.key, this.scrollController});
  final ScrollController? scrollController;

  @override
  ConsumerState<DashboardPage> createState() => _DashboardPageState();
}

class _DashboardPageState extends ConsumerState<DashboardPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(() => ref.read(clustersProvider.notifier).loadClusters());
  }

  void _showCreateClusterModal() {
    FrostedModal.showCupertino(
      context: context,
      expand: true,
      builder: (context) => _CreateClusterModal(
        onCreated: () {
          Navigator.pop(context);
          ref.read(clustersProvider.notifier).loadClusters();
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(clustersProvider);
    final theme = Theme.of(context);

    return FrostedScaffold(
      title: 'Clusters',
      automaticallyImplyLeading: false,
      bodyPadding: EdgeInsets.only(
        left: context.isMobile ? 16 : 88,
        right: 16,
      ),
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.plus),
          onPressed: _showCreateClusterModal,
          tooltip: 'New Cluster',
        ),
      ],
      body: _buildBody(state, theme),
    );
  }

  Widget _buildBody(ClustersState state, ThemeData theme) {
    if (state.isLoading && state.clusters.isEmpty) {
      return const Center(child: CupertinoActivityIndicator());
    }

    if (state.error != null && state.clusters.isEmpty) {
      return _buildErrorState(state, theme);
    }

    if (state.clusters.isEmpty) {
      return _buildEmptyState(theme);
    }

    return RefreshIndicator(
      onRefresh: () => ref.read(clustersProvider.notifier).loadClusters(),
      color: theme.colorScheme.primary,
      child: LayoutBuilder(
        builder: (context, constraints) {
          final crossAxisCount = constraints.maxWidth > 900
              ? 3
              : (constraints.maxWidth > 560 ? 2 : 1);

          if (crossAxisCount == 1) {
            return ListView.separated(
              controller: widget.scrollController,
              padding: const EdgeInsets.all(UIConstants.spacingLG),
              itemCount: state.clusters.length + 1,
              separatorBuilder: (_, _) =>
                  const SizedBox(height: UIConstants.spacingMD),
              itemBuilder: (context, index) {
                if (index == state.clusters.length) {
                  return const SizedBox(height: 80);
                }
                final cluster = state.clusters[index];
                return ClusterCard(
                  cluster: cluster,
                  onTap: () => Navigator.of(
                    context,
                  ).pushNamed(AppRoutes.clusterDetail(cluster.id)),
                );
              },
            );
          }

          return GridView.builder(
            controller: widget.scrollController,
            padding: const EdgeInsets.all(UIConstants.spacingLG),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: crossAxisCount,
              crossAxisSpacing: UIConstants.spacingLG,
              mainAxisSpacing: UIConstants.spacingLG,
              childAspectRatio: 1.9,
            ),
            itemCount: state.clusters.length,
            itemBuilder: (context, index) {
              final cluster = state.clusters[index];
              return ClusterCard(
                cluster: cluster,
                onTap: () => Navigator.of(
                  context,
                ).pushNamed(AppRoutes.clusterDetail(cluster.id)),
              );
            },
          );
        },
      ),
    );
  }

  Widget _buildEmptyState(ThemeData theme) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(UIConstants.spacingXXL),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            FrostedGlass(
              borderRadius: BorderRadius.circular(24),
              padding: const EdgeInsets.all(24),
              tintColor: theme.colorScheme.primary,
              opacity: 0.08,
              shadow: false,
              child: Icon(
                IconlyBroken.discovery,
                size: 48,
                color: theme.colorScheme.primary.withValues(alpha: 0.7),
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),
            Text(
              'No clusters yet',
              style: theme.textTheme.titleLarge?.copyWith(
                fontWeight: FontWeight.bold,
                color: theme.colorScheme.onSurface.withValues(alpha: 0.8),
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              'Create your first compute cluster to get started',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            FilledButton.icon(
              onPressed: _showCreateClusterModal,
              icon: const Icon(IconlyBroken.plus, size: 18),
              label: const Text('Create Cluster'),
              style: FilledButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildErrorState(ClustersState state, ThemeData theme) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(UIConstants.spacingXXL),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            FrostedGlass(
              borderRadius: BorderRadius.circular(24),
              padding: const EdgeInsets.all(24),
              tintColor: theme.colorScheme.error,
              opacity: 0.1,
              shadow: false,
              child: Icon(
                IconlyBroken.danger,
                size: 40,
                color: theme.colorScheme.error,
              ),
            ),
            const SizedBox(height: UIConstants.spacingXL),
            Text(
              'Something went wrong',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            Text(
              state.error!,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: UIConstants.spacingXL),
            FilledButton.icon(
              onPressed: () =>
                  ref.read(clustersProvider.notifier).loadClusters(),
              icon: const Icon(IconlyBroken.arrow_down_2, size: 18),
              label: const Text('Retry'),
              style: FilledButton.styleFrom(
                padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Create Cluster Modal
// ---------------------------------------------------------------------------

class _CreateClusterModal extends ConsumerStatefulWidget {
  final VoidCallback onCreated;

  const _CreateClusterModal({required this.onCreated});

  @override
  ConsumerState<_CreateClusterModal> createState() =>
      _CreateClusterModalState();
}

class _CreateClusterModalState extends ConsumerState<_CreateClusterModal> {
  final _nameController = TextEditingController();
  final _formKey = GlobalKey<FormState>();
  bool _isCreating = false;

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  Future<void> _handleCreate() async {
    if (!_formKey.currentState!.validate()) return;

    setState(() => _isCreating = true);

    final success = await ref
        .read(clustersProvider.notifier)
        .createCluster(name: _nameController.text.trim());

    if (!mounted) return;

    setState(() => _isCreating = false);

    if (success) {
      widget.onCreated();
    } else {
      final error = ref.read(clustersProvider).error;
      if (error != null) {
        NotificationToast.error(context, error);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Material(
      type: MaterialType.transparency,
      child: SafeArea(
        top: false,
        child: Column(
          children: [
            // Drag handle — always at top
            Center(
              child: Container(
                width: 36,
                height: 4,
                margin: const EdgeInsets.only(top: 12, bottom: 8),
                decoration: BoxDecoration(
                  color: theme.colorScheme.onSurface.withValues(alpha: 0.2),
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
            ),
            // Scrollable form content
            Expanded(
              child: Center(
                child: SingleChildScrollView(
                  controller: SheetScrollController.of(context),
                  padding: const EdgeInsets.symmetric(
                    horizontal: UIConstants.spacingXL,
                    vertical: UIConstants.spacingXL,
                  ),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 460),
                    child: Form(
                      key: _formKey,
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                  // Icon header
                  Center(
                    child: Container(
                      width: 64,
                      height: 64,
                      decoration: BoxDecoration(
                        color: theme.colorScheme.primary
                            .withValues(alpha: 0.12),
                        borderRadius: BorderRadius.circular(20),
                      ),
                      child: Icon(
                        IconlyBold.plus,
                        size: 30,
                        color: theme.colorScheme.primary,
                      ),
                    ),
                  ),
                  const SizedBox(height: UIConstants.spacingXL),

                  // Title
                  Text(
                    'Create Compute Cluster',
                    style: theme.textTheme.titleLarge?.copyWith(
                      fontWeight: FontWeight.bold,
                      letterSpacing: -0.5,
                    ),
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: UIConstants.spacingSM),

                  // Subtitle
                  Text(
                    'Give your cluster a name to get started',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurface
                          .withValues(alpha: 0.5),
                    ),
                    textAlign: TextAlign.center,
                  ),
                  const SizedBox(height: UIConstants.spacingXXL),

                  // Name field inside a frosted container
                  FrostedGlass(
                    borderRadius: BorderRadius.circular(
                        UIConstants.radiusMedium),
                    padding: const EdgeInsets.symmetric(
                      horizontal: UIConstants.spacingLG,
                      vertical: UIConstants.spacingXS,
                    ),
                    shadow: false,
                    opacity: 0.08,
                    child: TextFormField(
                      controller: _nameController,
                      decoration: InputDecoration(
                        labelText: 'Cluster Name',
                        hintText: 'e.g. Production Alpha',
                        prefixIcon: Icon(
                          IconlyBroken.discovery,
                          color: theme.colorScheme.primary
                              .withValues(alpha: 0.7),
                        ),
                        border: InputBorder.none,
                        enabledBorder: InputBorder.none,
                        focusedBorder: InputBorder.none,
                        errorBorder: InputBorder.none,
                        focusedErrorBorder: InputBorder.none,
                        contentPadding: const EdgeInsets.symmetric(
                          vertical: UIConstants.spacingMD,
                        ),
                      ),
                      textInputAction: TextInputAction.done,
                      autofocus: true,
                      onFieldSubmitted: (_) => _handleCreate(),
                      validator: (value) {
                        if (value == null || value.trim().isEmpty) {
                          return 'Cluster name is required';
                        }
                        if (value.trim().length < 3) {
                          return 'Name must be at least 3 characters';
                        }
                        return null;
                      },
                    ),
                  ),
                  const SizedBox(height: UIConstants.spacingXL),

                  // Create button
                  SizedBox(
                    height: UIConstants.buttonLG,
                    child: FilledButton(
                      onPressed: _isCreating ? null : _handleCreate,
                      style: FilledButton.styleFrom(
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(
                            UIConstants.radiusMedium,
                          ),
                        ),
                      ),
                      child: _isCreating
                          ? SizedBox(
                              height: 20,
                              width: 20,
                              child: CupertinoActivityIndicator(
                                color: theme.colorScheme.onPrimary,
                              ),
                            )
                          : Row(
                              mainAxisAlignment: MainAxisAlignment.center,
                              children: [
                                Icon(IconlyBold.plus, size: 18,
                                  color: theme.colorScheme.onPrimary),
                                const SizedBox(width: UIConstants.spacingSM),
                                Text(
                                  'Create Cluster',
                                  style: TextStyle(
                                    fontWeight: FontWeight.w700,
                                    color: theme.colorScheme.onPrimary,
                                  ),
                                ),
                              ],
                            ),
                    ),
                  ),

                  const SizedBox(height: UIConstants.spacingLG),

                  // Cancel button
                  SizedBox(
                    height: UIConstants.buttonLG,
                    child: TextButton(
                      onPressed: () => Navigator.pop(context),
                      style: TextButton.styleFrom(
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(
                            UIConstants.radiusMedium,
                          ),
                        ),
                      ),
                      child: Text(
                        'Cancel',
                        style: TextStyle(
                          fontWeight: FontWeight.w600,
                          color: theme.colorScheme.onSurface
                              .withValues(alpha: 0.6),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
      ),
      ],
      ),
      ),
    );
  }
}
