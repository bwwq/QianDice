import 'dart:io';
Future<(String, String)> connection(List<String> args) async {
  String endpoint = 'http://127.0.0.1:9610';
  String token = '';
  for (var i = 0; i < args.length - 1; i++) {
    if (args[i] == '--endpoint') endpoint = args[i + 1];
    if (args[i] == '--data-dir') {
      final file = File('${args[i + 1]}${Platform.pathSeparator}config${Platform.pathSeparator}admin-token.txt');
      if (await file.exists()) token = (await file.readAsString()).trim();
    }
  }
  return (endpoint, token);
}
