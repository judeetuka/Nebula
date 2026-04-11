import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

/// Bottom navigation controls for the browser: back, forward, refresh, tabs, menu.
class BrowserControls extends StatelessWidget {
  final bool canGoBack;
  final bool canGoForward;
  final bool isLoading;
  final int tabCount;
  final VoidCallback onBack;
  final VoidCallback onForward;
  final VoidCallback onReload;
  final VoidCallback onShowTabs;
  final VoidCallback onMenu;

  const BrowserControls({
    super.key,
    required this.canGoBack,
    required this.canGoForward,
    required this.isLoading,
    required this.tabCount,
    required this.onBack,
    required this.onForward,
    required this.onReload,
    required this.onShowTabs,
    required this.onMenu,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final iconColor = theme.colorScheme.onSurface;
    final disabledColor =
        theme.colorScheme.onSurface.withValues(alpha: 0.3);

    return Container(
      height: 48,
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(
          top: BorderSide(
            color: theme.colorScheme.outlineVariant,
            width: 0.5,
          ),
        ),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceEvenly,
        children: [
          // Back
          IconButton(
            onPressed: canGoBack ? onBack : null,
            icon: Icon(
              Icons.arrow_back_ios_new,
              size: UIConstants.iconMD,
              color: canGoBack ? iconColor : disabledColor,
            ),
            tooltip: 'Back',
          ),

          // Forward
          IconButton(
            onPressed: canGoForward ? onForward : null,
            icon: Icon(
              Icons.arrow_forward_ios,
              size: UIConstants.iconMD,
              color: canGoForward ? iconColor : disabledColor,
            ),
            tooltip: 'Forward',
          ),

          // Reload / Stop
          IconButton(
            onPressed: onReload,
            icon: Icon(
              isLoading ? Icons.close : Icons.refresh,
              size: UIConstants.iconLG,
              color: iconColor,
            ),
            tooltip: isLoading ? 'Stop' : 'Reload',
          ),

          // Tab count indicator
          GestureDetector(
            onTap: onShowTabs,
            child: Container(
              width: 28,
              height: 28,
              decoration: BoxDecoration(
                borderRadius:
                    BorderRadius.circular(UIConstants.radiusSmall),
                border: Border.all(
                  color: iconColor,
                  width: 1.5,
                ),
              ),
              child: Center(
                child: Text(
                  '$tabCount',
                  style: theme.textTheme.labelSmall?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: iconColor,
                  ),
                ),
              ),
            ),
          ),

          // Menu
          IconButton(
            onPressed: onMenu,
            icon: Icon(
              Icons.more_horiz,
              size: UIConstants.iconLG,
              color: iconColor,
            ),
            tooltip: 'Menu',
          ),
        ],
      ),
    );
  }
}
