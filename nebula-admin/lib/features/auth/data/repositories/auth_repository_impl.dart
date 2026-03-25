import '../../../../core/storage/local_storage.dart';
import '../../domain/entities/user.dart';
import '../../domain/repositories/auth_repository.dart';
import '../datasources/auth_remote_source.dart';

class AuthRepositoryImpl implements AuthRepository {
  final AuthRemoteSource remoteSource;
  final LocalStorage storage;

  const AuthRepositoryImpl(this.remoteSource, this.storage);

  @override
  Future<User> signIn({required String email, required String password}) {
    return remoteSource.signIn(email: email, password: password);
  }

  @override
  Future<void> signOut() async {
    await remoteSource.signOut();
    await storage.clearAuth();
  }

  @override
  Future<User?> getCurrentUser() {
    return remoteSource.getCurrentUser();
  }
}
