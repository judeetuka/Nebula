@TestOn('vm')
library;

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:hive_ce/hive_ce.dart';

import 'package:nebula_admin/core/storage/local_storage.dart';

void main() {
  late LocalStorage storage;
  late Directory tempDir;

  setUp(() async {
    TestWidgetsFlutterBinding.ensureInitialized();
    tempDir = await Directory.systemTemp.createTemp('nebula_hive_test_');

    // Mock the path_provider platform channel so Hive.initFlutter() gets
    // our temp directory instead of trying to reach the real plugin.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('plugins.flutter.io/path_provider'),
          (MethodCall methodCall) async {
            if (methodCall.method == 'getApplicationDocumentsDirectory') {
              return tempDir.path;
            }
            return null;
          },
        );

    storage = LocalStorage();
    await storage.init();
  });

  tearDown(() async {
    await Hive.close();

    // Remove the mock.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('plugins.flutter.io/path_provider'),
          null,
        );

    if (tempDir.existsSync()) {
      tempDir.deleteSync(recursive: true);
    }
  });

  group('JWT token', () {
    test('stores and retrieves JWT token', () async {
      expect(storage.jwtToken, isNull);

      await storage.setJwtToken('eyJhbGciOiJIUzI1NiJ9.test');
      expect(storage.jwtToken, 'eyJhbGciOiJIUzI1NiJ9.test');
    });

    test('clearAuth removes token', () async {
      await storage.setJwtToken('some-token');
      expect(storage.jwtToken, isNotNull);

      await storage.clearAuth();
      expect(storage.jwtToken, isNull);
    });
  });

  group('Server URL', () {
    test('returns default when nothing stored', () {
      expect(storage.serverUrl, 'http://localhost:8080');
    });

    test('stores and retrieves server URL', () async {
      await storage.setServerUrl('https://nebula.example.com');
      expect(storage.serverUrl, 'https://nebula.example.com');
    });
  });

  group('Theme mode', () {
    test('returns default "system" when nothing stored', () {
      expect(storage.themeMode, 'system');
    });

    test('stores and retrieves theme mode', () async {
      await storage.setThemeMode('dark');
      expect(storage.themeMode, 'dark');
    });
  });

  group('Generic accessors', () {
    test('set and get on settings box', () async {
      await storage.set(LocalStorage.settingsBoxName, 'custom_key', 42);
      final value = storage.get<int>(
        LocalStorage.settingsBoxName,
        'custom_key',
      );
      expect(value, 42);
    });

    test('delete removes a key', () async {
      await storage.set(LocalStorage.authBoxName, 'temp', 'val');
      expect(storage.get<String>(LocalStorage.authBoxName, 'temp'), 'val');

      await storage.delete(LocalStorage.authBoxName, 'temp');
      expect(storage.get<String>(LocalStorage.authBoxName, 'temp'), isNull);
    });

    test('clearBox removes all data from a box', () async {
      await storage.set(LocalStorage.cacheBoxName, 'a', 1);
      await storage.set(LocalStorage.cacheBoxName, 'b', 2);

      await storage.clearBox(LocalStorage.cacheBoxName);

      expect(storage.get<int>(LocalStorage.cacheBoxName, 'a'), isNull);
      expect(storage.get<int>(LocalStorage.cacheBoxName, 'b'), isNull);
    });
  });

  group('Cache helpers', () {
    test('cacheJson and getCachedJson round-trip', () async {
      final data = {'nodes': 5, 'status': 'ok'};
      await storage.cacheJson('cluster_snapshot', data);

      final retrieved = storage.getCachedJson('cluster_snapshot');
      expect(retrieved, isA<Map>());
      expect(retrieved['nodes'], 5);
      expect(retrieved['status'], 'ok');
    });

    test('clearCache removes all cached data', () async {
      await storage.cacheJson('key1', 'value1');
      await storage.clearCache();

      expect(storage.getCachedJson('key1'), isNull);
    });
  });
}
