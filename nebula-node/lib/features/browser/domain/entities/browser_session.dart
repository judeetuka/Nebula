import 'browser_tab.dart';

/// Represents a browser session containing multiple tabs,
/// cookies, and session metadata. Sessions persist across
/// app restarts via local storage.
class BrowserSession {
  final String id;
  final List<BrowserTab> tabs;
  final int activeTabIndex;
  final Map<String, String> cookies;
  final DateTime createdAt;

  const BrowserSession({
    required this.id,
    required this.tabs,
    this.activeTabIndex = 0,
    this.cookies = const {},
    required this.createdAt,
  });

  BrowserTab? get activeTab =>
      activeTabIndex >= 0 && activeTabIndex < tabs.length
          ? tabs[activeTabIndex]
          : null;

  BrowserSession copyWith({
    String? id,
    List<BrowserTab>? tabs,
    int? activeTabIndex,
    Map<String, String>? cookies,
    DateTime? createdAt,
  }) {
    return BrowserSession(
      id: id ?? this.id,
      tabs: tabs ?? this.tabs,
      activeTabIndex: activeTabIndex ?? this.activeTabIndex,
      cookies: cookies ?? this.cookies,
      createdAt: createdAt ?? this.createdAt,
    );
  }

  /// Add a new tab and make it the active tab.
  BrowserSession addTab(BrowserTab tab) {
    final updatedTabs = [...tabs, tab];
    return copyWith(
      tabs: updatedTabs,
      activeTabIndex: updatedTabs.length - 1,
    );
  }

  /// Close a tab by index. If the active tab is closed,
  /// the previous tab (or the next one if first) becomes active.
  BrowserSession closeTab(int index) {
    if (tabs.length <= 1) return this;
    final updatedTabs = [...tabs]..removeAt(index);
    int newActiveIndex = activeTabIndex;
    if (index == activeTabIndex) {
      newActiveIndex = index > 0 ? index - 1 : 0;
    } else if (index < activeTabIndex) {
      newActiveIndex = activeTabIndex - 1;
    }
    return copyWith(
      tabs: updatedTabs,
      activeTabIndex: newActiveIndex.clamp(0, updatedTabs.length - 1),
    );
  }

  /// Update a specific tab.
  BrowserSession updateTab(int index, BrowserTab tab) {
    final updatedTabs = [...tabs];
    updatedTabs[index] = tab;
    return copyWith(tabs: updatedTabs);
  }

  /// Set a cookie for a domain.
  BrowserSession setCookie(String domain, String cookieString) {
    final updatedCookies = Map<String, String>.from(cookies);
    updatedCookies[domain] = cookieString;
    return copyWith(cookies: updatedCookies);
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'tabs': tabs.map((t) => t.toJson()).toList(),
      'active_tab_index': activeTabIndex,
      'cookies': cookies,
      'created_at': createdAt.toIso8601String(),
    };
  }

  factory BrowserSession.fromJson(Map<String, dynamic> json) {
    return BrowserSession(
      id: json['id'] as String,
      tabs: (json['tabs'] as List<dynamic>)
          .map((t) => BrowserTab.fromJson(t as Map<String, dynamic>))
          .toList(),
      activeTabIndex: json['active_tab_index'] as int,
      cookies: Map<String, String>.from(json['cookies'] as Map),
      createdAt: DateTime.parse(json['created_at'] as String),
    );
  }
}
