/// Represents a single browser tab with its URL, title, loading state,
/// and navigation history for back/forward functionality.
class BrowserTab {
  final String id;
  final String url;
  final String? title;
  final bool isLoading;
  final double loadProgress;
  final List<String> history;
  final int historyIndex;

  const BrowserTab({
    required this.id,
    required this.url,
    this.title,
    this.isLoading = false,
    this.loadProgress = 0.0,
    this.history = const [],
    this.historyIndex = -1,
  });

  bool get canGoBack => historyIndex > 0;
  bool get canGoForward => historyIndex < history.length - 1;

  String? get currentHistoryUrl =>
      historyIndex >= 0 && historyIndex < history.length
          ? history[historyIndex]
          : null;

  BrowserTab copyWith({
    String? id,
    String? url,
    String? title,
    bool? isLoading,
    double? loadProgress,
    List<String>? history,
    int? historyIndex,
  }) {
    return BrowserTab(
      id: id ?? this.id,
      url: url ?? this.url,
      title: title ?? this.title,
      isLoading: isLoading ?? this.isLoading,
      loadProgress: loadProgress ?? this.loadProgress,
      history: history ?? this.history,
      historyIndex: historyIndex ?? this.historyIndex,
    );
  }

  /// Navigate to a new URL, truncating forward history.
  BrowserTab navigateTo(String newUrl) {
    final truncatedHistory = historyIndex >= 0
        ? history.sublist(0, historyIndex + 1)
        : <String>[];
    final updatedHistory = [...truncatedHistory, newUrl];
    return copyWith(
      url: newUrl,
      history: updatedHistory,
      historyIndex: updatedHistory.length - 1,
      isLoading: true,
      loadProgress: 0.0,
    );
  }

  /// Go back in history.
  BrowserTab goBack() {
    if (!canGoBack) return this;
    final newIndex = historyIndex - 1;
    return copyWith(
      url: history[newIndex],
      historyIndex: newIndex,
      isLoading: true,
      loadProgress: 0.0,
    );
  }

  /// Go forward in history.
  BrowserTab goForward() {
    if (!canGoForward) return this;
    final newIndex = historyIndex + 1;
    return copyWith(
      url: history[newIndex],
      historyIndex: newIndex,
      isLoading: true,
      loadProgress: 0.0,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'url': url,
      'title': title,
      'history': history,
      'history_index': historyIndex,
    };
  }

  factory BrowserTab.fromJson(Map<String, dynamic> json) {
    return BrowserTab(
      id: json['id'] as String,
      url: json['url'] as String,
      title: json['title'] as String?,
      history: (json['history'] as List<dynamic>).cast<String>(),
      historyIndex: json['history_index'] as int,
    );
  }
}
