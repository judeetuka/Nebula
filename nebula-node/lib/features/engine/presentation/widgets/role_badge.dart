import 'package:flutter/material.dart';
import 'package:nebula_ui/nebula_ui.dart';

class RoleBadge extends StatelessWidget {
  final String state;

  const RoleBadge({super.key, required this.state});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final (icon, color, label) = _resolveState();

    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: UIConstants.spacingLG,
        vertical: UIConstants.spacingSM,
      ),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(UIConstants.radiusLarge),
        border: Border.all(color: color.withValues(alpha: 0.4)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, color: color, size: UIConstants.iconMD),
          const SizedBox(width: UIConstants.spacingSM),
          Text(
            label,
            style: theme.textTheme.labelLarge?.copyWith(
              color: color,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }

  (IconData, Color, String) _resolveState() {
    return switch (state) {
      'active' => (Icons.bolt, Colors.green, 'Active'),
      'configured' => (Icons.settings, Colors.blue, 'Configured'),
      'idle' => (Icons.hourglass_empty, Colors.orange, 'Idle'),
      'uninitialized' => (Icons.warning_amber, Colors.red, 'Uninitialized'),
      _ => (Icons.help_outline, Colors.grey, state),
    };
  }
}
