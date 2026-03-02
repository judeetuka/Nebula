import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/datasources/browser_session_source.dart';
import '../../data/repositories/browser_repository_impl.dart';
import '../../domain/entities/browser_session.dart';
import '../../domain/entities/browser_tab.dart';
import '../../domain/repositories/browser_repository.dart';

// -- Infrastructure providers --

final browserSessionSourceProvider = Provider<BrowserSessionSource>((ref) {
  final source = BrowserSessionSource();
  ref.onDispose(source.disposeAll);
  return source;
});

final browserRepositoryProvider = Provider<BrowserRepository>((ref) {
  final source = ref.watch(browserSessionSourceProvider);
  return BrowserRepositoryImpl(source);
});

// -- State notifier for the active browser session --

final browserSessionProvider =
    StateNotifierProvider<BrowserSessionNotifier, BrowserSession?>((ref) {
  final source = ref.watch(browserSessionSourceProvider);
  final repository = ref.watch(browserRepositoryProvider);
  return BrowserSessionNotifier(source: source, repository: repository);
});

class BrowserSessionNotifier extends StateNotifier<BrowserSession?> {
  final BrowserSessionSource source;
  final BrowserRepository repository;

  BrowserSessionNotifier({
    required this.source,
    required this.repository,
  }) : super(null);

  /// Initialize: try to load the last session, or create a new one.
  Future<void> init() async {
    final ids = await repository.listSessionIds();
    if (ids.isNotEmpty) {
      final session = await repository.loadSession(ids.last);
      if (session != null) {
        state = session;
        return;
      }
    }
    await createNewSession();
  }

  /// Create a brand new session.
  Future<void> createNewSession({String? initialUrl}) async {
    final session =
        await repository.createSession(initialUrl: initialUrl);
    state = session;
  }

  /// Navigate the active tab to a URL.
  Future<void> loadUrl(String url) async {
    final session = state;
    if (session == null) return;

    final activeTab = session.activeTab;
    if (activeTab == null) return;

    final updatedTab = activeTab.navigateTo(url);
    final updatedSession =
        session.updateTab(session.activeTabIndex, updatedTab);
    state = updatedSession;

    await source.loadUrl(activeTab.id, url);
    await repository.saveSession(updatedSession);
  }

  /// Update the active tab's metadata (title, loading state, progress).
  void updateActiveTab({
    String? title,
    String? url,
    bool? isLoading,
    double? loadProgress,
  }) {
    final session = state;
    if (session == null) return;

    final activeTab = session.activeTab;
    if (activeTab == null) return;

    var updatedTab = activeTab.copyWith(
      title: title,
      isLoading: isLoading,
      loadProgress: loadProgress,
    );

    // If we got a new URL from the WebView (page navigation), record it in history.
    if (url != null && url != activeTab.url) {
      updatedTab = updatedTab.navigateTo(url);
    }

    final updatedSession =
        session.updateTab(session.activeTabIndex, updatedTab);
    state = updatedSession;
    source.updateSession(updatedSession);
  }

  /// Go back in the active tab.
  Future<void> goBack() async {
    final session = state;
    if (session == null) return;

    final activeTab = session.activeTab;
    if (activeTab == null || !activeTab.canGoBack) return;

    await source.goBack(activeTab.id);
    final updatedTab = activeTab.goBack();
    state = session.updateTab(session.activeTabIndex, updatedTab);
  }

  /// Go forward in the active tab.
  Future<void> goForward() async {
    final session = state;
    if (session == null) return;

    final activeTab = session.activeTab;
    if (activeTab == null || !activeTab.canGoForward) return;

    await source.goForward(activeTab.id);
    final updatedTab = activeTab.goForward();
    state = session.updateTab(session.activeTabIndex, updatedTab);
  }

  /// Reload the active tab.
  Future<void> reload() async {
    final session = state;
    if (session == null) return;

    final activeTab = session.activeTab;
    if (activeTab == null) return;

    await source.reload(activeTab.id);
    final updatedTab = activeTab.copyWith(isLoading: true, loadProgress: 0.0);
    state = session.updateTab(session.activeTabIndex, updatedTab);
  }

  /// Add a new tab and switch to it.
  Future<void> addTab({String? url}) async {
    final session = state;
    if (session == null) return;

    final tabUrl = url ?? 'https://www.google.com';
    final tabId = DateTime.now().microsecondsSinceEpoch.toRadixString(36);
    final tab = BrowserTab(
      id: tabId,
      url: tabUrl,
      history: [tabUrl],
      historyIndex: 0,
    );

    final controller = source.getOrCreateController(tabId);
    controller.loadRequest(Uri.parse(tabUrl));

    final updatedSession = session.addTab(tab);
    state = updatedSession;
    await repository.saveSession(updatedSession);
  }

  /// Close a tab by index.
  Future<void> closeTab(int index) async {
    final session = state;
    if (session == null || session.tabs.length <= 1) return;

    final tabId = session.tabs[index].id;
    source.removeController(tabId);

    final updatedSession = session.closeTab(index);
    state = updatedSession;
    await repository.saveSession(updatedSession);
  }

  /// Switch to a tab by index.
  void switchTab(int index) {
    final session = state;
    if (session == null) return;
    if (index < 0 || index >= session.tabs.length) return;

    state = session.copyWith(activeTabIndex: index);
  }

  /// Execute JavaScript in the active tab. Returns the result.
  Future<String?> executeJavaScript(String script) async {
    final session = state;
    if (session == null) return null;

    final activeTab = session.activeTab;
    if (activeTab == null) return null;

    return source.executeJavaScript(activeTab.id, script);
  }

  /// Get the HTML source of the active tab's page.
  Future<String?> getPageContent() async {
    final session = state;
    if (session == null) return null;

    final activeTab = session.activeTab;
    if (activeTab == null) return null;

    return source.getPageContent(activeTab.id);
  }

  /// Save the current session to persistent storage.
  Future<void> saveSession() async {
    final session = state;
    if (session == null) return;
    await repository.saveSession(session);
  }
}
