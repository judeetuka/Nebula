import '../../../../core/usecases/usecase.dart';
import '../entities/browser_session.dart';
import '../repositories/browser_repository.dart';

class CreateSessionParams {
  final String? initialUrl;

  const CreateSessionParams({this.initialUrl});
}

class CreateSession extends UseCase<BrowserSession, CreateSessionParams> {
  final BrowserRepository repository;

  CreateSession(this.repository);

  @override
  Future<BrowserSession> call(CreateSessionParams params) {
    return repository.createSession(initialUrl: params.initialUrl);
  }
}
