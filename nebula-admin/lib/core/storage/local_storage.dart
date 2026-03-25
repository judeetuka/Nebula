import 'package:flutter/foundation.dart';
import 'package:hive_ce_flutter/hive_ce_flutter.dart';

/// Hive-backed local storage for persistent key-value data.
///
/// Three boxes partition data by concern:
///   - **settings**: server URL, theme mode
///   - **auth**: JWT token, cached user info
///   - **cache**: cluster data for offline access
class LocalStorage {
  static const String _settingsBox = 'settings';
  static const String _authBox = 'auth';
  static const String _cacheBox = 'cache';

  // --- Settings keys ---
  static const String keyServerUrl = 'server_url';
  static const String keyThemeMode = 'theme_mode';

  // --- Auth keys ---
  static const String keyJwtToken = 'jwt_token';
  static const String keyUserUid = 'user_uid';
  static const String keyUserEmail = 'user_email';
  static const String keyUserDisplayName = 'user_display_name';
  static const String keyUserRole = 'user_role';

  late Box<dynamic> _settings;
  late Box<dynamic> _auth;
  late Box<dynamic> _cache;

  /// Initialize Hive and open all boxes. Call once in main() before runApp.
  Future<void> init() async {
    await Hive.initFlutter();
    _settings = await Hive.openBox(_settingsBox);
    _auth = await Hive.openBox(_authBox);
    _cache = await Hive.openBox(_cacheBox);
    debugPrint('LocalStorage initialized');
  }

  // --- Generic accessors ---

  T? get<T>(String box, String key) {
    return _boxFor(box).get(key) as T?;
  }

  Future<void> set<T>(String box, String key, T value) {
    return _boxFor(box).put(key, value);
  }

  Future<void> delete(String box, String key) {
    return _boxFor(box).delete(key);
  }

  Future<void> clearBox(String box) {
    return _boxFor(box).clear();
  }

  Box<dynamic> _boxFor(String name) {
    switch (name) {
      case _settingsBox:
        return _settings;
      case _authBox:
        return _auth;
      case _cacheBox:
        return _cache;
      default:
        throw ArgumentError('Unknown box: $name');
    }
  }

  // --- Convenience: Settings ---

  String get serverUrl =>
      _settings.get(keyServerUrl, defaultValue: 'http://localhost:8080')
          as String;

  Future<void> setServerUrl(String url) => _settings.put(keyServerUrl, url);

  String get themeMode =>
      _settings.get(keyThemeMode, defaultValue: 'system') as String;

  Future<void> setThemeMode(String mode) => _settings.put(keyThemeMode, mode);

  // --- Convenience: Auth ---

  String? get jwtToken => _auth.get(keyJwtToken) as String?;

  Future<void> setJwtToken(String token) => _auth.put(keyJwtToken, token);

  Future<void> clearAuth() => _auth.clear();

  // --- Convenience: Cache ---

  Future<void> cacheJson(String key, dynamic json) => _cache.put(key, json);

  dynamic getCachedJson(String key) => _cache.get(key);

  Future<void> clearCache() => _cache.clear();

  // Box name constants for external use
  static const String settingsBoxName = _settingsBox;
  static const String authBoxName = _authBox;
  static const String cacheBoxName = _cacheBox;
}
