import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

class NodeMetricsCard extends StatelessWidget {
  final String nodeId;
  final String? clusterId;
  final String state;
  final bool isConfigured;

  const NodeMetricsCard({
    super.key,
    required this.nodeId,
    this.clusterId,
    required this.state,
    required this.isConfigured,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Card(
      child: Padding(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Node Information',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),
            _MetricRow(label: 'Node ID', value: _truncateId(nodeId)),
            const Divider(height: UIConstants.spacingLG),
            _MetricRow(
              label: 'Cluster ID',
              value: clusterId != null
                  ? _truncateId(clusterId!)
                  : 'Not joined',
            ),
            const Divider(height: UIConstants.spacingLG),
            _MetricRow(label: 'State', value: state),
            const Divider(height: UIConstants.spacingLG),
            _MetricRow(
              label: 'Configured',
              value: isConfigured ? 'Yes' : 'No',
            ),
          ],
        ),
      ),
    );
  }

  String _truncateId(String id) {
    if (id.length <= 16) return id;
    return '${id.substring(0, 8)}...${id.substring(id.length - 8)}';
  }
}

class _MetricRow extends StatelessWidget {
  final String label;
  final String value;

  const _MetricRow({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Text(
          label,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        Flexible(
          child: Text(
            value,
            style: theme.textTheme.bodyMedium?.copyWith(
              fontWeight: FontWeight.w500,
            ),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }
}
