import '../../domain/entities/user.dart';
import '../../domain/repositories/auth_repository.dart';
import '../datasources/auth_remote_source.dart';

class AuthRepositoryImpl implements AuthRepository {
  final AuthRemoteSource remoteSource;

  const AuthRepositoryImpl(this.remoteSource);

  @override
  Future<User> signIn({required String email, required String password}) {
    return remoteSource.signIn(email: email, password: password);
  }

  @override
  Future<void> signOut() {
    return remoteSource.signOut();
  }

  @override
  Future<User?> getCurrentUser() {
    return remoteSource.getCurrentUser();
  }
}
