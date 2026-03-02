import 'package:flutter/material.dart';
import 'package:nebula_ui/nebula_ui.dart';

class QrDisplay extends StatelessWidget {
  final String clusterId;

  const QrDisplay({super.key, required this.clusterId});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Container(
      width: 200,
      height: 200,
      decoration: BoxDecoration(
        border: Border.all(
          color: theme.colorScheme.outlineVariant,
          width: 2,
        ),
        borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
        color: theme.colorScheme.surface,
      ),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.qr_code_2,
            size: 80,
            color: theme.colorScheme.onSurfaceVariant,
          ),
          const SizedBox(height: UIConstants.spacingSM),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: UIConstants.spacingSM),
            child: Text(
              'QR Code for $clusterId',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.outline,
              ),
              textAlign: TextAlign.center,
            ),
          ),
        ],
      ),
    );
  }
}
