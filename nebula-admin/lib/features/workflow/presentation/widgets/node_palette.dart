import 'package:flutter/material.dart';

import '../../data/models/workflow_node_model.dart';

/// A catalog of available plugin actions and flow-control nodes.
///
/// Each entry is a [WorkflowNode] template that can be dragged or tapped to
/// add onto the editor canvas.
class NodePalette extends StatelessWidget {
  final ValueChanged<WorkflowNode> onNodeSelected;

  const NodePalette({super.key, required this.onNodeSelected});

  // ---- Catalog definition ---------------------------------------------------

  static const _builtInPlugins = [
    _PaletteEntry(
      group: 'Comm-Link',
      icon: Icons.cell_tower,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'comm-link',
          action: 'send_ussd',
          label: 'Send USSD',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'comm-link',
          action: 'send_sms',
          label: 'Send SMS',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Observer',
      icon: Icons.visibility,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'observer',
          action: 'start_observing',
          label: 'Start Observing',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'observer',
          action: 'stop_observing',
          label: 'Stop Observing',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Device Info',
      icon: Icons.phone_android,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'device-info',
          action: 'get_device_info',
          label: 'Get Device Info',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Accessibility',
      icon: Icons.touch_app,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'accessibility',
          action: 'tap',
          label: 'Tap Element',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'accessibility',
          action: 'type_text',
          label: 'Type Text',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'accessibility',
          action: 'navigate',
          label: 'Navigate',
        ),
      ],
    ),
  ];

  static const _firstPartyPlugins = [
    _PaletteEntry(
      group: 'WhatsApp',
      icon: Icons.chat,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'whatsapp',
          action: 'send_message',
          label: 'Send Message',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'whatsapp',
          action: 'send_media',
          label: 'Send Media',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'whatsapp',
          action: 'get_contacts',
          label: 'Get Contacts',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Classifier',
      icon: Icons.category,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'classifier',
          action: 'classify',
          label: 'Classify',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'classifier',
          action: 'get_categories',
          label: 'Get Categories',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Email',
      icon: Icons.email,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'email',
          action: 'send',
          label: 'Send Email',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'email',
          action: 'check_inbox',
          label: 'Check Inbox',
        ),
      ],
    ),
    _PaletteEntry(
      group: 'Linux Bridge',
      icon: Icons.terminal,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'linux-bridge',
          action: 'exec',
          label: 'Execute Command',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typePlugin,
          pluginId: 'linux-bridge',
          action: 'install_distro',
          label: 'Install Distro',
        ),
      ],
    ),
  ];

  static const _flowControlNodes = [
    _PaletteEntry(
      group: 'Flow Control',
      icon: Icons.account_tree,
      nodes: [
        WorkflowNode(
          id: '',
          type: WorkflowNode.typeCondition,
          pluginId: 'flow',
          action: 'condition',
          label: 'Condition',
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typeDelay,
          pluginId: 'flow',
          action: 'delay',
          label: 'Delay',
          config: {'seconds': 5},
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typeLoop,
          pluginId: 'flow',
          action: 'loop',
          label: 'Loop',
          config: {'iterations': 3},
        ),
        WorkflowNode(
          id: '',
          type: WorkflowNode.typeTrigger,
          pluginId: 'flow',
          action: 'trigger',
          label: 'Trigger',
        ),
      ],
    ),
  ];

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Container(
      width: 240,
      color: theme.colorScheme.surfaceContainerLow,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(16),
            child: Text(
              'Node Palette',
              style: theme.textTheme.titleSmall?.copyWith(
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              children: [
                _SectionHeader(title: 'Built-in Plugins'),
                for (final entry in _builtInPlugins)
                  _PaletteGroup(entry: entry, onNodeSelected: onNodeSelected),
                const SizedBox(height: 12),
                _SectionHeader(title: 'First-Party Plugins'),
                for (final entry in _firstPartyPlugins)
                  _PaletteGroup(entry: entry, onNodeSelected: onNodeSelected),
                const SizedBox(height: 12),
                _SectionHeader(title: 'Flow Control'),
                for (final entry in _flowControlNodes)
                  _PaletteGroup(entry: entry, onNodeSelected: onNodeSelected),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

class _PaletteEntry {
  final String group;
  final IconData icon;
  final List<WorkflowNode> nodes;

  const _PaletteEntry({
    required this.group,
    required this.icon,
    required this.nodes,
  });
}

class _SectionHeader extends StatelessWidget {
  final String title;
  const _SectionHeader({required this.title});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(left: 8, top: 8, bottom: 4),
      child: Text(
        title.toUpperCase(),
        style: Theme.of(context).textTheme.labelSmall?.copyWith(
          color: Theme.of(context).colorScheme.onSurfaceVariant,
          fontWeight: FontWeight.w600,
          letterSpacing: 0.8,
        ),
      ),
    );
  }
}

class _PaletteGroup extends StatelessWidget {
  final _PaletteEntry entry;
  final ValueChanged<WorkflowNode> onNodeSelected;

  const _PaletteGroup({required this.entry, required this.onNodeSelected});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ExpansionTile(
      leading: Icon(entry.icon, size: 18),
      title: Text(
        entry.group,
        style: theme.textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w600),
      ),
      tilePadding: const EdgeInsets.symmetric(horizontal: 8),
      childrenPadding: const EdgeInsets.only(left: 16, bottom: 4),
      dense: true,
      visualDensity: VisualDensity.compact,
      children: [
        for (final node in entry.nodes)
          _PaletteItem(node: node, onTap: () => onNodeSelected(node)),
      ],
    );
  }
}

class _PaletteItem extends StatelessWidget {
  final WorkflowNode node;
  final VoidCallback onTap;

  const _PaletteItem({required this.node, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Row(
          children: [
            Icon(
              Icons.add_circle_outline,
              size: 14,
              color: theme.colorScheme.primary,
            ),
            const SizedBox(width: 8),
            Expanded(child: Text(node.label, style: theme.textTheme.bodySmall)),
          ],
        ),
      ),
    );
  }
}
