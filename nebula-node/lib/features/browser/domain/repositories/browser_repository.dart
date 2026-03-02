import '../entities/browser_session.dart';

/// Abstract repository for browser session management.
/// Domain layer contract -- no data or presentation imports.
abstract class BrowserRepository {
  /// Create a new browser session with an initial tab.
  Future<BrowserSession> createSession({String? initialUrl});

  /// Load a persisted session from local storage.
  Future<BrowserSession?> loadSession(String sessionId);

  /// Save the current session state to local storage.
  Future<void> saveSession(BrowserSession session);

  /// Delete a session from local storage.
  Future<void> deleteSession(String sessionId);

  /// List all persisted session IDs.
  Future<List<String>> listSessionIds();

  /// Load a URL in the active tab's WebView.
  Future<void> loadUrl(String sessionId, String url);

  /// Execute JavaScript in the active tab's WebView.
  /// Returns the JS evaluation result as a string.
  Future<String?> executeJavaScript(String sessionId, String script);

  /// Extract the full HTML source of the current page.
  Future<String?> getPageContent(String sessionId);
}
