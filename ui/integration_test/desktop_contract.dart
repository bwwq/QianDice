// Stable desktop behavior: real API, navigation state, resize, theme, isolated replies.
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:qianbian_ui/main.dart' as app;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('desktop preserves draft and replies through isolated API', (
    tester,
  ) async {
    final backendPath = Platform.environment['QIANBIAN_TEST_BINARY'];
    if (backendPath == null)
      throw StateError('QIANBIAN_TEST_BINARY must select an existing backend');
    final root = await Directory.systemTemp.createTemp('千变 Fluent 验收 ');
    final portProbe = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = portProbe.port;
    await portProbe.close();
    final backend = await Process.start(backendPath, [
      '--root',
      root.path,
      '--listen',
      '127.0.0.1:$port',
    ]);
    final diagnostics = <String>[];
    final stderr = backend.stderr
        .transform(utf8.decoder)
        .listen(diagnostics.add);
    final stdout = backend.stdout.drain<void>();
    app.Api? api;
    final screenshot = GlobalKey();
    Future<void> until(Finder finder) async {
      for (var attempt = 0; attempt < 60; attempt++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 100)),
        );
        await tester.pump();
        if (finder.evaluate().isNotEmpty) return;
      }
      throw StateError('Desktop did not reach expected state: $finder');
    }

    Future<void> capture(String name) async {
      await tester.pumpAndSettle();
      final boundary =
          screenshot.currentContext!.findRenderObject()!
              as RenderRepaintBoundary;
      final image = await boundary.toImage();
      final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
      final folder = Directory(
        Platform.environment['QIANBIAN_UI_REPORT_DIR'] ?? '../dist',
      );
      await folder.create(recursive: true);
      await File(
        '${folder.path}/$name.png',
      ).writeAsBytes(bytes!.buffer.asUint8List());
      image.dispose();
    }

    try {
      for (var attempt = 0; attempt < 100; attempt++) {
        try {
          final token = (await File(
            '${root.path}/data/config/admin-token.txt',
          ).readAsString()).trim();
          api = app.Api('http://127.0.0.1:$port', token);
          await api.request('/status');
          break;
        } catch (_) {
          api = null;
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }
      }
      expect(api, isNotNull, reason: diagnostics.join());
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      await tester.pumpWidget(
        RepaintBoundary(
          key: screenshot,
          child: app.QianbianApp(api: api!),
        ),
      );
      await until(find.byType(NavigationView));
      await tester.pumpAndSettle();
      await capture('fluent-overview');

      await tester.tap(find.text('模拟聊天').hitTestable().first);
      await tester.pumpAndSettle();
      final input = find.byWidgetPredicate(
        (widget) =>
            widget is TextBox && widget.placeholder == '输入指令，如 .r 1d100',
      );
      await tester.enterText(input, '.r 1d1 未发送草稿');
      await tester.tap(find.text('设置').hitTestable().first);
      await tester.pumpAndSettle();
      await tester.tap(find.byType(ComboBox<ThemeMode>).hitTestable());
      await tester.pumpAndSettle();
      await tester.tap(find.text('深色').hitTestable().last);
      await tester.pumpAndSettle();
      tester.view.physicalSize = const Size(700, 800);
      await tester.pumpAndSettle();
      tester.view.physicalSize = const Size(1280, 800);
      await tester.pumpAndSettle();
      await tester.tap(find.text('模拟聊天').hitTestable().first);
      await tester.pumpAndSettle();
      expect(tester.widget<TextBox>(input).controller!.text, '.r 1d1 未发送草稿');
      await tester.tap(input);
      await tester.pumpAndSettle();
      await tester.enterText(input, '.r 1d1');
      await tester.pumpAndSettle();
      expect(tester.widget<TextBox>(input).controller!.text, '.r 1d1');
      await tester.tap(find.text('发送').hitTestable());
      await until(find.textContaining('1d1 = 1'));
      expect(find.textContaining('未发送草稿'), findsNothing);
      final worlds = await api.request('/worlds') as List;
      expect(worlds, isNotEmpty);
      expect(
        worlds.every(
          (world) => jsonDecode(world['key'] as String)[0] == 'simulation',
        ),
        isTrue,
      );
      expect(tester.takeException(), isNull);
      await capture('fluent-dark-chat');
    } finally {
      await tester.pumpWidget(const SizedBox.shrink());
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
      try {
        await api?.request('/shutdown', method: 'POST');
      } catch (_) {}
      try {
        await backend.exitCode.timeout(const Duration(seconds: 10));
      } catch (_) {
        backend.kill();
        await backend.exitCode;
      }
      await stderr.cancel();
      await stdout;
      await root.delete(recursive: true);
    }
  });
}
