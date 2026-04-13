import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../../data/models/workflow_model.dart';
import '../providers/workflow_provider.dart';

class WorkflowListPage extends ConsumerStatefulWidget {
  const WorkflowListPage({super.key, this.scrollController});
  final ScrollController? scrollController;

  @override
  ConsumerState<WorkflowListPage> createState() => _WorkflowListPageState();
}

class _WorkflowListPageState extends ConsumerState<WorkflowListPage> {
  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      ref.read(workflowListProvider.notifier).loadWorkflows();
    });
  }

  void _createWorkflow() {
    AppAlertDialog.showWithInput(
      context: context,
      title: 'New Workflow',
      message: 'Enter a name for the new workflow.',
      hintText: 'e.g. USSD Balance Check',
      actionText: 'Create',
      validator: (v) => v.trim().isEmpty ? 'Name cannot be empty' : null,
      onActionPressed: (name) {
        ref.read(workflowEditorProvider.notifier).createNew(name.trim(), '');
        Navigator.of(context).pushNamed(AppRoutes.workflowEditor);
      },
    );
  }

  void _openWorkflow(Workflow workflow) {
    ref.read(workflowEditorProvider.notifier).loadExisting(workflow.id);
    Navigator.of(
      context,
    ).pushNamed(AppRoutes.workflowEditorWithId(workflow.id));
  }

  void _deleteWorkflow(Workflow workflow) {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Delete Workflow',
      message: 'Are you sure you want to delete "${workflow.name}"?',
      actionText: 'Delete',
      onActionPressed: () async {
        await ref
            .read(workflowListProvider.notifier)
            .deleteWorkflow(workflow.id);
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(workflowListProvider);

    return FrostedScaffold(
      title: 'Workflows',
      automaticallyImplyLeading: false,
      bodyPadding: EdgeInsets.only(
        left: context.isMobile ? 16 : 88,
        right: 16,
      ),
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.plus),
          onPressed: _createWorkflow,
          tooltip: 'New Workflow',
        ),
      ],
      body: state.isLoading
          ? const Center(child: CupertinoActivityIndicator())
          : state.workflows.isEmpty
              ? _EmptyState(onCreateTap: _createWorkflow)
              : _WorkflowListBody(
                  workflows: state.workflows,
                  scrollController: widget.scrollController,
                  onOpen: _openWorkflow,
                  onDelete: _deleteWorkflow,
                  onRefresh: () async {
                    ref.read(workflowListProvider.notifier).loadWorkflows();
                  },
                ),
    );
  }
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

class _EmptyState extends StatelessWidget {
  final VoidCallback onCreateTap;

  const _EmptyState({required this.onCreateTap});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final muted = theme.colorScheme.onSurface;

    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: UIConstants.spacingXXL,
        ),
        child: FrostedGlass(
          padding: const EdgeInsets.symmetric(
            horizontal: UIConstants.spacingXL,
            vertical: UIConstants.spacingXXL,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 72,
                height: 72,
                decoration: BoxDecoration(
                  color: theme.colorScheme.primary.withValues(alpha: 0.1),
                  borderRadius: BorderRadius.circular(UIConstants.radiusXLarge),
                ),
                child: Icon(
                  IconlyBroken.activity,
                  size: 36,
                  color: theme.colorScheme.primary.withValues(alpha: 0.7),
                ),
              ),
              const SizedBox(height: UIConstants.spacingXL),
              Text(
                'No workflows yet',
                style: theme.textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.w600,
                  color: muted.withValues(alpha: 0.8),
                ),
              ),
              const SizedBox(height: UIConstants.spacingSM),
              Text(
                'Build visual pipelines to automate\ntasks across your cluster.',
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: muted.withValues(alpha: 0.5),
                  height: 1.5,
                ),
              ),
              const SizedBox(height: UIConstants.spacingXL),
              FilledButton.icon(
                onPressed: onCreateTap,
                icon: const Icon(IconlyBroken.plus, size: 18),
                label: const Text('Create Workflow'),
                style: FilledButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    horizontal: UIConstants.spacingXL,
                    vertical: UIConstants.spacingMD,
                  ),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(
                      UIConstants.radiusMedium,
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
}

// ---------------------------------------------------------------------------
// Workflow list body
// ---------------------------------------------------------------------------

class _WorkflowListBody extends StatelessWidget {
  final List<Workflow> workflows;
  final ScrollController? scrollController;
  final ValueChanged<Workflow> onOpen;
  final ValueChanged<Workflow> onDelete;
  final Future<void> Function() onRefresh;

