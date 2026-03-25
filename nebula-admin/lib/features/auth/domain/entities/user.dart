class User {
  final String uid;
  final String email;
  final String? displayName;
  final String? photoUrl;
  final String role;

  const User({
    required this.uid,
    required this.email,
    this.displayName,
    this.photoUrl,
    this.role = 'viewer',
  });

  bool get isAdmin => role == 'admin';
  bool get isOperator => role == 'operator' || isAdmin;
}
