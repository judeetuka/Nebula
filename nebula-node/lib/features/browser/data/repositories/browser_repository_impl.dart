import '../../domain/entities/browser_session.dart';
import '../../domain/repositories/browser_repository.dart';
import '../datasources/browser_session_source.dart';

class BrowserRepositoryImpl implements BrowserRepository {
  final BrowserSessionSource sessionSource;

  BrowserRepositoryImpl(this.sessionSource);

  @override
  Future<BrowserSession> createSession({String? initialUrl}) async {
    final session = sessionSource.createSession(initialUrl: initialUrl);
    await sessionSource.saveSession(session);
    return session;
  }

  @override
  Future<BrowserSession?> loadSession(String sessionId) {
    return sessionSource.loadSession(sessionId);
  }

  @override
  Future<void> saveSession(BrowserSession session) {
    return sessionSource.saveSession(session);
  }

  @override
  Future<void> deleteSession(String sessionId) {
    return sessionSource.deleteSession(sessionId);
  }

  @override
  Future<List<String>> listSessionIds() {
    return sessionSource.listSessionIds();
  }

  @override
  Future<void> loadUrl(String sessionId, String url) async {
    final session = sessionSource.currentSession;
    if (session == null || session.id != sessionId) return;

    final activeTab = session.activeTab;
    if (activeTab == null) return;

    final updatedTab = activeTab.navigateTo(url);
    final updatedSession =
        session.updateTab(session.activeTabIndex, updatedTab);
    sessionSource.updateSession(updatedSession);
    await sessionSource.loadUrl(activeTab.id, url);
    await sessionSource.saveSession(updatedSession);
  }

  @override
  Future<String?> executeJavaScript(String sessionId, String script) {
    final session = sessionSource.currentSession;
    if (session == null || session.id != sessionId) return Future.value();

    final activeTab = session.activeTab;
    if (activeTab == null) return Future.value();

    return sessionSource.executeJavaScript(activeTab.id, script);
  }

  @override
  Future<String?> getPageContent(String sessionId) {
    final session = sessionSource.currentSession;
    if (session == null || session.id != sessionId) return Future.value();

    final activeTab = session.activeTab;
    if (activeTab == null) return Future.value();

    return sessionSource.getPageContent(activeTab.id);
  }
}