  const _WorkflowListBody({
    required this.workflows,
    required this.scrollController,
    required this.onOpen,
    required this.onDelete,
    required this.onRefresh,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final muted = theme.colorScheme.onSurface;

    return RefreshIndicator(
      onRefresh: onRefresh,
      child: ListView.builder(
        controller: scrollController,
        padding: EdgeInsets.only(
          top: UIConstants.spacingLG,
          bottom: 96,
        ),
        itemCount: workflows.length + 1,
        itemBuilder: (context, index) {
          // Header row: count summary
          if (index == 0) {
            return Padding(
              padding: const EdgeInsets.only(
                bottom: UIConstants.spacingMD,
                left: UIConstants.spacingXS,
              ),
              child: Text(
                '${workflows.length} workflow${workflows.length == 1 ? '' : 's'}',
                style: theme.textTheme.bodySmall?.copyWith(
                  color: muted.withValues(alpha: 0.45),
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.3,
                ),
              ),
            );
          }

          final wf = workflows[index - 1];
          return Padding(
            padding: const EdgeInsets.only(bottom: UIConstants.spacingSM),
            child: _WorkflowCard(
              workflow: wf,
              onTap: () => onOpen(wf),
              onDelete: () => onDelete(wf),
            ),
          );
        },
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Workflow card
// ---------------------------------------------------------------------------

class _WorkflowCard extends StatelessWidget {
  final Workflow workflow;
  final VoidCallback onTap;
  final VoidCallback onDelete;

  const _WorkflowCard({
    required this.workflow,
    required this.onTap,
    required this.onDelete,
  });

  String _timeAgo(DateTime dt) {
    final diff = DateTime.now().difference(dt);
    if (diff.inDays > 365) return '${diff.inDays ~/ 365}y ago';
    if (diff.inDays > 30) return '${diff.inDays ~/ 30}mo ago';
    if (diff.inDays > 0) return '${diff.inDays}d ago';
    if (diff.inHours > 0) return '${diff.inHours}h ago';
    if (diff.inMinutes > 0) return '${diff.inMinutes}m ago';
    return 'just now';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final primary = theme.colorScheme.primary;
    final muted = theme.colorScheme.onSurface;

    return GestureDetector(
      onTap: onTap,
      child: FrostedGlass(
        padding: const EdgeInsets.all(UIConstants.spacingLG),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Top row: icon + title + delete
            Row(
              children: [
                // Workflow icon
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: primary.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(
                      UIConstants.radiusMedium,
                    ),
                  ),
                  child: Icon(
                    IconlyBold.activity,
                    size: UIConstants.iconMD,
                    color: primary,
                  ),
                ),
                const SizedBox(width: UIConstants.spacingMD),

                // Name and timestamp
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        workflow.name,
                        style: theme.textTheme.titleSmall?.copyWith(
                          fontWeight: FontWeight.w700,
                          letterSpacing: -0.2,
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 2),
                      Row(
                        children: [
                          Icon(
                            IconlyBroken.time_circle,
                            size: 12,
                            color: muted.withValues(alpha: 0.4),
                          ),
                          const SizedBox(width: 4),
                          Text(
                            _timeAgo(workflow.updatedAt),
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: muted.withValues(alpha: 0.45),
                              fontWeight: FontWeight.w400,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),

                // Delete button
                SizedBox(
                  width: 36,
                  height: 36,
                  child: IconButton(
                    onPressed: onDelete,
                    padding: EdgeInsets.zero,
                    icon: Icon(
                      IconlyBroken.delete,
                      size: 18,
                      color: theme.colorScheme.error.withValues(alpha: 0.7),
                    ),
                    tooltip: 'Delete',
                    style: IconButton.styleFrom(
                      backgroundColor: theme.colorScheme.error.withValues(
                        alpha: 0.08,
                      ),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(
                          UIConstants.radiusSmall,
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),

            // Description (if present)
            if (workflow.description.isNotEmpty) ...[
              const SizedBox(height: UIConstants.spacingMD),
              Text(
                workflow.description,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: muted.withValues(alpha: 0.6),
                  height: 1.4,
                ),
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
              ),
            ],

            // Bottom row: badges + arrow
            const SizedBox(height: UIConstants.spacingMD),
            Row(
              children: [
                _StatPill(
                  icon: IconlyBold.discovery,
                  label: '${workflow.nodes.length}',
                  tooltip: 'Nodes',
                  color: primary,
                ),
                const SizedBox(width: UIConstants.spacingSM),
                _StatPill(
                  icon: IconlyBold.send,
                  label: '${workflow.edges.length}',
                  tooltip: 'Edges',
                  color: theme.colorScheme.tertiary,
                ),
                const Spacer(),
                Icon(
                  IconlyBroken.arrow_right_2,
                  size: 16,
                  color: muted.withValues(alpha: 0.3),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Stat pill badge
// ---------------------------------------------------------------------------

class _StatPill extends StatelessWidget {
  final IconData icon;
  final String label;
  final String tooltip;
  final Color color;

  const _StatPill({
    required this.icon,
    required this.label,
    required this.tooltip,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Tooltip(
      message: tooltip,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.1),
          borderRadius: BorderRadius.circular(UIConstants.radiusCircle),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 12, color: color.withValues(alpha: 0.8)),
            const SizedBox(width: 5),
            Text(
              label,
              style: theme.textTheme.labelSmall?.copyWith(
                color: color.withValues(alpha: 0.9),
                fontWeight: FontWeight.w600,
                fontSize: 11,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
