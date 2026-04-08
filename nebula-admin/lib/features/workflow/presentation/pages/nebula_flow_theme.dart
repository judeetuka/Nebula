import 'package:flutter/material.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';

/// Builds a heavily customized NEBULA flow theme.
NodeFlowTheme nebulaFlowTheme({
  required bool isDark,
  required ColorScheme colorScheme,
}) {
  final base = isDark ? NodeFlowTheme.dark : NodeFlowTheme.light;

  return base.copyWith(
    connectionTheme: (isDark ? ConnectionTheme.dark : ConnectionTheme.light)
        .copyWith(
          style: ConnectionStyles.bezier,
          color: isDark
              ? colorScheme.primary.withValues(alpha: 0.85)
              : colorScheme.primary,
          animationEffect: ConnectionEffects.flowingDash,
          selectedColor: colorScheme.tertiary,
        ),
    gridTheme: (isDark ? GridTheme.dark : GridTheme.light).copyWith(
      style: GridStyles.dots,
      color: isDark ? Colors.white10 : Colors.black.withValues(alpha: 0.06),
    ),
    nodeTheme: (isDark ? NodeTheme.dark : NodeTheme.light).copyWith(
      borderRadius: BorderRadius.circular(16),
      selectedBorderColor: colorScheme.primary,
      selectedBorderWidth: 2.5,
    ),
    portTheme: (isDark ? PortTheme.dark : PortTheme.light).copyWith(
      color: colorScheme.primary,
      connectedColor: colorScheme.tertiary,
    ),
  );
}
