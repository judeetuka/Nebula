import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../domain/entities/browser_tab.dart';

/// Horizontal scrollable tab bar showing all open browser tabs.
/// Supports switching tabs, closing individual tabs, and adding new ones.
class BrowserTabBar extends StatelessWidget {
  final List<BrowserTab> tabs;
  final int activeIndex;
  final ValueChanged<int> onTabSelected;
  final ValueChanged<int> onTabClosed;
  final VoidCallback onAddTab;

  const BrowserTabBar({
    super.key,
    required this.tabs,
    required this.activeIndex,
    required this.onTabSelected,
    required this.onTabClosed,
    required this.onAddTab,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Container(
      height: 42,
      color: theme.colorScheme.surfaceContainerHighest,
      child: Row(
        children: [
          Expanded(
            child: ListView.builder(
              scrollDirection: Axis.horizontal,
              itemCount: tabs.length,
              padding: const EdgeInsets.symmetric(
                horizontal: UIConstants.spacingXS,
                vertical: UIConstants.spacingXS,
              ),
              itemBuilder: (context, index) {
                final tab = tabs[index];
                final isActive = index == activeIndex;

                return GestureDetector(
                  onTap: () => onTabSelected(index),
                  child: Container(
                    constraints: const BoxConstraints(
                      maxWidth: 180,
                      minWidth: 100,
                    ),
                    margin: const EdgeInsets.only(
                      right: UIConstants.spacingXS,
                    ),
                    padding: const EdgeInsets.symmetric(
                      horizontal: UIConstants.spacingSM,
                    ),
                    decoration: BoxDecoration(
                      color: isActive
                          ? theme.colorScheme.surface
                          : theme.colorScheme.surfaceContainerHighest,
                      borderRadius:
                          BorderRadius.circular(UIConstants.radiusSmall),
                      border: isActive
                          ? Border.all(
                              color: theme.colorScheme.primary
                                  .withValues(alpha: 0.3),
                            )
                          : null,
                    ),
                    child: Row(
                      children: [
                        if (tab.isLoading)
                          SizedBox(
                            width: 14,
                            height: 14,
                            child: CircularProgressIndicator(
                              strokeWidth: 1.5,
                              color: theme.colorScheme.primary,
                            ),
                          )
                        else
                          Icon(
                            Icons.public,
                            size: 14,
                            color: isActive
                                ? theme.colorScheme.primary
                                : theme.colorScheme.onSurfaceVariant,
                          ),
                        const SizedBox(width: UIConstants.spacingXS),
                        Expanded(
                          child: Text(
                            tab.title ?? _displayUrl(tab.url),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: isActive
                                  ? theme.colorScheme.onSurface
                                  : theme.colorScheme.onSurfaceVariant,
                              fontWeight: isActive
                                  ? FontWeight.w600
                                  : FontWeight.normal,
                            ),
                          ),
                        ),
                        if (tabs.length > 1)
                          GestureDetector(
                            onTap: () => onTabClosed(index),
                            child: Padding(
                              padding: const EdgeInsets.only(
                                left: UIConstants.spacingXS,
                              ),
                              child: Icon(
                                Icons.close,
                                size: 14,
                                color:
                                    theme.colorScheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                );
              },
            ),
          ),
          // Add tab button
          IconButton(
            onPressed: onAddTab,
            icon: Icon(
              Icons.add,
              size: UIConstants.iconMD,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            padding: const EdgeInsets.symmetric(
              horizontal: UIConstants.spacingSM,
            ),
            constraints: const BoxConstraints(minWidth: 36, minHeight: 36),
            tooltip: 'New tab',
          ),
        ],
      ),
    );
  }

  String _displayUrl(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null) return url;
    return uri.host.isNotEmpty ? uri.host : url;
  }
}
