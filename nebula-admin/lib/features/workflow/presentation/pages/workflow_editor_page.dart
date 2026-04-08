import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';

import '../../data/models/workflow_node_model.dart';
import '../../data/models/workflow_edge_model.dart';
import '../providers/workflow_provider.dart';
import '../widgets/node_palette.dart';
import 'nebula_flow_theme.dart';

/// Data payload stored in each vyuh_node_flow Node.
class WorkflowNodeData {
  final String pluginId;
  final String action;
  final String label;
  final String nodeType;
  final Map<String, dynamic> config;

  const WorkflowNodeData({
    required this.pluginId,
    required this.action,
    required this.label,
    required this.nodeType,
    this.config = const {},
  });
}

/// Full-screen workflow editor powered by vyuh_node_flow.
class WorkflowEditorPage extends ConsumerStatefulWidget {
  final String? workflowId;
  const WorkflowEditorPage({super.key, this.workflowId});

  @override
  ConsumerState<WorkflowEditorPage> createState() => _WorkflowEditorPageState();
}

class _WorkflowEditorPageState extends ConsumerState<WorkflowEditorPage> {
  late final NodeFlowController<WorkflowNodeData, dynamic> _flowController;
  String? _selectedNodeId;
  int _dropCounter = 0;

  @override
  void initState() {
    super.initState();

    _flowController = NodeFlowController<WorkflowNodeData, dynamic>();

    if (widget.workflowId != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _loadWorkflow());
    }
  }

  Node<WorkflowNodeData> _makeFlowNode(
    String nodeId,
    String type,
    Offset pos,
    WorkflowNodeData data,
  ) {
    return Node<WorkflowNodeData>(
      id: nodeId,
      type: type,
      position: pos,
      data: data,
      ports: [
        Port(
          id: '$nodeId-in',
          name: 'In',
          position: PortPosition.left,
          type: PortType.input,
        ),
        Port(
          id: '$nodeId-out',
          name: 'Out',
          position: PortPosition.right,
          type: PortType.output,
        ),
      ],
    );
  }

  void _loadWorkflow() {
    final editor = ref.read(workflowEditorProvider.notifier);
    editor.loadExisting(widget.workflowId!);
    final wf = ref.read(workflowEditorProvider).workflow;
    if (wf == null) return;

    final nodes = wf.nodes
        .map(
          (wn) => _makeFlowNode(
            wn.id,
            wn.type,
            wn.position,
            WorkflowNodeData(
              pluginId: wn.pluginId,
              action: wn.action,
              label: wn.label,
              nodeType: wn.type,
              config: wn.config,
            ),
          ),
        )
        .toList();

    final connections = wf.edges
        .map(
          (we) => Connection<dynamic>(
            id: we.id,
            sourceNodeId: we.fromNodeId,
            sourcePortId: '${we.fromNodeId}-out',
            targetNodeId: we.toNodeId,
            targetPortId: '${we.toNodeId}-in',
          ),
        )
        .toList();

    _flowController.loadGraph(
      NodeGraph(nodes: nodes, connections: connections),
    );
  }

  void _onNodeAddedFromPalette(WorkflowNode template) {
    final offset = Offset(
      200 + (_dropCounter % 4) * 220.0,
      100 + (_dropCounter ~/ 4) * 140.0,
    );
    _dropCounter++;
    final nodeId = 'node_${DateTime.now().millisecondsSinceEpoch}';

    _flowController.addNode(
      _makeFlowNode(
        nodeId,
        template.type,
        offset,
        WorkflowNodeData(
          pluginId: template.pluginId,
          action: template.action,
          label: template.label,
          nodeType: template.type,
          config: template.config,
        ),
      ),
    );
  }

  void _onSave() {
    final nodes = _flowController.nodes.values.map((n) {
      final d = n.data;
      return WorkflowNode(
        id: n.id,
        type: n.type,
        pluginId: d.pluginId,
        action: d.action,
        config: d.config,
        position: n.position.value,
        label: d.label,
      );
    }).toList();

    final edges = _flowController.connections
        .map(
          (c) => WorkflowEdge(
            id: c.id,
            fromNodeId: c.sourceNodeId,
            toNodeId: c.targetNodeId,
            condition: null,
          ),
        )
        .toList();

    final editor = ref.read(workflowEditorProvider.notifier);
    if (ref.read(workflowEditorProvider).workflow == null) {
      editor.createNew(
        'Workflow ${DateTime.now().toIso8601String().substring(0, 16)}',
        '',
      );
    }
    final wf = ref.read(workflowEditorProvider).workflow!;
    final updated = wf.copyWith(
      nodes: nodes,
      edges: edges,
      updatedAt: DateTime.now(),
    );

    ref.read(workflowRepositoryProvider).save(updated);

    ScaffoldMessenger.of(
      context,
    ).showSnackBar(const SnackBar(content: Text('Workflow saved')));
  }

  void _onClear() {
    final ids = _flowController.nodes.keys.toList();
    for (final id in ids) {
      _flowController.removeNode(id);
    }
    _dropCounter = 0;
  }

  @override
  void dispose() {
    _flowController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final isDark = cs.brightness == Brightness.dark;

    return Scaffold(
      appBar: AppBar(
        title: Text(
          widget.workflowId != null ? 'Edit Workflow' : 'New Workflow',
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.delete_outline),
            onPressed: _onClear,
            tooltip: 'Clear',
          ),
          const SizedBox(width: 8),
          FilledButton.icon(
            icon: const Icon(Icons.save, size: 18),
            label: const Text('Save'),
            onPressed: _onSave,
          ),
          const SizedBox(width: 12),
        ],
      ),
      body: Row(
        children: [
          SizedBox(
            width: 260,
            child: NodePalette(onNodeSelected: _onNodeAddedFromPalette),
          ),
          VerticalDivider(width: 1, color: cs.outlineVariant),
          Expanded(
            child: NodeFlowEditor<WorkflowNodeData, dynamic>(
              controller: _flowController,
              theme: nebulaFlowTheme(isDark: isDark, colorScheme: cs),
              nodeBuilder: (context, node) => _buildNodeCard(node, cs),
              events: NodeFlowEvents<WorkflowNodeData, dynamic>(
                node: NodeEvents<WorkflowNodeData>(
                  onTap: (node) {
                    setState(() => _selectedNodeId = node.id);
                    _showNodeProperties(context, node);
                  },
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildNodeCard(Node<WorkflowNodeData> node, ColorScheme cs) {
    final data = node.data;
    final color = _nodeColor(data.nodeType, cs);
    final icon = _nodeIcon(data.nodeType);
    final isSelected = _selectedNodeId == node.id;

    return Container(
      width: 200,
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(
          color: isSelected ? cs.primary : color.withValues(alpha: 0.4),
          width: isSelected ? 2.5 : 1.5,
        ),
        boxShadow: [
          BoxShadow(
            color: color.withValues(alpha: isSelected ? 0.25 : 0.08),
            blurRadius: isSelected ? 12 : 4,
            offset: const Offset(0, 2),
          ),
        ],
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 32,
                height: 32,
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.2),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Icon(icon, size: 18, color: color),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  data.label,
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 13,
                    color: cs.onSurface,
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '${data.pluginId}:${data.action}',
            style: TextStyle(
              fontSize: 11,
              color: cs.onSurfaceVariant,
              fontFamily: 'monospace',
            ),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          if (data.config.isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              '${data.config.length} param${data.config.length == 1 ? '' : 's'}',
              style: TextStyle(fontSize: 10, color: cs.outline),
            ),
          ],
        ],
      ),
    );
  }

  Color _nodeColor(String type, ColorScheme cs) => switch (type) {
    WorkflowNode.typePlugin => cs.primary,
    WorkflowNode.typeCondition => Colors.amber,
    WorkflowNode.typeDelay => Colors.teal,
    WorkflowNode.typeTrigger => Colors.deepOrange,
    WorkflowNode.typeLoop => Colors.purple,
    _ => cs.secondary,
  };

  IconData _nodeIcon(String type) => switch (type) {
    WorkflowNode.typePlugin => Icons.extension,
    WorkflowNode.typeCondition => Icons.call_split,
    WorkflowNode.typeDelay => Icons.timer,
    WorkflowNode.typeTrigger => Icons.bolt,
    WorkflowNode.typeLoop => Icons.loop,
    _ => Icons.circle,
  };

  void _showNodeProperties(BuildContext context, Node<WorkflowNodeData> node) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      builder: (_) => DraggableScrollableSheet(
        expand: false,
        initialChildSize: 0.35,
        minChildSize: 0.2,
        maxChildSize: 0.6,
        builder: (context, sc) => Padding(
          padding: const EdgeInsets.all(16),
          child: ListView(
            controller: sc,
            children: [
              Text(
                node.data.label,
                style: Theme.of(
                  context,
                ).textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
              ),
              const SizedBox(height: 4),
              Text(
                '${node.data.pluginId}:${node.data.action}',
                style: Theme.of(
                  context,
                ).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
              ),
              const Divider(height: 24),
              Text('Parameters', style: Theme.of(context).textTheme.labelLarge),
              const SizedBox(height: 8),
              if (node.data.config.isEmpty)
                Text(
                  'No parameters configured',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ...node.data.config.entries.map(
                (e) => Padding(
                  padding: const EdgeInsets.symmetric(vertical: 2),
                  child: Row(
                    children: [
                      Text(
                        '${e.key}: ',
                        style: const TextStyle(
                          fontWeight: FontWeight.w500,
                          fontSize: 13,
                        ),
                      ),
                      Expanded(
                        child: Text(
                          '${e.value}',
                          style: const TextStyle(fontSize: 13),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 16),
              OutlinedButton.icon(
                icon: const Icon(Icons.delete, size: 18),
                label: const Text('Remove Node'),
                onPressed: () {
                  _flowController.removeNode(node.id);
                  Navigator.pop(context);
                },
              ),
            ],
          ),
        ),
      ),
    );
  }
}
