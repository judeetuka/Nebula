import 'package:http/http.dart' as http;

import '../storage/local_storage.dart';

/// HTTP client that automatically injects `Authorization: Bearer <token>`
/// from [LocalStorage] into every request.
///
/// Falls back to a plain request when no token is stored.
class AuthorizedClient extends http.BaseClient {
  final http.Client _inner;
  final LocalStorage _storage;

  AuthorizedClient({required LocalStorage storage, http.Client? inner})
    : _inner = inner ?? http.Client(),
      _storage = storage;

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) {
    final token = _storage.jwtToken;
    if (token != null && token.isNotEmpty) {
      request.headers['Authorization'] = 'Bearer $token';
    }
    return _inner.send(request);
  }
}
