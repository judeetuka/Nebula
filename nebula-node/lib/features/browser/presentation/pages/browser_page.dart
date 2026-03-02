import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';
import 'package:webview_flutter/webview_flutter.dart';

import '../../data/datasources/browser_session_source.dart';
import '../providers/browser_provider.dart';
import '../widgets/browser_controls.dart';
import '../widgets/browser_tab_bar.dart';
import '../widgets/url_bar.dart';

/// Full in-app browser page with tabs, URL bar, navigation controls,
/// and WebView rendering. The WebView controllers are held in the
/// BrowserSessionSource so they survive even when this page is not
/// visible, allowing background plugin commands.
class BrowserPage extends ConsumerStatefulWidget {
  final String? initialUrl;

  const BrowserPage({super.key, this.initialUrl});

  @override
  ConsumerState<BrowserPage> createState() => _BrowserPageState();
}

class _BrowserPageState extends ConsumerState<BrowserPage> {
  bool _initialized = false;

  @override
  void initState() {
    super.initState();
    // Initialize the browser session on first load.
    WidgetsBinding.instance.addPostFrameCallback((_) => _initBrowser());
  }

  Future<void> _initBrowser() async {
    final notifier = ref.read(browserSessionProvider.notifier);
    final session = ref.read(browserSessionProvider);

    if (session == null) {
      if (widget.initialUrl != null) {
        await notifier.createNewSession(initialUrl: widget.initialUrl);
      } else {
        await notifier.init();
      }
    }
    setState(() => _initialized = true);
  }

  void _setupNavigationDelegate(
    WebViewController controller,
    String tabId,
  ) {
    controller.setNavigationDelegate(
      NavigationDelegate(
        onPageStarted: (url) {
          ref.read(browserSessionProvider.notifier).updateActiveTab(
                url: url,
                isLoading: true,
                loadProgress: 0.0,
              );
        },
        onProgress: (progress) {
          ref.read(browserSessionProvider.notifier).updateActiveTab(
                loadProgress: progress / 100.0,
              );
        },
        onPageFinished: (url) {
          // Extract the page title.
          final source = ref.read(browserSessionSourceProvider);
          final ctrl = source.controllers[tabId];
          ctrl?.getTitle().then((title) {
            ref.read(browserSessionProvider.notifier).updateActiveTab(
                  title: title,
                  isLoading: false,
                  loadProgress: 1.0,
                );
          });
        },
        onWebResourceError: (error) {
          if (error.isForMainFrame == true) {
            ref.read(browserSessionProvider.notifier).updateActiveTab(
                  isLoading: false,
                );
          }
        },
      ),
    );
  }

