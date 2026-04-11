import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

/// URL input bar with navigation controls and loading progress indicator.
/// Supports typing a URL or search query, with inline progress feedback.
class UrlBar extends StatefulWidget {
  final String currentUrl;
  final bool isLoading;
  final double loadProgress;
  final ValueChanged<String> onUrlSubmitted;
  final VoidCallback? onStopLoading;

  const UrlBar({
    super.key,
    required this.currentUrl,
    required this.isLoading,
    required this.loadProgress,
    required this.onUrlSubmitted,
    this.onStopLoading,
  });

  @override
  State<UrlBar> createState() => _UrlBarState();
}

class _UrlBarState extends State<UrlBar> {
  late TextEditingController _controller;
  final FocusNode _focusNode = FocusNode();
  bool _isEditing = false;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.currentUrl);
    _focusNode.addListener(_onFocusChange);
  }

  @override
  void didUpdateWidget(UrlBar oldWidget) {
    super.didUpdateWidget(oldWidget);
    // Only update the text if we are NOT currently editing.
    if (!_isEditing && widget.currentUrl != oldWidget.currentUrl) {
      _controller.text = widget.currentUrl;
    }
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocusChange);
    _focusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _onFocusChange() {
    setState(() {
      _isEditing = _focusNode.hasFocus;
    });
    if (_focusNode.hasFocus) {
      _controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: _controller.text.length,
      );
    }
  }

  void _handleSubmit(String value) {
    final trimmed = value.trim();
    if (trimmed.isEmpty) return;

    // Simple heuristic: if it looks like a URL, use it directly.
    // Otherwise, treat it as a search query.
    final url = _normalizeInput(trimmed);
    widget.onUrlSubmitted(url);
    _focusNode.unfocus();
  }

  String _normalizeInput(String input) {
    if (input.startsWith('http://') || input.startsWith('https://')) {
      return input;
    }
    // If it contains a dot and no spaces, treat as a URL.
    if (input.contains('.') && !input.contains(' ')) {
      return 'https://$input';
    }
    // Otherwise, Google search.
    return 'https://www.google.com/search?q=${Uri.encodeComponent(input)}';
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          height: 44,
          padding: const EdgeInsets.symmetric(
            horizontal: UIConstants.spacingSM,
          ),
          child: Row(
            children: [
              // Lock/globe icon
              Icon(
                widget.currentUrl.startsWith('https')
                    ? Icons.lock_outline
                    : Icons.public,
                size: UIConstants.iconSM,
                color: widget.currentUrl.startsWith('https')
                    ? Colors.green
                    : theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: UIConstants.spacingSM),

              // URL text field
              Expanded(
                child: TextField(
                  controller: _controller,
                  focusNode: _focusNode,
                  onSubmitted: _handleSubmit,
                  textInputAction: TextInputAction.go,
                  style: theme.textTheme.bodyMedium,
                  decoration: InputDecoration(
                    isDense: true,
                    contentPadding: const EdgeInsets.symmetric(
                      vertical: 8,
                    ),
                    border: InputBorder.none,
                    hintText: 'Search or enter URL',
                    hintStyle: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant
                          .withValues(alpha: 0.6),
                    ),
                  ),
                ),
              ),

              // Stop / Reload inline button
              if (widget.isLoading)
                GestureDetector(
                  onTap: widget.onStopLoading,
                  child: Icon(
                    Icons.close,
                    size: UIConstants.iconSM,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
            ],
          ),
        ),

        // Loading progress bar
        if (widget.isLoading)
          LinearProgressIndicator(
            value: widget.loadProgress > 0 ? widget.loadProgress : null,
            minHeight: 2,
            backgroundColor: Colors.transparent,
            color: theme.colorScheme.primary,
          ),
      ],
    );
  }
}
