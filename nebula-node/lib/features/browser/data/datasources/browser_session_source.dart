import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:webview_flutter/webview_flutter.dart';

import '../../domain/entities/browser_session.dart';
import '../../domain/entities/browser_tab.dart';

/// Manages WebView sessions in memory and persists session metadata
/// to SharedPreferences. WebView controllers are held in memory so
/// plugins can command them even when the browser page is not visible.
class BrowserSessionSource {
  static const _sessionKeyPrefix = 'nebula_browser_session_';
  static const _sessionListKey = 'nebula_browser_session_ids';

  /// In-memory WebView controllers keyed by tab ID.
  /// These stay alive so background plugin commands can reach the WebView.
  final Map<String, WebViewController> _controllers = {};

  /// In-memory session state. Single session for now, expandable later.
  BrowserSession? _currentSession;

  BrowserSession? get currentSession => _currentSession;
  Map<String, WebViewController> get controllers =>
      Map.unmodifiable(_controllers);

  /// Get or create the WebViewController for a specific tab.
  WebViewController getOrCreateController(String tabId) {
    return _controllers.putIfAbsent(tabId, () {
      final controller = WebViewController()
        ..setJavaScriptMode(JavaScriptMode.unrestricted);
      return controller;
    });
  }

  /// Remove a controller when a tab is closed.
  void removeController(String tabId) {
    _controllers.remove(tabId);
  }

  /// Create a new session with a default tab.
  BrowserSession createSession({String? initialUrl}) {
    final tabId = _generateId();
    final url = initialUrl ?? 'https://www.google.com';
    final tab = BrowserTab(
      id: tabId,
      url: url,
      history: [url],
      historyIndex: 0,
    );

    final session = BrowserSession(
      id: _generateId(),
      tabs: [tab],
      activeTabIndex: 0,
      createdAt: DateTime.now(),
    );

    _currentSession = session;

    // Create the controller and load the initial URL.
    final controller = getOrCreateController(tabId);
    controller.loadRequest(Uri.parse(url));

    return session;
  }

  /// Update the in-memory session state.
  void updateSession(BrowserSession session) {
    _currentSession = session;
  }

  /// Load a URL in the specified tab's WebView controller.
  Future<void> loadUrl(String tabId, String url) async {
    final controller = _controllers[tabId];
    if (controller != null) {
      await controller.loadRequest(Uri.parse(url));
    }
  }

  /// Execute JavaScript in the specified tab's WebView controller.
  Future<String?> executeJavaScript(String tabId, String script) async {
    final controller = _controllers[tabId];
    if (controller == null) return null;
    final result = await controller.runJavaScriptReturningResult(script);
    return result.toString();
  }

  /// Get the full HTML source of the current page in the specified tab.
  Future<String?> getPageContent(String tabId) async {
    return executeJavaScript(
      tabId,
      'document.documentElement.outerHTML',
    );
  }

  /// Navigate back in the specified tab's WebView.
  Future<void> goBack(String tabId) async {
    final controller = _controllers[tabId];
    if (controller != null && await controller.canGoBack()) {
      await controller.goBack();
    }
  }

  /// Navigate forward in the specified tab's WebView.
  Future<void> goForward(String tabId) async {
    final controller = _controllers[tabId];
    if (controller != null && await controller.canGoForward()) {
      await controller.goForward();
    }
  }

  /// Reload the current page in the specified tab.
  Future<void> reload(String tabId) async {
    final controller = _controllers[tabId];
    if (controller != null) {
      await controller.reload();
    }
  }

  // -- Persistence via SharedPreferences --

  /// Save a session to SharedPreferences.
  Future<void> saveSession(BrowserSession session) async {
    final prefs = await SharedPreferences.getInstance();
    final key = '$_sessionKeyPrefix${session.id}';
    final json = jsonEncode(session.toJson());
    await prefs.setString(key, json);

    // Update the session ID list.
    final ids = prefs.getStringList(_sessionListKey) ?? [];
    if (!ids.contains(session.id)) {
      ids.add(session.id);
      await prefs.setStringList(_sessionListKey, ids);
    }
  }

  /// Load a session from SharedPreferences.
  Future<BrowserSession?> loadSession(String sessionId) async {
    final prefs = await SharedPreferences.getInstance();
    final key = '$_sessionKeyPrefix$sessionId';
    final json = prefs.getString(key);
    if (json == null) return null;

    final session =
        BrowserSession.fromJson(jsonDecode(json) as Map<String, dynamic>);
    _currentSession = session;

    // Restore WebView controllers for each tab.
    for (final tab in session.tabs) {
      final controller = getOrCreateController(tab.id);
      controller.loadRequest(Uri.parse(tab.url));
    }

    return session;
  }

  /// Delete a session from SharedPreferences.
  Future<void> deleteSession(String sessionId) async {
    final prefs = await SharedPreferences.getInstance();
    final key = '$_sessionKeyPrefix$sessionId';
    await prefs.remove(key);

    final ids = prefs.getStringList(_sessionListKey) ?? [];
    ids.remove(sessionId);
    await prefs.setStringList(_sessionListKey, ids);

    if (_currentSession?.id == sessionId) {
      // Clean up controllers for this session.
      for (final tab in _currentSession!.tabs) {
        removeController(tab.id);
      }
      _currentSession = null;
    }
  }

  /// List all persisted session IDs.
  Future<List<String>> listSessionIds() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getStringList(_sessionListKey) ?? [];
  }

  /// Dispose all controllers. Call when the feature is torn down.
  void disposeAll() {
    _controllers.clear();
    _currentSession = null;
  }

  String _generateId() {
    return DateTime.now().microsecondsSinceEpoch.toRadixString(36) +
        UniqueKey().toString().hashCode.toRadixString(36);
  }
}
