import '../../../../core/usecases/usecase.dart';
import '../repositories/browser_repository.dart';

class GetPageContent extends UseCase<String?, String> {
  final BrowserRepository repository;

  GetPageContent(this.repository);

  @override
  Future<String?> call(String sessionId) {
    return repository.getPageContent(sessionId);
  }
}
