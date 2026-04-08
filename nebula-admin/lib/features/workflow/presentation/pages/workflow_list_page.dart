import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import '../../../../config/router.dart';
import '../../data/models/workflow_model.dart';
import '../providers/workflow_provider.dart';

class WorkflowListPage extends ConsumerStatefulWidget {
  const WorkflowListPage({super.key});

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
    final nameController = TextEditingController();
    final descController = TextEditingController();
    showDialog<void>(
      context: context,
      builder: (ctx) {
        return AlertDialog(
          title: const Text('New Workflow'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                controller: nameController,
                decoration: const InputDecoration(
                  labelText: 'Name',
                  hintText: 'e.g. USSD Balance Check',
                ),
                autofocus: true,
              ),
              const SizedBox(height: 12),
              TextField(
                controller: descController,
                decoration: const InputDecoration(
                  labelText: 'Description',
                  hintText: 'Optional description',
                ),
                maxLines: 2,
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(ctx).pop(),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () {
                final name = nameController.text.trim();
                if (name.isEmpty) return;
                Navigator.of(ctx).pop();
                ref
                    .read(workflowEditorProvider.notifier)
                    .createNew(name, descController.text.trim());
                Navigator.of(context).pushNamed(AppRoutes.workflowEditor);
              },
              child: const Text('Create'),
            ),
          ],
        );
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

    return Scaffold(
      appBar: AppBar(title: const Text('Workflows')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _createWorkflow,
        icon: const Icon(Icons.add),
        label: const Text('New Workflow'),
      ),
      body: state.isLoading
          ? const Center(child: CircularProgressIndicator())
          : state.workflows.isEmpty
          ? Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.account_tree_outlined,
                    size: 64,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(height: UIConstants.spacingLG),
                  Text(
                    'No workflows yet',
                    style: theme.textTheme.titleMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: UIConstants.spacingSM),
                  Text(
                    'Create a workflow to build a visual pipeline.',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
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
                padding: UIConstants.paddingLG,
                itemCount: state.workflows.length,
                separatorBuilder: (_, _) =>
                    const SizedBox(height: UIConstants.spacingSM),
                itemBuilder: (context, index) {
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
    return Card(
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  color: theme.colorScheme.primaryContainer,
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Icon(
                  Icons.account_tree,
                  color: theme.colorScheme.onPrimaryContainer,
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
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    const SizedBox(height: 4),
                    Text(
                      '${workflow.nodes.length} nodes · ${workflow.edges.length} edges',
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              IconButton(
                icon: const Icon(Icons.delete_outline, size: 20),
                onPressed: onDelete,
                tooltip: 'Delete',
              ),
            ],
          ),
        ),
      ),
    );
  }
}
