class Failure implements Exception {
  final String message;

  const Failure(this.message);

  @override
  String toString() => message;
}

class AuthFailure extends Failure {
  const AuthFailure(super.message);
}

class ClusterFailure extends Failure {
  const ClusterFailure(super.message);
}

class ServerFailure extends Failure {
  const ServerFailure(super.message);
}

class WorkflowFailure extends Failure {
  const WorkflowFailure(super.message);
}
