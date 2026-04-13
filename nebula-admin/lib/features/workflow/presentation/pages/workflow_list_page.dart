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
    final theme = Theme.of(context);
    final state = ref.watch(workflowListProvider);

    return FrostedScaffold(
      title: 'Workflows',
      actions: [
        IconButton(
          icon: const Icon(IconlyBroken.plus),
          onPressed: _createWorkflow,
          tooltip: 'New Workflow',
        ),
      ],
      body: state.isLoading
          ? const Center(child: CircularProgressIndicator())
          : state.workflows.isEmpty
          ? Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    IconlyBroken.activity,
                    size: 64,
                    color: theme.colorScheme.onSurface.withValues(alpha: 0.4),
                  ),
                  const SizedBox(height: UIConstants.spacingLG),
                  Text(
                    'No workflows yet',
                    style: theme.textTheme.titleMedium?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
                    ),
                  ),
                  const SizedBox(height: UIConstants.spacingSM),
                  Text(
                    'Create a workflow to build a visual pipeline.',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                    ),
                  ),
                ],
              ),
            )
          : RefreshIndicator(
              onRefresh: () async {
                ref.read(workflowListProvider.notifier).loadWorkflows();
              },
              child: ListView.separated(
                controller: widget.scrollController,
                padding:
                    UIConstants.paddingLG +
                    EdgeInsets.only(left: context.isMobile ? 0 : 72),
                itemCount: state.workflows.length + 1, // +1 for bottom spacer
                separatorBuilder: (_, _) =>
                    const SizedBox(height: UIConstants.spacingSM),
                itemBuilder: (context, index) {
                  if (index == state.workflows.length) {
                    return const SizedBox(height: 80);
                  }
                  final wf = state.workflows[index];
                  return _WorkflowCard(
                    workflow: wf,
                    onTap: () => _openWorkflow(wf),
                    onDelete: () => _deleteWorkflow(wf),
                  );
                },
              ),
            ),
    );
  }
}

class _WorkflowCard extends StatelessWidget {
  final Workflow workflow;
  final VoidCallback onTap;
  final VoidCallback onDelete;

  const _WorkflowCard({
    required this.workflow,
    required this.onTap,
    required this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return GestureDetector(
      onTap: onTap,
      child: FrostedGlass(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Container(
              width: 44,
              height: 44,
              decoration: BoxDecoration(
                color: theme.colorScheme.primary.withValues(alpha: 0.15),
                borderRadius: BorderRadius.circular(12),
              ),
              child: Icon(
                IconlyBold.activity,
                color: theme.colorScheme.primary,
              ),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    workflow.name,
                    style: theme.textTheme.titleSmall?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  if (workflow.description.isNotEmpty)
                    Text(
                      workflow.description,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurface.withValues(
                          alpha: 0.6,
                        ),
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  const SizedBox(height: 4),
                  Text(
                    '${workflow.nodes.length} nodes · ${workflow.edges.length} edges',
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onSurface.withValues(alpha: 0.5),
                    ),
                  ),
                ],
              ),
            ),
            IconButton(
              icon: Icon(
                IconlyBroken.delete,
                size: 20,
                color: theme.colorScheme.error,
              ),
              onPressed: onDelete,
              tooltip: 'Delete',
            ),
          ],
        ),
      ),
    );
  }
}
