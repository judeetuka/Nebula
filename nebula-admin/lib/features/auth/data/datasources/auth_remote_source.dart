import '../../../../core/error/failures.dart';
import '../../domain/entities/user.dart';

abstract class AuthRemoteSource {
  Future<User> signIn({required String email, required String password});
  Future<void> signOut();
  Future<User?> getCurrentUser();
}

class AuthRemoteSourceStub implements AuthRemoteSource {
  User? _currentUser;

  @override
  Future<User> signIn({required String email, required String password}) async {
    await Future<void>.delayed(const Duration(milliseconds: 500));

    // DEV ONLY: This stub is never used in production builds.
    // Production uses AuthApiSource which talks to the server JWT endpoint.
    if (email == 'dev@nebula.local' &&
        password == 'dev-only-not-for-production') {
      _currentUser = User(
        uid: 'usr_001',
        email: email,
        displayName: 'NEBULA Admin',
      );
      return _currentUser!;
    }

    throw const AuthFailure('Invalid email or password');
  }

  @override
  Future<void> signOut() async {
    await Future<void>.delayed(const Duration(milliseconds: 200));
    _currentUser = null;
  }

  @override
  Future<User?> getCurrentUser() async {
    return _currentUser;
  }
}
