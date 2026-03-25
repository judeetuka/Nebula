import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../../../core/error/failures.dart';
import '../../../../core/storage/local_storage.dart';
import '../../domain/entities/user.dart';
import 'auth_remote_source.dart';

/// REST API implementation of [AuthRemoteSource].
///
/// Authenticates via `POST /api/auth/login` and stores the JWT token in
/// [LocalStorage]. Subsequent calls to [getCurrentUser] hit
/// `GET /api/auth/me` using the stored token.
class AuthApiSource implements AuthRemoteSource {
  final String baseUrl;
  final LocalStorage _storage;
  final http.Client _client;

  AuthApiSource({
    required this.baseUrl,
    required LocalStorage storage,
    http.Client? client,
  }) : _storage = storage,
       _client = client ?? http.Client();

  @override
  Future<User> signIn({required String email, required String password}) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/auth/login'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'email': email, 'password': password}),
    );

    if (response.statusCode != 200) {
      final body = _tryDecodeBody(response.body);
      final message =
          body?['error'] as String? ?? 'Login failed (${response.statusCode})';
      throw AuthFailure(message);
    }

    final json = jsonDecode(response.body) as Map<String, dynamic>;
    final token = json['token'] as String? ?? '';
    if (token.isEmpty) {
      throw const AuthFailure('Server returned no token');
    }

    await _storage.setJwtToken(token);

    // Extract user from response or fetch via /me
    if (json.containsKey('user')) {
      final u = json['user'] as Map<String, dynamic>;
      final user = _parseUser(u);
      await _cacheUser(user);
      return user;
    }

    return getCurrentUser().then((u) {
      if (u == null) throw const AuthFailure('Failed to load user after login');
      return u;
    });
  }

  @override
  Future<void> signOut() async {
    await _storage.clearAuth();
  }

  @override
  Future<User?> getCurrentUser() async {
    final token = _storage.jwtToken;
    if (token == null || token.isEmpty) return null;

    final response = await _client.get(
      Uri.parse('$baseUrl/api/auth/me'),
      headers: {'Authorization': 'Bearer $token'},
    );

    if (response.statusCode == 401) {
      // Token expired / invalid — clear stored auth
      await _storage.clearAuth();
      return null;
    }

    if (response.statusCode != 200) {
      return _cachedUser();
    }

    final json = jsonDecode(response.body) as Map<String, dynamic>;
    final user = _parseUser(json);
    await _cacheUser(user);
    return user;
  }

  // --- Helpers ---

  User _parseUser(Map<String, dynamic> json) {
    return User(
      uid: (json['uid'] ?? json['id'] ?? '') as String,
      email: (json['email'] ?? '') as String,
      displayName: json['display_name'] as String? ?? json['name'] as String?,
      role: (json['role'] ?? 'viewer') as String,
    );
  }

  Future<void> _cacheUser(User user) async {
    await _storage.set(
      LocalStorage.authBoxName,
      LocalStorage.keyUserUid,
      user.uid,
    );
    await _storage.set(
      LocalStorage.authBoxName,
      LocalStorage.keyUserEmail,
      user.email,
    );
    await _storage.set(
      LocalStorage.authBoxName,
      LocalStorage.keyUserDisplayName,
      user.displayName,
    );
    await _storage.set(
      LocalStorage.authBoxName,
      LocalStorage.keyUserRole,
      user.role,
    );
  }

  User? _cachedUser() {
    final uid = _storage.get<String>(
      LocalStorage.authBoxName,
      LocalStorage.keyUserUid,
    );
    if (uid == null) return null;
    return User(
      uid: uid,
      email:
          _storage.get<String>(
            LocalStorage.authBoxName,
            LocalStorage.keyUserEmail,
          ) ??
          '',
      displayName: _storage.get<String>(
        LocalStorage.authBoxName,
        LocalStorage.keyUserDisplayName,
      ),
      role:
          _storage.get<String>(
            LocalStorage.authBoxName,
            LocalStorage.keyUserRole,
          ) ??
          'viewer',
    );
  }

  Map<String, dynamic>? _tryDecodeBody(String body) {
    try {
      return jsonDecode(body) as Map<String, dynamic>;
    } catch (_) {
      return null;
    }
  }
}
