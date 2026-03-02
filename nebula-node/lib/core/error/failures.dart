abstract class Failure {
  final String message;
  const Failure(this.message);
}

class EngineFailure extends Failure {
  const EngineFailure(super.message);
}

class ScanFailure extends Failure {
  const ScanFailure(super.message);
}

class BrowserFailure extends Failure {
  const BrowserFailure(super.message);
}
