import 'package:firebase_auth/firebase_auth.dart' as fb;

import '../../../../core/error/failures.dart';
import '../../domain/entities/user.dart';
import 'auth_remote_source.dart';

/// Firebase Auth implementation of [AuthRemoteSource].
///
/// Delegates sign-in, sign-out, and session queries to [fb.FirebaseAuth].
/// Converts Firebase user objects to the domain [User] entity.
class AuthFirebaseSource implements AuthRemoteSource {
  final fb.FirebaseAuth _auth;

  AuthFirebaseSource({fb.FirebaseAuth? auth})
      : _auth = auth ?? fb.FirebaseAuth.instance;

  @override
  Future<User> signIn({
    required String email,
    required String password,
  }) async {
    try {
      final credential = await _auth.signInWithEmailAndPassword(
        email: email,
        password: password,
      );
      final fbUser = credential.user;
      if (fbUser == null) {
        throw const AuthFailure('Sign-in succeeded but no user returned');
      }
      return User(
        uid: fbUser.uid,
        email: fbUser.email ?? email,
        displayName: fbUser.displayName,
        photoUrl: fbUser.photoURL,
      );
    } on fb.FirebaseAuthException catch (e) {
      throw AuthFailure(e.message ?? e.code);
    }
  }

  @override
  Future<void> signOut() async {
    await _auth.signOut();
  }

  @override
  Future<User?> getCurrentUser() async {
    final fbUser = _auth.currentUser;
    if (fbUser == null) return null;
    return User(
      uid: fbUser.uid,
      email: fbUser.email ?? '',
      displayName: fbUser.displayName,
      photoUrl: fbUser.photoURL,
    );
  }
}
