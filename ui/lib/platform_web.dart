import 'dart:typed_data';
import 'dart:js_interop';
import 'package:web/web.dart' as web;
Future<(String, String)> connection(List<String> args) async => ('', '');
Future<bool> save(Uint8List bytes, String name, String mime) async {
  final blob = web.Blob(<JSAny>[bytes.toJS].toJS, web.BlobPropertyBag(type: mime));
  final url = web.URL.createObjectURL(blob);
  final link = web.HTMLAnchorElement()..href = url..download = name;
  web.document.body?.append(link);
  link.click();
  link.remove();
  Future<void>.delayed(const Duration(seconds: 1), () => web.URL.revokeObjectURL(url));
  return true;
}