  void _showBrowserMenu() {
    final theme = Theme.of(context);

    showModalBottomSheet(
      context: context,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) => Container(
        padding: UIConstants.paddingLG,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Drag handle
            Container(
              width: 40,
              height: 4,
              decoration: BoxDecoration(
                color: theme.colorScheme.onSurface.withValues(alpha: 0.2),
                borderRadius: BorderRadius.circular(2),
              ),
            ),
            const SizedBox(height: UIConstants.spacingLG),
            _MenuItem(
              icon: Icons.code,
              label: 'View Page Source',
              onTap: () {
                Navigator.pop(context);
                _viewPageSource();
              },
            ),
            _MenuItem(
              icon: Icons.javascript,
              label: 'Run JavaScript',
              onTap: () {
                Navigator.pop(context);
                _showJsConsole();
              },
            ),
            _MenuItem(
              icon: Icons.save_alt,
              label: 'Save Session',
              onTap: () {
                Navigator.pop(context);
                ref.read(browserSessionProvider.notifier).saveSession();
                NotificationToast.success(context, 'Session saved');
              },
            ),
            _MenuItem(
              icon: Icons.delete_outline,
              label: 'Clear Session',
              onTap: () {
                Navigator.pop(context);
                _clearSession();
              },
            ),
            const SizedBox(height: UIConstants.spacingLG),
          ],
        ),
      ),
    );
  }

  Future<void> _viewPageSource() async {
    final content =
        await ref.read(browserSessionProvider.notifier).getPageContent();
    if (!mounted) return;

    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) => DraggableScrollableSheet(
        expand: false,
        initialChildSize: 0.7,
        maxChildSize: 0.95,
        builder: (context, scrollController) => Padding(
          padding: UIConstants.paddingLG,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Center(
                child: Container(
                  width: 40,
                  height: 4,
                  decoration: BoxDecoration(
                    color: Theme.of(context)
                        .colorScheme
                        .onSurface
                        .withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(2),
                  ),
                ),
              ),
              const SizedBox(height: UIConstants.spacingLG),
              Text(
                'Page Source',
                style: Theme.of(context).textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w600,
                    ),
              ),
              const SizedBox(height: UIConstants.spacingSM),
              Expanded(
                child: SingleChildScrollView(
                  controller: scrollController,
                  child: SelectableText(
                    content ?? 'Unable to retrieve page content.',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          fontFamily: 'monospace',
                          fontSize: 11,
                        ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _showJsConsole() async {
    AppAlertDialog.showWithInput(
      context: context,
      title: 'Execute JavaScript',
      message: 'Enter JavaScript code to run in the current page.',
      hintText: 'document.title',
      actionText: 'Run',
      multiLine: true,
      onActionPressed: (script) async {
        final result = await ref
            .read(browserSessionProvider.notifier)
            .executeJavaScript(script);
        if (!mounted) return;
        NotificationToast.info(
          context,
          'Result: ${result ?? "null"}',
          duration: const Duration(seconds: 5),
        );
      },
    );
  }

  void _clearSession() {
    AppAlertDialog.showDanger(
      context: context,
      title: 'Clear Session?',
      message:
          'This will close all tabs and clear browsing data. This cannot be undone.',
      actionText: 'Clear',
      onActionPressed: () async {
        final session = ref.read(browserSessionProvider);
        if (session != null) {
          await ref
              .read(browserRepositoryProvider)
              .deleteSession(session.id);
        }
        await ref
            .read(browserSessionProvider.notifier)
            .createNewSession();
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(browserSessionProvider);

    if (!_initialized || session == null) {
      return Scaffold(
        appBar: AppBar(title: const Text('Browser')),
        body: const Center(child: CircularProgressIndicator()),
      );
    }

    final activeTab = session.activeTab;
    final source = ref.watch(browserSessionSourceProvider);

    // Ensure the active tab's controller has navigation delegates wired up.
    if (activeTab != null) {
      final controller = source.controllers[activeTab.id];
      if (controller != null) {
        _setupNavigationDelegate(controller, activeTab.id);
      }
    }

    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            // Tab bar
            BrowserTabBar(
              tabs: session.tabs,
              activeIndex: session.activeTabIndex,
              onTabSelected: (index) {
                ref.read(browserSessionProvider.notifier).switchTab(index);
              },
              onTabClosed: (index) {
                ref.read(browserSessionProvider.notifier).closeTab(index);
              },
              onAddTab: () {
                ref.read(browserSessionProvider.notifier).addTab();
              },
            ),

            // URL bar
            UrlBar(
              currentUrl: activeTab?.url ?? '',
              isLoading: activeTab?.isLoading ?? false,
              loadProgress: activeTab?.loadProgress ?? 0.0,
              onUrlSubmitted: (url) {
                ref.read(browserSessionProvider.notifier).loadUrl(url);
              },
              onStopLoading: () {
                // Stop loading by reloading (WebView API does not expose stop).
                ref.read(browserSessionProvider.notifier).reload();
              },
            ),

            // WebView content
            Expanded(
              child: _buildWebView(source, activeTab?.id),
            ),

            // Bottom controls
            BrowserControls(
              canGoBack: activeTab?.canGoBack ?? false,
              canGoForward: activeTab?.canGoForward ?? false,
              isLoading: activeTab?.isLoading ?? false,
              tabCount: session.tabs.length,
              onBack: () {
                ref.read(browserSessionProvider.notifier).goBack();
              },
              onForward: () {
                ref.read(browserSessionProvider.notifier).goForward();
              },
              onReload: () {
                ref.read(browserSessionProvider.notifier).reload();
              },
              onShowTabs: () {
                // Scroll to top of tab bar (already visible).
                // Could open a grid view of tabs in the future.
              },
              onMenu: _showBrowserMenu,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildWebView(BrowserSessionSource source, String? tabId) {
    if (tabId == null) {
      return const Center(child: Text('No tab open'));
    }

    final controller = source.controllers[tabId];
    if (controller == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return WebViewWidget(controller: controller);
  }
}

/// Simple menu item widget used in the browser bottom sheet menu.
class _MenuItem extends StatelessWidget {
  final IconData icon;
  final String label;
  final VoidCallback onTap;

  const _MenuItem({
    required this.icon,
    required this.label,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(UIConstants.radiusMedium),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          vertical: UIConstants.spacingMD,
          horizontal: UIConstants.spacingSM,
        ),
        child: Row(
          children: [
            Icon(
              icon,
              size: UIConstants.iconLG,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            const SizedBox(width: UIConstants.spacingLG),
            Text(
              label,
              style: theme.textTheme.bodyLarge,
            ),
          ],
        ),
      ),
    );
  }
}
